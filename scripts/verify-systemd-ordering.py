#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from collections import defaultdict
from pathlib import Path

ORDER_KEYS = {"After", "Before"}
DEPENDENCY_KEYS = {"Wants", "Requires"}
INSTALL_KEYS = {"WantedBy", "RequiredBy"}
STANDARD_EDGES = (
    ("local-fs.target", "sysinit.target"),
    ("sysinit.target", "basic.target"),
    ("basic.target", "multi-user.target"),
)


class UnitFile:
    def __init__(self, path: Path):
        self.path = path
        self.name = path.name
        self.values: dict[tuple[str, str], list[str]] = defaultdict(list)
        self._parse(path.read_text(encoding="utf-8"))

    def _parse(self, text: str) -> None:
        section = ""
        pending = ""
        for physical in text.splitlines():
            stripped = physical.strip()
            if not stripped or stripped.startswith(("#", ";")):
                continue
            line = pending + stripped
            if line.endswith("\\"):
                pending = line[:-1] + " "
                continue
            pending = ""
            if line.startswith("[") and line.endswith("]"):
                section = line[1:-1]
                continue
            if "=" not in line or not section:
                raise ValueError(f"{self.path}: malformed line {line!r}")
            key, value = line.split("=", 1)
            self.values[(section, key)].extend(value.split())
        if pending:
            raise ValueError(f"{self.path}: unterminated continuation")

    def words(self, section: str, key: str) -> set[str]:
        return set(self.values.get((section, key), []))

    def scalar(self, section: str, key: str) -> str | None:
        values = self.values.get((section, key), [])
        return values[-1] if values else None


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def require_members(actual: set[str], expected: set[str], message: str) -> None:
    missing = sorted(expected - actual)
    require(not missing, f"{message}: missing {', '.join(missing)}")


def verify_contract(units: dict[str, UnitFile]) -> None:
    required_names = {
        "rigos-state.service",
        "rigos-recovery-access.service",
        "rigos-state-ready.service",
        "rigos-profile-apply.service",
        "rigos-firstboot.service",
        "rigos-hugepages.service",
        "rigos-miner.service",
    }
    require_members(set(units), required_names, "RIGOS unit set is incomplete")

    state = units["rigos-state.service"]
    require(
        state.scalar("Unit", "DefaultDependencies") == "no",
        "state service must remain outside normal default dependencies",
    )
    require_members(
        state.words("Unit", "Before"),
        {"local-fs.target", "rigos-state-ready.service"},
        "state service ordering is incomplete",
    )
    require_members(
        state.words("Install", "WantedBy"),
        {"local-fs.target"},
        "state service must be pulled into local-fs",
    )

    recovery = units["rigos-recovery-access.service"]
    require_members(
        recovery.words("Unit", "After"),
        {"rigos-state.service"},
        "recovery access must follow state mount",
    )
    require_members(
        recovery.words("Unit", "Before"),
        {"rigos-state-ready.service", "rigos-firstboot.service"},
        "recovery access ordering is incomplete",
    )
    require(
        recovery.scalar("Service", "TTYVHangup") != "yes",
        "recovery access must not hang up tty1",
    )

    ready = units["rigos-state-ready.service"]
    require(
        ready.scalar("Unit", "DefaultDependencies") != "no",
        "state-ready must use normal boot dependencies",
    )
    require_members(
        ready.words("Unit", "After"),
        {"rigos-state.service", "rigos-recovery-access.service"},
        "state-ready ordering is incomplete",
    )
    require_members(
        ready.words("Unit", "Requires"),
        {"rigos-state.service"},
        "state-ready must require state mount",
    )
    require_members(
        ready.words("Unit", "Before"),
        {
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
    require_members(
        ready.words("Install", "WantedBy"),
        {"multi-user.target"},
        "state-ready must be installed under multi-user",
    )

    firstboot = units["rigos-firstboot.service"]
    require_members(
        firstboot.words("Unit", "Requires"),
        {"rigos-state-ready.service"},
        "firstboot must require state-ready",
    )
    require(
        firstboot.scalar("Service", "TTYVHangup") != "yes",
        "firstboot must not hang up tty1",
    )


def build_graph(units: dict[str, UnitFile]) -> dict[str, set[str]]:
    graph: dict[str, set[str]] = defaultdict(set)
    for source, destination in STANDARD_EDGES:
        graph[source].add(destination)

    for name, unit in units.items():
        if unit.scalar("Unit", "DefaultDependencies") != "no":
            graph["basic.target"].add(name)
        for dependency in unit.words("Unit", "After"):
            graph[dependency].add(name)
        for dependency in unit.words("Unit", "Before"):
            graph[name].add(dependency)
        for key in INSTALL_KEYS:
            for target in unit.words("Install", key):
                graph[name].add(target)
        for key in DEPENDENCY_KEYS:
            for dependency in unit.words("Unit", key):
                graph.setdefault(dependency, set())
        for key in ORDER_KEYS:
            for dependency in unit.words("Unit", key):
                graph.setdefault(dependency, set())
        graph.setdefault(name, set())
    return graph


def find_cycle(graph: dict[str, set[str]]) -> list[str] | None:
    state: dict[str, int] = {}
    stack: list[str] = []
    positions: dict[str, int] = {}

    def visit(node: str) -> list[str] | None:
        state[node] = 1
        positions[node] = len(stack)
        stack.append(node)
        for target in sorted(graph[node]):
            if state.get(target, 0) == 0:
                cycle = visit(target)
                if cycle:
                    return cycle
            elif state.get(target) == 1:
                start = positions[target]
                return stack[start:] + [target]
        stack.pop()
        positions.pop(node, None)
        state[node] = 2
        return None

    for node in sorted(graph):
        if state.get(node, 0) == 0:
            cycle = visit(node)
            if cycle:
                return cycle
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "unit_dir",
        nargs="?",
        default="build/usb/includes.chroot/etc/systemd/system",
    )
    args = parser.parse_args()
    unit_dir = Path(args.unit_dir)
    try:
        units = {
            path.name: UnitFile(path)
            for path in sorted(unit_dir.glob("rigos-*.service"))
        }
        verify_contract(units)
        cycle = find_cycle(build_graph(units))
        require(cycle is None, "systemd ordering cycle: " + " -> ".join(cycle or []))
    except (OSError, ValueError) as error:
        print(f"verify-systemd-ordering: {error}", file=sys.stderr)
        return 1
    print("RIGOS systemd ordering verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
