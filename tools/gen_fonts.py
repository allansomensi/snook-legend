#!/usr/bin/env python3
"""Generates the fonts for the scripts DejaVu doesn't cover
(crates/client/assets/noto/): Noto Sans for Arabic, Devanagari, Bengali and
Thai (whole, shaped by the game), and Noto Sans CJK cut down to what the
game needs (Chinese, Japanese and Korean, a weight each for regular and
bold text).

The CJK fonts keep the characters of the game's own texts in those
languages plus the common ones of each (GB 2312 level 1, JIS X 0208 level
1 with the kana, KS X 1001's Hangul), so chat and names read too.

Requirements: `pip install fonttools` and the Noto fonts of Debian/Ubuntu
(`apt install fonts-noto-cjk fonts-noto-core`), or set NOTO_CJK and
NOTO_CORE to the folders with NotoSansCJK-*.ttc and NotoSans*-*.ttf.

    python3 tools/gen_fonts.py

Run it again after a change to the Chinese, Japanese or Korean texts.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLIENT = os.path.join(ROOT, "crates/client")
OUT = os.path.join(CLIENT, "assets/noto")
CJK = os.environ.get("NOTO_CJK", "/usr/share/fonts/opentype/noto")
CORE = os.environ.get("NOTO_CORE", "/usr/share/fonts/truetype/noto")

# (file name, index in NotoSansCJK-*.ttc, the game's language, encoding and
# the byte ranges of its common characters).
CJK_FONTS = [
    ("NotoSansSC", 2, "zh-CN", "gb2312", [(range(0xA1, 0xAA), range(0xA1, 0xFF)), (range(0xB0, 0xD8), range(0xA1, 0xFF))]),
    ("NotoSansJP", 0, "ja", "euc_jp", [(range(0xA1, 0xA9), range(0xA1, 0xFF)), (range(0xB0, 0xD0), range(0xA1, 0xFF))]),
    ("NotoSansKR", 1, "ko", "euc_kr", [(range(0xA1, 0xAD), range(0xA1, 0xFF)), (range(0xB0, 0xC9), range(0xA1, 0xFF))]),
]

SHAPED = ["Arabic", "Devanagari", "Bengali", "Thai"]


def common(encoding, ranges):
    out = set()
    for hi, lo in ranges:
        for a in hi:
            for b in lo:
                try:
                    out.add(bytes([a, b]).decode(encoding))
                except UnicodeDecodeError:
                    pass
    return out


def game_text(lang):
    """Every character of the game's texts in `lang`."""
    files = [
        os.path.join(CLIENT, "lang", f"{lang}.txt"),
        os.path.join(CLIENT, "whatsnew", f"{lang}.md"),
        os.path.join(CLIENT, "rules", "brazilian", f"{lang}.md"),
        os.path.join(CLIENT, "rules", "official", f"{lang}.md"),
    ]
    chars = set()
    for f in files:
        if os.path.exists(f):
            with open(f, encoding="utf-8") as fh:
                chars |= set(fh.read())
    return chars


def subset(src, out, extra_args):
    subprocess.run(
        ["pyftsubset", src, f"--output-file={out}", "--no-hinting", "--name-IDs=*", "--name-languages=*", *extra_args],
        check=True,
    )
    print(f"{os.path.relpath(out, ROOT)}: {os.path.getsize(out) // 1024} KiB")


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, index, lang, encoding, ranges in CJK_FONTS:
        chars = common(encoding, ranges) | game_text(lang)
        # Fullwidth forms and CJK punctuation, whatever the encoding has.
        chars |= {chr(c) for c in range(0x3000, 0x3040)} | {chr(c) for c in range(0xFF01, 0xFF5F)}
        chars = {c for c in chars if ord(c) > 0x2E7F}
        text = os.path.join(OUT, f".{name}.txt")
        with open(text, "w", encoding="utf-8") as fh:
            fh.write("".join(sorted(chars)))
        for weight in ["Regular", "Bold"]:
            src = os.path.join(CJK, f"NotoSansCJK-{weight}.ttc")
            subset(src, os.path.join(OUT, f"{name}-{weight}.otf"), [f"--font-number={index}", f"--text-file={text}", "--layout-features="])
        os.remove(text)
    for script in SHAPED:
        for weight in ["Regular", "Bold"]:
            src = os.path.join(CORE, f"NotoSans{script}-{weight}.ttf")
            subset(src, os.path.join(OUT, f"NotoSans{script}-{weight}.ttf"), ["--unicodes=*", "--layout-features=*"])


if __name__ == "__main__":
    sys.exit(main())
