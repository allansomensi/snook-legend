#!/usr/bin/env python3
"""Checks that the repository is ready to be published as its version: what
the "Publish a version" workflow runs before it tags a commit of main
(.github/workflows/publish.yml), and what to run before pushing a release
to main:

    python3 tools/release_check.py

    python3 tools/release_check.py 0.13.0    # and that it is this version

It fails (exit status 1, saying everything that is wrong) unless:

- the version in Cargo.toml ([workspace.package]) is a SemVer version, and
  Cargo.lock has every crate of the workspace at it;
- it is newer than every version tagged so far (v*), and isn't tagged yet;
- CHANGELOG.md opens with its section ("## <version>", nothing "Not
  released yet" above it);
- "What's new" (crates/client/whatsnew/*.md) opens with it in every
  language, with as many highlights as in English, and the Store's notes
  of every language can be written from it (tools/msstore_submit.py);
- the Flatpak's MetaInfo has it as its first release (tools/gen_metainfo.py).

On success it prints the version alone (the workflow reads it).
"""

import os
import re
import subprocess
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))

SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$")
CRATES = ["snook-legend", "snook_legend_core", "snook_legend_proto", "snook_legend_server"]
METAINFO = os.path.join(ROOT, "flatpak", "app.snooklegend.SnookLegend.metainfo.xml")


def order(version):
    """A version's place among the others (a pre-release before its version)."""
    m = SEMVER.match(version)
    pre = m.group(4)
    parts = tuple(int(x) for x in m.groups()[:3])
    if pre is None:
        return parts + (1, ())
    keys = tuple((0, int(p), "") if p.isdigit() else (1, 0, p) for p in pre.split("."))
    return parts + (0, keys)


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def check(wanted):
    problems = []

    with open(os.path.join(ROOT, "Cargo.toml"), "rb") as f:
        version = tomllib.load(f)["workspace"]["package"]["version"]
    if wanted and wanted.removeprefix("v") != version:
        problems.append(f"Cargo.toml says {version}, not {wanted}")
    if not SEMVER.match(version):
        return version, [f"Cargo.toml's version {version!r} isn't a SemVer version (1.2.3)"]

    with open(os.path.join(ROOT, "Cargo.lock"), "rb") as f:
        lock = tomllib.load(f)
    for name in CRATES:
        found = [p["version"] for p in lock.get("package", []) if p["name"] == name]
        if found != [version]:
            problems.append(f"Cargo.lock has {name} at {found or 'nothing'}, not {version} (run cargo build)")

    tags = [t for t in git("tag", "--list", "v*").split() if SEMVER.match(t[1:])]
    if f"v{version}" in tags:
        problems.append(f"v{version} is tagged already: raise the version for a new release")
    newer = [t for t in tags if order(t[1:]) >= order(version) and t != f"v{version}"]
    if newer:
        newest = max(newer, key=lambda t: order(t[1:]))
        problems.append(f"{version} isn't newer than {newest}, released already")

    with open(os.path.join(ROOT, "CHANGELOG.md"), encoding="utf-8") as f:
        sections = [line.rstrip() for line in f if line.startswith("## ")]
    first = sections[0] if sections else ""
    if not re.match(rf"^## {re.escape(version)}(\s|$)", first):
        problems.append(f'CHANGELOG.md opens with "{first}", not "## {version}"')

    from gen_metainfo import WHATSNEW, notes

    english = notes("en")
    if not english or english[0][0] != version:
        problems.append(f"whatsnew/en.md opens with {english[0][0] if english else 'nothing'}, not {version}")
    for name in sorted(os.listdir(WHATSNEW)):
        if not name.endswith(".md"):
            continue
        code = name[:-3]
        mine = notes(code)
        if not mine or mine[0][0] != version:
            problems.append(f"whatsnew/{name} opens with {mine[0][0] if mine else 'nothing'}, not {version}")
        elif english and english[0][0] == version and len(mine[0][1]) != len(english[0][1]):
            problems.append(
                f"whatsnew/{name} has {len(mine[0][1])} highlights for {version}, en.md {len(english[0][1])}"
            )

    if not problems:
        import msstore_submit

        for name in sorted(os.listdir(WHATSNEW)):
            if name.endswith(".md"):
                try:
                    text = msstore_submit.release_notes(name[:-3], version)
                except SystemExit as e:
                    problems.append(str(e))
                    continue
                if len(text) > msstore_submit.MAX_NOTES:
                    problems.append(f"the Store's notes of {name} are {len(text)} characters")

    with open(METAINFO, encoding="utf-8") as f:
        releases = re.findall(r'<release version="([^"]+)"', f.read())
    if not releases or releases[0] != version:
        problems.append(
            f"the MetaInfo's first release is {releases[0] if releases else 'none'}, not {version}"
            " (run python3 tools/gen_metainfo.py)"
        )
    return version, problems


def main():
    wanted = sys.argv[1] if len(sys.argv) > 1 else None
    version, problems = check(wanted)
    if problems:
        print(f"Not ready to publish {version}:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        sys.exit(1)
    print(version)


if __name__ == "__main__":
    main()
