#!/usr/bin/env python3
"""Generates the interface's icon font (crates/client/assets/icons.ttf).

Every `Icon` of crates/client/src/icons.rs, in the order the enum lists
them, gets two glyphs: U+E000 + n in the outline style (Phosphor's Bold
weight) and U+E100 + n filled (Phosphor's Fill weight, for a selected or
solid look). The game draws them as text, so they are rasterised at the
exact size they are shown and stay sharp at any interface size. Icons the
game needs and Phosphor doesn't have (the table, a coin, a cue, a ball,
chalk) are drawn here, on Phosphor's 256 grid and in its proportions.
Extra layers (drawn under an icon in another colour) follow at U+E200.

Requirements: `pip install fonttools` and Phosphor's SVGs
(`npm pack @phosphor-icons/core`, MIT): set PHOSPHOR to the package's
`assets` folder.

    PHOSPHOR=~/Downloads/package/assets python3 tools/gen_icons.py

Run it again after adding an icon to the enum (and a line to ICONS here).
"""
import math
import os
import re
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import parse_path

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLIENT = os.path.join(ROOT, "crates/client")
OUT = os.path.join(CLIENT, "assets/icons.ttf")
PHOSPHOR = os.path.expanduser(os.environ.get("PHOSPHOR", "~/Downloads/package/assets"))

UPM = 1000
ASCENT = 800
DESCENT = -200

# --- Drawing on the 256 grid (y down, as in the SVGs) -----------------------


def circle(cx, cy, r, ccw=False):
    k = 0.5522847498 * r
    if not ccw:
        return (
            f"M{cx},{cy - r} C{cx + k},{cy - r} {cx + r},{cy - k} {cx + r},{cy} "
            f"C{cx + r},{cy + k} {cx + k},{cy + r} {cx},{cy + r} "
            f"C{cx - k},{cy + r} {cx - r},{cy + k} {cx - r},{cy} "
            f"C{cx - r},{cy - k} {cx - k},{cy - r} {cx},{cy - r} Z"
        )
    return (
        f"M{cx},{cy - r} C{cx - k},{cy - r} {cx - r},{cy - k} {cx - r},{cy} "
        f"C{cx - r},{cy + k} {cx - k},{cy + r} {cx},{cy + r} "
        f"C{cx + k},{cy + r} {cx + r},{cy + k} {cx + r},{cy} "
        f"C{cx + r},{cy - k} {cx + k},{cy - r} {cx},{cy - r} Z"
    )


def ring(cx, cy, r_out, r_in):
    return circle(cx, cy, r_out) + " " + circle(cx, cy, r_in, ccw=True)


def rrect(x, y, w, h, r, ccw=False):
    k = 0.5522847498 * r
    if not ccw:
        return (
            f"M{x + r},{y} L{x + w - r},{y} C{x + w - r + k},{y} {x + w},{y + r - k} {x + w},{y + r} "
            f"L{x + w},{y + h - r} C{x + w},{y + h - r + k} {x + w - r + k},{y + h} {x + w - r},{y + h} "
            f"L{x + r},{y + h} C{x + r - k},{y + h} {x},{y + h - r + k} {x},{y + h - r} "
            f"L{x},{y + r} C{x},{y + r - k} {x + r - k},{y} {x + r},{y} Z"
        )
    return (
        f"M{x + r},{y} C{x + r - k},{y} {x},{y + r - k} {x},{y + r} "
        f"L{x},{y + h - r} C{x},{y + h - r + k} {x + r - k},{y + h} {x + r},{y + h} "
        f"L{x + w - r},{y + h} C{x + w - r + k},{y + h} {x + w},{y + h - r + k} {x + w},{y + h - r} "
        f"L{x + w},{y + r} C{x + w},{y + r - k} {x + w - r + k},{y} {x + w - r},{y} Z"
    )


def rrect_ring(x, y, w, h, r, t):
    return rrect(x, y, w, h, r) + " " + rrect(x + t, y + t, w - 2 * t, h - 2 * t, max(r - t, 1), ccw=True)


def polygon(points, ccw=False):
    """Clockwise on screen (y down), as the circles above; `ccw` for a hole."""
    area = sum(
        points[i][0] * points[(i + 1) % len(points)][1] - points[(i + 1) % len(points)][0] * points[i][1]
        for i in range(len(points))
    )
    if (area < 0) != ccw:
        points = points[::-1]
    return f"M{points[0][0]},{points[0][1]} " + " ".join(f"L{x},{y}" for x, y in points[1:]) + " Z"


def capsule(a, b, w):
    """A stroke from a to b, `w` wide, with round ends."""
    (x1, y1), (x2, y2) = a, b
    dx, dy = x2 - x1, y2 - y1
    n = math.hypot(dx, dy) or 1.0
    ox, oy = -dy / n * w / 2, dx / n * w / 2
    body = polygon([(x1 + ox, y1 + oy), (x2 + ox, y2 + oy), (x2 - ox, y2 - oy), (x1 - ox, y1 - oy)])
    return " ".join([body, circle(x1, y1, w / 2), circle(x2, y2, w / 2)])


def polyline(points, w, closed=False):
    pts = points + ([points[0]] if closed else [])
    return " ".join(capsule(pts[i], pts[i + 1], w) for i in range(len(pts) - 1))


STROKE = 24  # Phosphor Bold's stroke

# The game's own icons: (outline, filled).
TABLE_RAIL = rrect_ring(16, 52, 224, 152, 34, STROKE)
TABLE_FELT_HOLES = " ".join(
    circle(x, y, 15, ccw=True) for x in (40, 128, 216) for y in (76, 180)
)
TABLE_FELT = rrect(40, 76, 176, 104, 12) + " " + TABLE_FELT_HOLES
TABLE_SOLID = TABLE_RAIL + " " + " ".join(circle(x, y, 13) for x in (44, 128, 212) for y in (80, 176))
COIN = circle(128, 128, 108) + " " + circle(128, 128, 80, ccw=True) + " " + circle(128, 128, 60)
COIN_OUTLINE = ring(128, 128, 108, 84) + " " + ring(128, 128, 56, 34)
CUE = (
    polygon([(30, 214), (42, 226), (190, 78), (178, 66)])
    + " " + circle(36, 220, 8.5)
    + " " + polygon([(178, 66), (190, 78), (204, 64), (192, 52)])
    + " " + circle(220, 36, 22)
)
BALL = ring(128, 128, 108, 84) + " " + ring(128, 116, 40, 22)
BALL_FILL = circle(128, 128, 108) + " " + circle(128, 116, 44, ccw=True)
CHALK = polyline([(128, 28), (220, 76), (220, 180), (128, 228), (36, 180), (36, 76)], STROKE, closed=True) + " " + polyline(
    [(36, 76), (128, 124), (220, 76)], STROKE
) + " " + polyline([(128, 124), (128, 228)], STROKE)
CHALK_FILL = polygon([(128, 16), (232, 70), (232, 186), (128, 240), (24, 186), (24, 70)]) + " " + polygon(
    [(128, 108), (62, 74), (128, 40), (194, 74)], ccw=True
)

CUSTOM = {
    "table": (TABLE_RAIL, TABLE_SOLID),
    "coin": (COIN, COIN),
    "cue": (CUE, CUE),
    "ball": (BALL, BALL_FILL),
    "chalk": (CHALK, CHALK_FILL),
}

# Each Icon: a Phosphor name, or "custom:<name>".
ICONS = {
    "Trophy": "trophy",
    "Person": "user",
    "People": "users",
    "Gear": "gear-six",
    "Power": "power",
    "CameraReset": "camera-rotate",
    "Replay": "arrow-counter-clockwise",
    "Plus": "plus",
    "Minus": "minus",
    "Close": "x",
    "Back": "arrow-left",
    "Lock": "lock",
    "Globe": "globe",
    "Swap": "arrows-left-right",
    "Network": "network",
    "Robot": "robot",
    "Medal": "medal",
    "Star": "star",
    "Refresh": "arrows-clockwise",
    "Table": "custom:table",
    "Play": "play",
    "Eye": "eye",
    "Shop": "storefront",
    "Coin": "custom:coin",
    "Crown": "crown-simple",
    "Gift": "gift",
    "Check": "check",
    "Hourglass": "hourglass-medium",
    "AddFriend": "user-plus",
    "Pencil": "pencil-simple",
    "List": "list-bullets",
    "Book": "book-open",
    "Pause": "pause",
    "Clean": "corners-out",
    "ChevronLeft": "caret-left",
    "ChevronRight": "caret-right",
    "RemoveFriend": "user-minus",
    "Block": "prohibit",
    "Flag": "flag",
    "SignOut": "sign-out",
    "Monitor": "monitor",
    "GameController": "game-controller",
    "Mouse": "mouse-simple",
    "Speaker": "speaker-high",
    "Target": "target",
    "Calendar": "calendar-star",
    "Ranking": "ranking",
    "Palette": "palette",
    "Confetti": "confetti",
    "Chat": "chat-circle",
    "Info": "info",
    "Warning": "warning",
    "WifiSlash": "wifi-slash",
    "Cue": "custom:cue",
    "Ball": "custom:ball",
    "Chalk": "custom:chalk",
    "Lamp": "lamp-pendant",
    "Frame": "frame-corners",
    "Ghost": "ghost",
    "Sword": "sword",
    "Lightning": "lightning",
    "Sliders": "sliders-horizontal",
    "Users": "users-three",
    "Bell": "bell",
    "Timer": "timer",
    "Sparkle": "sparkle",
    "TextAa": "text-aa",
    "Key": "key",
    "Envelope": "envelope-simple",
    "Download": "download-simple",
    "Question": "question",
    "Clock": "clock",
    "Handshake": "handshake",
}

# Layers drawn under an icon, in another colour: (name, path).
LAYERS = [("table-felt", TABLE_FELT)]


def enum_names():
    src = open(os.path.join(CLIENT, "src/icons.rs"), encoding="utf-8").read()
    body = re.search(r"pub enum Icon \{(.*?)\n\}", src, re.S).group(1)
    return re.findall(r"^\s{4}([A-Z][A-Za-z]*),", body, re.M)


def phosphor_path(name, weight):
    svg = open(os.path.join(PHOSPHOR, weight, f"{name}-{weight}.svg"), encoding="utf-8").read()
    paths = re.findall(r'<path d="([^"]+)"', svg)
    if not paths or re.search(r"<(circle|rect|line|polyline|polygon|ellipse)\b", svg):
        sys.exit(f"{name}-{weight}: not plain paths")
    return " ".join(paths)


def glyph(d):
    tt = TTGlyphPen(None)
    s = UPM / 256
    # SVG y down from the top of the em; the font's y up from the baseline.
    pen = TransformPen(Cu2QuPen(tt, max_err=0.6, reverse_direction=True), (s, 0, 0, -s, 0, ASCENT))
    parse_path(d, pen)
    return tt.glyph()


def main():
    names = enum_names()
    missing = [n for n in names if n not in ICONS]
    if missing:
        sys.exit(f"no icon for {missing}: add them to ICONS")
    order = [".notdef"]
    glyphs = {".notdef": TTGlyphPen(None).glyph()}
    cmap = {}
    for k, name in enumerate(names):
        source = ICONS[name]
        if source.startswith("custom:"):
            outline, fill = CUSTOM[source[7:]]
        else:
            outline, fill = phosphor_path(source, "bold"), phosphor_path(source, "fill")
        for base, d, suffix in ((0xE000, outline, ""), (0xE100, fill, ".fill")):
            gname = f"{name}{suffix}"
            glyphs[gname] = glyph(d)
            order.append(gname)
            cmap[base + k] = gname
    for k, (name, d) in enumerate(LAYERS):
        glyphs[name] = glyph(d)
        order.append(name)
        cmap[0xE200 + k] = name
    fb = FontBuilder(UPM, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({g: (UPM if g != ".notdef" else 0, 0) for g in order})
    fb.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT)
    fb.setupNameTable({"familyName": "Snook Legend Icons", "styleName": "Regular"})
    fb.setupOS2(sTypoAscender=ASCENT, sTypoDescender=DESCENT, usWinAscent=ASCENT, usWinDescent=-DESCENT)
    fb.setupPost()
    fb.save(OUT)
    print(f"{len(names)} icons, {len(LAYERS)} layers: {os.path.getsize(OUT) // 1024} KB")


if __name__ == "__main__":
    main()
