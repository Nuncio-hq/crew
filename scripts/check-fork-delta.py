#!/usr/bin/env python3
"""Verify every upstream-owned file Crew modified matches an AREA in
docs/crew/fork-delta.json.

"Upstream-owned" = the file exists at the pinned upstream commit
(docs/crew/upstream-buzz.json -> buzzCommit) and differs at HEAD. Files Crew
added are never required to be listed.

Usage:
  scripts/check-fork-delta.py          # CI mode: exit 1 on uncovered files
  scripts/check-fork-delta.py --list   # also print every matched file per area

The pinned commit must be reachable locally:
  git fetch https://github.com/block/buzz.git <buzzCommit>
"""

import fnmatch
import json
import subprocess
import sys


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout


def match(path: str, glob: str) -> bool:
    if glob == "*":
        return "/" not in path
    if glob.endswith("/*"):
        base = glob[:-2]
        return path.startswith(base + "/") and "/" not in path[len(base) + 1 :]
    if glob.endswith("/**"):
        return path.startswith(glob[:-3] + "/")
    return fnmatch.fnmatch(path, glob)


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    root = git("rev-parse", "--show-toplevel").strip()
    pin = json.load(open(f"{root}/docs/crew/upstream-buzz.json"))["buzzCommit"]
    probe = subprocess.run(
        ["git", "cat-file", "-e", f"{pin}^{{commit}}"], capture_output=True
    )
    if probe.returncode != 0:
        print(
            f"fork-delta: pinned upstream commit {pin} not available; run: "
            f"git fetch https://github.com/block/buzz.git {pin}",
            file=sys.stderr,
        )
        return 2

    changed = [
        line
        for line in git(
            "diff",
            "--name-only",
            "--diff-filter=MDT",
            pin,
            "HEAD",
            "--",
            ".",
            ":!docs/crew/**",
            ":!plans/**",
        ).splitlines()
        if line.strip()
    ]
    areas = json.load(open(f"{root}/docs/crew/fork-delta.json"))["areas"]

    covered: dict[str, str] = {}
    for area in areas:
        for path in changed:
            if path not in covered and match(path, area["area"]):
                covered[path] = area["area"]
    missing = [p for p in changed if p not in covered]

    if mode == "--list":
        by: dict[str, list[str]] = {}
        for path, area in covered.items():
            by.setdefault(area, []).append(path)
        for area in areas:
            files = by.get(area["area"], [])
            print(f"{area['area']}  ({len(files)})")
            for f in sorted(files):
                print(f"    {f}")

    if missing:
        print(
            "fork-delta: upstream files modified but matching NO area in "
            "docs/crew/fork-delta.json:",
            file=sys.stderr,
        )
        for p in missing:
            print(f"  {p}", file=sys.stderr)
        print(
            "fork-delta: add the file to an existing area's glob or add a new "
            "area with why/resolve.",
            file=sys.stderr,
        )
        return 1

    print(
        f"fork-delta: OK ({len(changed)} upstream files modified, all covered "
        f"by {len(areas)} areas)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
