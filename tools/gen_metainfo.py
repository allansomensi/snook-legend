#!/usr/bin/env python3
"""Writes the releases of the Flatpak's MetaInfo
(flatpak/app.snooklegend.SnookLegend.metainfo.xml) from the game's
"What's new" (crates/client/whatsnew): every version, newest first, with
its date and a link to its release (whose notes are its section of
CHANGELOG.md), and the latest ones with their highlights in English,
Brazilian Portuguese and Spanish (software centers show those).

The date is the one of the version's tag (`v0.12.1`); a version not tagged
yet (the one about to be released) gets today's.

    python3 tools/gen_metainfo.py

Run it after adding a version to whatsnew/*.md (see docs/RELEASING.md); the
client's tests check that the MetaInfo has every version.
"""
import datetime
import os
import re
import subprocess
import sys
from xml.sax.saxutils import escape

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WHATSNEW = os.path.join(ROOT, "crates", "client", "whatsnew")
METAINFO = os.path.join(ROOT, "flatpak", "app.snooklegend.SnookLegend.metainfo.xml")
RELEASES = "https://github.com/allansomensi/snook-legend/releases/tag/v{version}"
# The notes' languages: the file and the MetaInfo's xml:lang (English is
# the one without).
LANGS = [("en", None), ("pt-BR", "pt_BR"), ("es", "es")]
# How many of the latest versions keep their highlights (older ones are
# a version, a date and a link: the file stays small).
WITH_NOTES = 10


def notes(code):
    """[(version, [(title, text)])], as crates/client/src/whatsnew.rs reads
    them: `## <version>`, then `### <title>` and the lines of its text."""
    versions = []
    item = None
    with open(os.path.join(WHATSNEW, f"{code}.md"), encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith("### "):
                item = [line[4:].strip(), []]
                versions[-1][1].append(item)
            elif line.startswith("## "):
                versions.append((line[3:].strip(), []))
                item = None
            elif line.startswith("#") or not line:
                continue
            elif item is not None:
                item[1].append(line)
    return [(v, [(t, " ".join(text)) for t, text in items]) for v, items in versions]


def tag_date(version):
    try:
        out = subprocess.run(
            ["git", "log", "-1", "--format=%cs", f"v{version}", "--"],
            cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.strip()
    except subprocess.CalledProcessError:
        out = ""
    return out or datetime.date.today().isoformat()


def item_xml(title, text, lang):
    attr = f' xml:lang="{lang}"' if lang else ""
    title = title if title[-1:] in ".!?…:" else title + "."
    return f"          <li{attr}><em>{escape(title)}</em> {escape(text)}</li>"


def releases():
    by_lang = {code: notes(code) for code, _ in LANGS}
    english = by_lang["en"]
    for code, _ in LANGS:
        got = [(v, len(items)) for v, items in by_lang[code]]
        want = [(v, len(items)) for v, items in english]
        if got != want:
            sys.exit(f"whatsnew/{code}.md: not the same versions and highlights as en.md")
    out = []
    for n, (version, items) in enumerate(english):
        kind = "development" if "-" in version else "stable"
        out.append(f'    <release version="{version}" date="{tag_date(version)}" type="{kind}">')
        if n < WITH_NOTES:
            out.append("      <description>")
            out.append("        <ul>")
            for i in range(len(items)):
                for code, lang in LANGS:
                    title, text = by_lang[code][n][1][i]
                    out.append(item_xml(title, text, lang))
            out.append("        </ul>")
            out.append("      </description>")
        out.append(f'      <url type="details">{RELEASES.format(version=version)}</url>')
        out.append("    </release>")
    return "\n".join(out)


def main():
    with open(METAINFO, encoding="utf-8") as f:
        xml = f.read()
    block = re.compile(r"(  <releases>\n).*?(  </releases>)", re.S)
    if not block.search(xml):
        sys.exit(f"{METAINFO}: no <releases> block")
    xml = block.sub(lambda m: m.group(1) + releases() + "\n" + m.group(2), xml, count=1)
    with open(METAINFO, "w", encoding="utf-8") as f:
        f.write(xml)
    print(f"wrote {os.path.relpath(METAINFO, ROOT)}")


if __name__ == "__main__":
    main()
