#!/usr/bin/env python3
"""Checks one language's translation work.

    python3 check.py <code>

Reads out/<code>/ui*.tsv (lines "N<TAB>translation", N from source.tsv) and,
when present, out/<code>/whatsnew.md, out/<code>/rules-brazilian.md and
out/<code>/rules-official.md, and reports every problem found.
"""
import glob
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = "/home/user/snook-legend/crates/client"


def placeholders(s):
    return sorted(re.findall(r"\{([^{}]*)\}", s))


def load_source():
    src = {}
    for line in open(os.path.join(HERE, "source.tsv"), encoding="utf-8"):
        line = line.rstrip("\n")
        if not line:
            continue
        n, en, _es = line.split("\t", 2)
        src[int(n)] = en
    return src


def load_ui(code):
    out, errors = {}, []
    files = sorted(glob.glob(os.path.join(HERE, "out", code, "ui*.tsv")))
    if not files:
        errors.append(f"no out/{code}/ui*.tsv files")
    for f in files:
        for k, line in enumerate(open(f, encoding="utf-8"), 1):
            line = line.rstrip("\n").rstrip("\r")
            if not line.strip():
                continue
            if "\t" not in line:
                errors.append(f"{os.path.basename(f)}:{k}: no tab: {line[:60]!r}")
                continue
            n, tr = line.split("\t", 1)
            if not n.strip().isdigit():
                errors.append(f"{os.path.basename(f)}:{k}: bad number {n!r}")
                continue
            n = int(n)
            if n in out:
                errors.append(f"{os.path.basename(f)}:{k}: entry {n} given twice")
            out[n] = tr
    return out, errors


def check_ui(code):
    src = load_source()
    ui, errors = load_ui(code)
    missing = [n for n in src if n not in ui]
    if missing:
        errors.append(f"missing entries ({len(missing)}): {missing[:40]}{' …' if len(missing) > 40 else ''}")
    for n in sorted(ui):
        if n not in src:
            errors.append(f"entry {n} is not in source.tsv")
            continue
        en, tr = src[n], ui[n]
        if not tr.strip():
            errors.append(f"{n}: empty translation")
        if "\t" in tr:
            errors.append(f"{n}: a tab inside the translation")
        lead = lambda s: s[: len(s) - len(s.lstrip())]
        trail = lambda s: s[len(s.rstrip()):]
        if (lead(tr), trail(tr)) != (lead(en), trail(en)):
            errors.append(f"{n}: spaces at the start or end must be as in the English ({en!r})")
        if placeholders(en) != placeholders(tr):
            errors.append(f"{n}: placeholders {placeholders(en)} → {placeholders(tr)}: {tr}")
        if en.count("\\n") != tr.count("\\n"):
            errors.append(f"{n}: the English has {en.count(chr(92) + 'n')} \\n, the translation {tr.count(chr(92) + 'n')}")
        if tr.startswith("#") and not en.startswith("#"):
            errors.append(f"{n}: starts with # (would read as a comment)")
    return errors, len(ui), len(src)


def sections(text):
    """[(version_or_part_line, [titles])] of a whatsnew/rules file, comments skipped."""
    out = []
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("## "):
            out.append((s[3:].strip(), []))
        elif s.startswith("### "):
            if out:
                out[-1][1].append(s[4:].strip())
        elif s.startswith("#"):
            continue
    return out


def check_md(code, name, en_path, same_headings):
    path = os.path.join(HERE, "out", code, name)
    if not os.path.exists(path):
        return [f"{name}: not written yet"]
    errors = []
    en = sections(open(en_path, encoding="utf-8").read())
    tr_text = open(path, encoding="utf-8").read()
    tr = sections(tr_text)
    if len(en) != len(tr):
        errors.append(f"{name}: {len(en)} '## ' sections in English, {len(tr)} here")
    for (eh, et), (th, tt) in zip(en, tr):
        if same_headings and eh != th:
            errors.append(f"{name}: version heading '## {th}' should be '## {eh}' exactly")
        if len(et) != len(tt):
            errors.append(f"{name}: section '## {eh}' has {len(et)} '### ' items in English, {len(tt)} here")
    # Every item has text under it.
    lines = tr_text.splitlines()
    for k, line in enumerate(lines):
        if line.strip().startswith("### "):
            rest = [l for l in lines[k + 1:] if l.strip()]
            nxt = rest[0].strip() if rest else ""
            if not nxt or nxt.startswith("#"):
                errors.append(f"{name}: '{line.strip()}' has no text under it")
    return errors


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    code = sys.argv[1]
    errors, done, total = check_ui(code)
    errors += check_md(code, "whatsnew.md", f"{REPO}/whatsnew/en.md", True)
    errors += check_md(code, "rules-brazilian.md", f"{REPO}/rules/brazilian/en.md", False)
    errors += check_md(code, "rules-official.md", f"{REPO}/rules/official/en.md", False)
    print(f"{code}: {done}/{total} interface texts")
    for e in errors:
        print("  " + e)
    print("OK" if not errors else f"{len(errors)} problem(s)")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
