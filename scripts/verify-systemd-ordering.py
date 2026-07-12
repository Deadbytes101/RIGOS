#!/usr/bin/env python3
import argparse
import sys
from collections import defaultdict
from pathlib import Path


class Unit:
    def __init__(self, path: Path):
        self.path = path
        self.values = defaultdict(list)
        section = ""
        for raw in path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith(("#", ";")):
                continue
            if line.startswith("[") and line.endswith("]"):
                section = line[1:-1]
            elif section and "=" in line:
                key, value = line.split("=", 1)
                self.values[(section, key)].extend(value.split())
            else:
                raise ValueError(f"{path}: malformed line {line!r}")

    def words(self, section, key):
        return set(self.values[(section, key)])

    def scalar(self, section, key):
        values = self.values[(section, key)]
        return values[-1] if values else None


def require(condition, message):
    if not condition:
        raise ValueError(message)


def includes(actual, expected, message):
    missing = sorted(expected - actual)
    require(not missing, f"{message}: missing {', '.join(missing)}")


def verify(units):
    names = {
        "rigos-state.service",
        "rigos-recovery-access.service",
        "rigos-state-ready.service",
        "rigos-ssh-hostkeys.service",
        "rigos-profile-apply.service",
        "rigos-firstboot.service",
        "rigos-boot-utility.service",
        "rigos-hugepages.service",
        "rigos-miner.service",
    }
    includes(set(units), names, "RIGOS unit set is incomplete")
    state = units["rigos-state.service"]
    require(
        state.scalar("Unit", "DefaultDependencies") == "no",
        "state must retain early boot dependencies",
    )
    includes(
        state.words("Unit", "Before"),
        {"local-fs.target", "rigos-state-ready.service"},
        "state ordering is incomplete",
    )

    recovery = units["rigos-recovery-access.service"]
    includes(
        recovery.words("Unit", "After"),
        {"rigos-state.service"},
        "recovery must follow state",
    )
    includes(
        recovery.words("Unit", "Before"),
        {"rigos-state-ready.service", "rigos-firstboot.service"},
        "recovery ordering is incomplete",
    )
    require(
        recovery.scalar("Service", "TTYVHangup") != "yes",
        "recovery must not hang up tty1",
    )
    require(
        recovery.scalar("Service", "StandardError") == "journal",
        "recovery diagnostics must not write over tty1",
    )
    require(
        recovery.scalar("Service", "TTYVTDisallocate") == "yes",
        "recovery must clear tty1 before handoff",
    )

    ready = units["rigos-state-ready.service"]
    require(
        ready.scalar("Unit", "DefaultDependencies") != "no",
        "state-ready must use normal dependencies",
    )
    includes(
        ready.words("Unit", "After"),
        {"rigos-state.service", "rigos-recovery-access.service"},
        "state-ready ordering is incomplete",
    )
    includes(
        ready.words("Unit", "Requires"),
        {"rigos-state.service"},
        "state-ready must require state",
    )
    includes(
        ready.words("Unit", "Before"),
        {
            "rigos-ssh-hostkeys.service",
            "rigos-profile-apply.service",
            "rigos-firstboot.service",
            "rigos-hugepages.service",
            "rigos-miner.service",
        },
        "state-ready downstream ordering is incomplete",
    )
    require(
        "local-fs.target" not in ready.words("Unit", "Before"),
        "state-ready must not order before local-fs",
    )
    require(
        "local-fs.target" not in ready.words("Install", "WantedBy"),
        "state-ready must not be installed under local-fs",
    )
    includes(
        ready.words("Install", "WantedBy"),
        {"multi-user.target"},
        "state-ready must be installed under multi-user",
    )

    hostkeys = units["rigos-ssh-hostkeys.service"]
    includes(
        hostkeys.words("Unit", "After"),
        {"rigos-state-ready.service"},
        "SSH host-key authority must follow the state readiness attempt",
    )
    includes(
        hostkeys.words("Unit", "Wants"),
        {"rigos-state-ready.service"},
        "SSH host-key authority must request state readiness",
    )
    require(
        "rigos-state-ready.service" not in hostkeys.words("Unit", "Requires"),
        "diagnostic SSH must survive state readiness failure",
    )
    includes(
        hostkeys.words("Unit", "Before"),
        {"ssh.service"},
        "SSH host-key authority must precede sshd",
    )
    require(
        hostkeys.scalar("Service", "ExecStart") == "/usr/lib/rigos/rigos-ssh-hostkeys",
        "SSH host-key authority entrypoint is not exact",
    )
    includes(
        hostkeys.words("Install", "WantedBy"),
        {"multi-user.target"},
        "SSH host-key authority must be enabled under multi-user",
    )

    firstboot = units["rigos-firstboot.service"]
    includes(
        firstboot.words("Unit", "After"),
        {
            "rigos-state.service",
            "rigos-state-ready.service",
        },
        "firstboot ordering is incomplete",
    )
    require(
        "rigos-profile-apply.service" not in firstboot.words("Unit", "After"),
        "firstboot must not wait on profile apply before the initial commit",
    )
    includes(
        firstboot.words("Unit", "Wants"),
        {"rigos-state-ready.service"},
        "firstboot must request state verification",
    )
    require(
        "rigos-state-ready.service" not in firstboot.words("Unit", "Requires"),
        "firstboot diagnostics must survive state readiness failure",
    )
    require(
        "network-online.target" not in firstboot.words("Unit", "After")
        and "network-online.target" not in firstboot.words("Unit", "Wants"),
        "firstboot must remain available offline",
    )
    require(
        firstboot.scalar("Service", "StandardInput") == "tty-force",
        "firstboot must acquire tty1",
    )
    require(
        firstboot.scalar("Service", "TTYPath") == "/dev/tty1",
        "firstboot must use tty1",
    )
    require(
        firstboot.scalar("Service", "TTYVHangup") != "yes",
        "firstboot must not hang up tty1",
    )
    require(
        firstboot.scalar("Service", "StandardOutput") == "tty",
        "firstboot UI must write to tty1",
    )
    require(
        firstboot.scalar("Service", "StandardError") == "journal",
        "firstboot diagnostics must not write over tty1",
    )
    require(
        firstboot.scalar("Unit", "ConditionKernelCommandLine") == "!rigos.utility=1",
        "firstboot must not compete with utility boot mode",
    )

    getty_dropin = (
        Path(firstboot.path)
        .parent.joinpath("getty@tty1.service.d", "rigos-firstboot.conf")
    )
    require(getty_dropin.is_file(), "tty1 getty firstboot drop-in is missing")
    getty = Unit(getty_dropin)
    includes(
        getty.words("Unit", "Wants"),
        {"rigos-firstboot.service"},
        "tty1 getty must queue firstboot",
    )
    includes(
        getty.words("Unit", "After"),
        {"rigos-firstboot.service"},
        "tty1 getty must wait for firstboot to finish or skip",
    )

    profile = units["rigos-profile-apply.service"]
    includes(
        profile.words("Service", "ExecStart"),
        {"/usr/lib/rigos/rigos-config", "profile"},
        "profile apply must use the complete machine profile command",
    )
    require(
        "rigos-firstboot.service" not in profile.words("Unit", "Before"),
        "profile apply must not gate initial firstboot",
    )

    utility = units["rigos-boot-utility.service"]
    require(
        utility.scalar("Unit", "ConditionKernelCommandLine") == "rigos.utility=1",
        "utility console must require the utility boot argument",
    )
    includes(
        utility.words("Unit", "After"),
        {"rigos-recovery-access.service", "rigos-state-ready.service"},
        "utility console ordering is incomplete",
    )
    includes(
        utility.words("Unit", "Wants"),
        {"rigos-recovery-access.service", "rigos-state-ready.service"},
        "utility console must request recovery and state readiness",
    )
    require(
        "rigos-firstboot.service" not in utility.words("Unit", "Conflicts"),
        "utility console must not conflict firstboot out before conditions are evaluated",
    )
    require(
        "rigos-boot-utility.service" not in firstboot.words("Unit", "Conflicts"),
        "firstboot must not use a reciprocal conflict against utility mode",
    )
    require(
        utility.scalar("Service", "ExecStart") == "/usr/local/sbin/rigos-utility",
        "utility console entrypoint is not exact",
    )
    require(
        utility.scalar("Service", "StandardInput") == "tty-force",
        "utility console must acquire tty1",
    )
    require(
        utility.scalar("Service", "TTYPath") == "/dev/tty1",
        "utility console must use tty1",
    )

    reject_enabled_target_conflicts(units, "multi-user.target")


def reject_enabled_target_conflicts(units, target):
    pulled = {
        name
        for name, unit in units.items()
        if target in unit.words("Install", "WantedBy")
    }
    for name in sorted(pulled):
        for conflict in sorted(units[name].words("Unit", "Conflicts")):
            if conflict in pulled:
                raise ValueError(
                    f"{name} conflicts with {conflict} while both are pulled into {target}"
                )


def graph_for(units):
    graph = defaultdict(set)
    graph["local-fs.target"].add("sysinit.target")
    graph["sysinit.target"].add("basic.target")
    graph["basic.target"].add("multi-user.target")
    for name, unit in units.items():
        if unit.scalar("Unit", "DefaultDependencies") != "no":
            graph["basic.target"].add(name)
        for dependency in unit.words("Unit", "After"):
            graph[dependency].add(name)
        for target in unit.words("Unit", "Before"):
            graph[name].add(target)
        for target in unit.words("Install", "WantedBy"):
            graph[name].add(target)
        graph[name]
    return graph


def cycle_in(graph):
    state, stack, positions = {}, [], {}

    def visit(node):
        state[node] = 1
        positions[node] = len(stack)
        stack.append(node)
        for target in sorted(graph[node]):
            if state.get(target, 0) == 0:
                cycle = visit(target)
                if cycle:
                    return cycle
            elif state[target] == 1:
                return stack[positions[target] :] + [target]
        stack.pop()
        positions.pop(node)
        state[node] = 2
        return None

    for node in sorted(graph):
        if state.get(node, 0) == 0:
            cycle = visit(node)
            if cycle:
                return cycle
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "unit_dir",
        nargs="?",
        default="build/usb/includes.chroot/etc/systemd/system",
    )
    directory = Path(parser.parse_args().unit_dir)
    try:
        units = {path.name: Unit(path) for path in directory.glob("rigos-*.service")}
        verify(units)
        cycle = cycle_in(graph_for(units))
        require(cycle is None, "systemd ordering cycle: " + " -> ".join(cycle or []))
    except (OSError, ValueError) as error:
        print(f"verify-systemd-ordering: {error}", file=sys.stderr)
        return 1
    print("RIGOS systemd ordering verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
