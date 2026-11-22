#!/usr/bin/env python3
"""Generates the interface's fonts (crates/client/assets/inter/): Inter,
cut down to the scripts it draws for the game (Latin with Vietnamese, Greek,
Cyrillic, punctuation, arrows and the common symbols), with tabular figures
(every digit as wide as the others) as its plain digits, so a clock or a
score doesn't jiggle as it changes. The other scripts come from Noto
(tools/gen_fonts.py); what neither has, from DejaVu.

Requirements: `pip install fonttools` and Inter 4.x
(https://github.com/rsms/inter/releases): set INTER to the folder with its
static TTFs (extras/ttf in the release's zip).

    INTER=~/Downloads/Inter-4.1/extras/ttf python3 tools/gen_ui_fonts.py
"""
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates/client/assets/inter")
INTER = os.path.expanduser(os.environ.get("INTER", "~/Downloads/Inter/extras/ttf"))

# The text faces (body and bold) and the display ones (big titles: Inter
# Display is drawn for large sizes, its letters a little closer).
FILES = [
    "Inter-Regular.ttf",
    "Inter-SemiBold.ttf",
    "Inter-Bold.ttf",
    "InterDisplay-Bold.ttf",
    "InterDisplay-Black.ttf",
]

RANGES = [
    (0x0020, 0x024F),  # Latin, Latin-1, Latin Extended A and B
    (0x0259, 0x0259),
    (0x02B0, 0x036F),  # modifier letters, combining marks
    (0x0370, 0x03FF),  # Greek
    (0x0400, 0x052F),  # Cyrillic
    (0x1E00, 0x1EFF),  # Latin Extended Additional (Vietnamese)
    (0x2000, 0x206F),  # punctuation
    (0x20A0, 0x20CF),  # currency
    (0x2100, 0x214F),  # letterlike
    (0x2150, 0x218F),  # number forms
    (0x2190, 0x21FF),  # arrows
    (0x2200, 0x22FF),  # mathematical operators
    (0x2300, 0x23FF),  # technical
    (0x2460, 0x24FF),  # enclosed numbers
    (0x2500, 0x25FF),  # box drawing, blocks, shapes
    (0x2600, 0x27BF),  # symbols, dingbats
]


def tabular_figures(font):
    """Maps the digits to their tabular forms (the `tnum` feature, which
    the game can't ask for: it draws text without shaping)."""
    gsub = font["GSUB"].table
    subst = {}
    for record in gsub.FeatureList.FeatureRecord:
        if record.FeatureTag != "tnum":
            continue
        for index in record.Feature.LookupListIndex:
            for st in gsub.LookupList.Lookup[index].SubTable:
                if st.LookupType == 7:
                    st = st.ExtSubTable
                if hasattr(st, "mapping"):
                    subst.update(st.mapping)
    if not subst:
        sys.exit("no tnum feature")
    n = 0
    for table in font["cmap"].tables:
        for code, glyph in list(table.cmap.items()):
            if glyph in subst and chr(code) in "0123456789":
                table.cmap[code] = subst[glyph]
                n += 1
    return n


def main():
    os.makedirs(OUT, exist_ok=True)
    unicodes = [c for a, b in RANGES for c in range(a, b + 1)]
    for name in FILES:
        src = os.path.join(INTER, name)
        if not os.path.exists(src):
            sys.exit(f"missing {src} (set INTER)")
        font = TTFont(src)
        n = tabular_figures(font)
        options = subset.Options()
        options.layout_features = ["kern", "liga", "calt", "ccmp", "locl", "mark", "mkmk"]
        options.hinting = False
        options.name_IDs = ["*"]
        options.notdef_outline = True
        sub = subset.Subsetter(options)
        sub.populate(unicodes=unicodes)
        sub.subset(font)
        dst = os.path.join(OUT, name)
        font.save(dst)
        print(f"{name}: {os.path.getsize(dst) // 1024} KB, {n} digits tabular")


if __name__ == "__main__":
    main()
