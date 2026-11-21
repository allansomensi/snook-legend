#!/usr/bin/env python3
"""Writes one language's translation into the game: lang/<code>.txt (laid out
as lang/pt-BR.txt, comments included), whatsnew/<code>.md and the rules.

    python3 merge.py <code> "<English name of the language>"
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CLIENT = "/home/user/snook-legend/crates/client"


def main():
    code, name = sys.argv[1], sys.argv[2]
    check = subprocess.run([sys.executable, os.path.join(HERE, "check.py"), code])
    if check.returncode != 0:
        sys.exit(f"{code}: the checker found problems")
    sys.path.insert(0, HERE)
    import check as c

    ui, errors = c.load_ui(code)
    assert not errors
    template = open(os.path.join(CLIENT, "lang/pt-BR.txt"), encoding="utf-8").read().split("\n")
    out, n = [], 0
    for k, line in enumerate(template):
        if "\t" not in line:
            if k == 0:
                out.append(f"# Snook Legend: {name}.")
            else:
                out.append(line)
            continue
        n += 1
        en = line.split("\t", 1)[0]
        out.append(f"{en}\t{ui[n]}")
    with open(os.path.join(CLIENT, f"lang/{code}.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(out))
    for src, dst in [
        ("whatsnew.md", f"whatsnew/{code}.md"),
        ("rules-brazilian.md", f"rules/brazilian/{code}.md"),
        ("rules-official.md", f"rules/official/{code}.md"),
    ]:
        text = open(os.path.join(HERE, "out", code, src), encoding="utf-8").read()
        if not text.endswith("\n"):
            text += "\n"
        with open(os.path.join(CLIENT, dst), "w", encoding="utf-8") as f:
            f.write(text)
    print(f"{code}: {n} texts, the rules and what's new written")


if __name__ == "__main__":
    main()
