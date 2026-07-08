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
        "rigos-state.service", "rigos-recovery-access.service",
        "rigos-state-ready.service", "rigos-ssh-hostkeys.service",
        "rigos-profile-apply.service", "rigos-firstboot.service",
        "rigos-hugepages.service", "rigos-miner.service",
    }
    includes(set(units), names, "RIGOS unit set is incomplete")
    state = units["rigos-state.service"]
    require(state.scalar("Unit", "DefaultDependencies") == "no", "state must retain early boot dependencies")
    includes(state.words("Unit", "Before"), {"local-fs.target", "rigos-state-ready.service"}, "state ordering is incomplete")

    recovery = units["rigos-recovery-access.service"]
    includes(recovery.words("Unit", "After"), {"rigos-state.service"}, "recovery must follow state")
    includes(recovery.words("Unit", "Before"), {"rigos-state-ready.service", "rigos-firstboot.service"}, "recovery ordering is incomplete")
    require(recovery.scalar("Service", "TTYVHangup") != "yes", "recovery must not hang up tty1")

    ready = units["rigos-state-ready.service"]
    require(ready.scalar("Unit", "DefaultDependencies") != "no", "state-ready must use normal dependencies")
    includes(ready.words("Unit", "After"), {"rigos-state.service", "rigos-recovery-access.service"}, "state-ready ordering is incomplete")
    includes(ready.words("Unit", "Requires"), {"rigos-state.service"}, "state-ready must require state")
    includes(
        ready.words("Unit", "Before"),
        {
            "rigos-ssh-hostkeys.service", "rigos-profile-apply.service",
            "rigos-firstboot.service", "rigos-hugepages.service",
            "rigos-miner.service",
        },
        "state-ready downstream ordering is incomplete",
    )
    require("local-fs.target" not in ready.words("Unit", "Before"), "state-ready must not order before local-fs")
    require("local-fs.target" not in ready.words("Install", "WantedBy"), "state-ready must not be installed under local-fs")
    includes(ready.words("Install", "WantedBy"), {"multi-user.target"}, "state-ready must be installed under multi-user")

    hostkeys = units["rigos-ssh-hostkeys.service"]
    includes(hostkeys.words("Unit", "After"), {"rigos-state-ready.service"}, "SSH host-key authority must follow state readiness")
    includes(hostkeys.words("Unit", "Requires"), {"rigos-state-ready.service"}, "SSH host-key authority must require state readiness")
    includes(hostkeys.words("Unit", "Before"), {"ssh.service"}, "SSH host-key authority must precede sshd")
    require(
        hostkeys.scalar("Service", "ExecStart") == "/usr/lib/rigos/rigos-ssh-hostkeys",
        "SSH host-key authority entrypoint is not exact",
    )
    includes(hostkeys.words("Install", "WantedBy"), {"multi-user.target"}, "SSH host-key authority must be enabled under multi-user")

    firstboot = units["rigos-firstboot.service"]
    includes(firstboot.words("Unit", "Requires"), {"rigos-state-ready.service"}, "firstboot must require state-ready")
    require(firstboot.scalar("Service", "TTYVHangup") != "yes", "firstboot must not hang up tty1")


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
                return stack[positions[target]:] + [target]
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
    parser.add_argument("unit_dir", nargs="?", default="build/usb/includes.chroot/etc/systemd/system")
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
