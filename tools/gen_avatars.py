#!/usr/bin/env python3
"""Generates the profile-picture atlas (crates/client/assets/avatars.png):
one illustrated picture per code of AVATARS in crates/proto/src/lib.rs, in
that order, drawn here as SVG and rasterised by Chromium.

Requirements: Python 3 with Pillow, and Node.js with the `playwright`
package and its Chromium (`npm i -g playwright && npx playwright install
chromium`; `require('playwright')` must work from any directory).

    python3 tools/gen_avatars.py              # writes the atlas
    python3 tools/gen_avatars.py --keep DIR   # also keeps each SVG, each
                                              # picture at full size and a
                                              # contact sheet in DIR

Each picture is drawn on a 256 px canvas, rasterised at that size and
shrunk to a CELL px cell (so edges are smooth), cut to a disc with clear
corners. The cells go left to right, top to bottom, COLS to a row; the
client (crates/client/src/avatars.rs) knows CELL and COLS. A new picture
gets its code appended to AVATARS and a drawing below (PEOPLE or OBJECTS).
"""
import argparse
import math
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT = os.path.join(ROOT, "crates/client/assets/DejaVuSans-Bold.ttf")
OUT = os.path.join(ROOT, "crates/client/assets/avatars.png")

S = 256  # the canvas of a picture
CELL = 128  # a cell of the atlas
COLS = 8
CX = 128.0
R_DISC = 126.0  # the picture's disc (the client knows it: DISC)
R_IN = 122.0  # inside the rim
FIGURE = 1.1  # how much larger than drawn a person is shown

# ---------------------------------------------------------------- colours


def clamp(v):
    return max(0, min(255, int(round(v))))


def hx(c):
    return "#%02x%02x%02x" % tuple(clamp(v) for v in c[:3])


def shade(c, k):
    """Lighter towards white (k > 0) or darker towards black (k < 0)."""
    if k >= 0:
        return tuple(v + (255 - v) * k for v in c)
    return tuple(v * (1 + k) for v in c)


def warm(c, k):
    """A shadow on skin: darker, and redder than plain black would make it."""
    return (c[0] * (1 - 0.72 * k), c[1] * (1 - 0.98 * k), c[2] * (1 - 1.1 * k))


def mix(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def lum(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


# ------------------------------------------------------------------- svg


def fmt(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def d(cmds, mirror=False, dx=0.0, dy=0.0):
    """A path from commands whose x is measured from the middle line."""
    parts = []
    for c in cmds:
        op, v = c[0], c[1:]
        if op == "Z":
            parts.append("Z")
            continue
        pts = []
        for i in range(0, len(v), 2):
            x = (-v[i] if mirror else v[i]) + CX + dx
            pts.append(f"{fmt(x)} {fmt(v[i + 1] + dy)}")
        parts.append(op + " ".join(pts))
    return " ".join(parts)


def sym(right):
    """A closed shape symmetric about the middle line, from its right half
    (from a point on the line, round to another point on the line)."""
    segs = right[1:]
    pts = [(right[0][1], right[0][2])] + [(s[-2], s[-1]) for s in segs]
    out = list(right)
    for i in range(len(segs) - 1, -1, -1):
        s, (x0, y0) = segs[i], pts[i]
        if s[0] == "C":
            out.append(("C", -s[3], s[4], -s[1], s[2], -x0, y0))
        elif s[0] == "Q":
            out.append(("Q", -s[1], s[2], -x0, y0))
        else:
            out.append(("L", -x0, y0))
    out.append(("Z",))
    return out


def lock_d(p0, p1, p2, w):
    """A strand: a crescent from p0 to p2 bent towards p1, `w` wide in the
    middle, pointed at both ends (x from the middle line)."""
    (x0, y0), (x1, y1), (x2, y2) = p0, p1, p2
    dx, dy = x2 - x0, y2 - y0
    ln = math.hypot(dx, dy) or 1.0
    nx, ny = -dy / ln * w, dx / ln * w
    return d([("M", x0, y0), ("Q", x1 + nx, y1 + ny, x2, y2), ("Q", x1 - nx, y1 - ny, x0, y0), ("Z",)])


def mirror_cmds(cmds):
    return [(c[0],) + tuple(-v if i % 2 == 0 else v for i, v in enumerate(c[1:])) for c in cmds]


def attrs(a):
    out = []
    for k, v in a.items():
        if v is None:
            continue
        if isinstance(v, float):
            v = fmt(v)
        elif isinstance(v, tuple):
            v = hx(v)
        out.append(f'{k.rstrip("_").replace("_", "-")}="{v}"')
    return " ".join(out)


class Svg:
    def __init__(self, uid):
        self.uid = uid.replace("_", "")
        self.defs = []
        self.body = []
        self.k = 0
        self.blurs = {}

    def nid(self, tag):
        self.k += 1
        return f"{self.uid}{tag}{self.k}"

    def add(self, *xs):
        self.body.extend(xs)

    def el(self, tag, **a):
        self.body.append(f"<{tag} {attrs(a)}/>")

    def path(self, dd, fill="none", **a):
        if isinstance(dd, list):
            dd = d(dd)
        self.el("path", d=dd, fill=fill, **a)

    def circle(self, x, y, r, fill, **a):
        self.el("circle", cx=float(x), cy=float(y), r=float(r), fill=fill, **a)

    def ellipse(self, x, y, rx, ry, fill, **a):
        self.el("ellipse", cx=float(x), cy=float(y), rx=float(rx), ry=float(ry), fill=fill, **a)

    def _stops(self, stops):
        out = []
        for st in stops:
            o, c = st[0], st[1]
            a = st[2] if len(st) > 2 else 1.0
            op = f' stop-opacity="{fmt(a)}"' if a < 1 else ""
            out.append(f'<stop offset="{fmt(o)}" stop-color="{hx(c)}"{op}/>')
        return "".join(out)

    def lin(self, x1, y1, x2, y2, stops):
        i = self.nid("l")
        self.defs.append(
            f'<linearGradient id="{i}" gradientUnits="userSpaceOnUse" x1="{fmt(x1)}" '
            f'y1="{fmt(y1)}" x2="{fmt(x2)}" y2="{fmt(y2)}">{self._stops(stops)}</linearGradient>'
        )
        return f"url(#{i})"

    def rad(self, cx, cy, r, stops, fx=None, fy=None, sx=1.0, sy=1.0):
        i = self.nid("r")
        f = f' fx="{fmt(fx)}" fy="{fmt(fy)}"' if fx is not None else ""
        t = ""
        if sx != 1.0 or sy != 1.0:
            t = (
                f' gradientTransform="translate({fmt(cx)} {fmt(cy)}) scale({fmt(sx)} {fmt(sy)}) '
                f'translate({fmt(-cx)} {fmt(-cy)})"'
            )
        self.defs.append(
            f'<radialGradient id="{i}" gradientUnits="userSpaceOnUse" cx="{fmt(cx)}" '
            f'cy="{fmt(cy)}" r="{fmt(r)}"{f}{t}>{self._stops(stops)}</radialGradient>'
        )
        return f"url(#{i})"

    def clip(self, inner):
        if isinstance(inner, list):
            inner = f'<path d="{d(inner)}"/>'
        elif not inner.startswith("<"):
            inner = f'<path d="{inner}"/>'
        i = self.nid("c")
        self.defs.append(f'<clipPath id="{i}">{inner}</clipPath>')
        return f"url(#{i})"

    def blur(self, sd):
        if sd not in self.blurs:
            i = self.nid("b")
            self.defs.append(
                f'<filter id="{i}" x="-50%" y="-50%" width="200%" height="200%">'
                f'<feGaussianBlur stdDeviation="{fmt(sd)}"/></filter>'
            )
            self.blurs[sd] = f"url(#{i})"
        return self.blurs[sd]

    def text(self, x, y, txt, size, fill, **a):
        self.body.append(
            f'<text {attrs(dict(x=float(x), y=float(y), font_family="SLBold", font_size=float(size), text_anchor="middle", fill=fill, **a))}>{txt}</text>'
        )

    def group(self, **a):
        self.body.append(f"<g {attrs(a)}>")

    def end(self):
        self.body.append("</g>")

    def svg(self):
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S} {S}" width="{S}" '
            f'height="{S}"><defs>{"".join(self.defs)}</defs>{"".join(self.body)}</svg>'
        )


def disc_open(s, bg, halo=True):
    """The coloured disc, lighter at the top left; opens the group that
    keeps the drawing inside the rim."""
    s.circle(
        CX,
        CX,
        R_DISC,
        s.rad(
            CX - 30,
            CX - 46,
            200,
            [(0, shade(bg, 0.2)), (0.5, bg), (1, shade(bg, -0.42))],
        ),
    )
    if halo:
        s.ellipse(CX, 104, 80, 70, shade(bg, 0.24), opacity=0.45, filter=s.blur(18))
    s.group(clip_path=s.clip(f'<circle cx="{CX}" cy="{CX}" r="{R_IN}"/>'))


def disc_close(s, bg):
    s.end()
    s.circle(CX, CX, R_DISC - 2.6, "none", stroke=shade(bg, 0.34), stroke_width=5.2)
    s.circle(CX, CX, R_IN - 0.6, "none", stroke=shade(bg, -0.55), stroke_width=1.4, opacity=0.4)


# ---------------------------------------------------------------- people

GOLD = (236, 188, 70)
SILVER = (214, 218, 224)
PEARL = (246, 240, 228)
INK = (34, 22, 18)


def look(bg, skin, hair, hair_color, top, **kw):
    L = dict(
        bg=bg,
        skin=skin,
        hair=hair,
        hair_color=hair_color,
        top=top,
        iris=(70, 50, 34),
        glasses=None,
        beard=None,
        fem=False,
        grin=False,
        age="adult",
        clothes="crew",
        earrings=None,
        freckles=False,
        necklace=None,
    )
    L.update(kw)
    return L


# The look behind each code of AVATARS: the same people as the earlier
# drawings (skin, hair, clothes, glasses, beard and background), drawn anew.
PEOPLE = {
    "sunny": look((40, 130, 130), (246, 210, 180), "wavy", (226, 182, 90), (70, 170, 90),
                  fem=True, grin=True, iris=(60, 120, 150), clothes="scoop", earrings="stud"),
    "marco": look((200, 110, 40), (214, 160, 118), "short", (48, 34, 26), (40, 60, 110),
                  beard="short", clothes="crew"),
    "zuri": look((110, 70, 150), (118, 76, 52), "afro", (28, 22, 20), (235, 190, 50),
                 fem=True, clothes="scoop", earrings="hoop"),
    "kai": look((60, 130, 190), (232, 186, 146), "cap", (60, 40, 28), (40, 100, 200),
                grin=True, cap=(200, 50, 44), clothes="hoodie", age="young"),
    "mei": look((170, 80, 110), (244, 214, 186), "bun", (26, 22, 24), (200, 50, 60),
                fem=True, glasses=("bold", (26, 24, 28)), clothes="turtleneck", earrings="stud"),
    "omar": look((190, 150, 50), (150, 100, 70), "bald", (36, 26, 22), (40, 130, 80),
                 beard="full", grin=True, clothes="polo"),
    "lena": look((40, 50, 100), (248, 216, 196), "long", (176, 70, 36), (120, 70, 170),
                 fem=True, iris=(70, 130, 80), clothes="vneck", freckles=True),
    "theo": look((50, 120, 70), (240, 200, 168), "spiky", (120, 76, 40), (230, 120, 40),
                 glasses=("round", (92, 54, 30)), grin=True, iris=(70, 110, 160), age="young"),
    "nia": look((200, 90, 80), (96, 62, 44), "sleek", (22, 18, 18), (30, 140, 140),
                fem=True, grin=True, clothes="scoop", earrings="hoop"),
    "raul": look((80, 90, 110), (210, 156, 112), "beanie", (52, 36, 26), (170, 40, 40),
                 beard="full", knit=(120, 124, 130), clothes="flannel"),
    "ana": look((60, 150, 120), (222, 176, 136), "messy_bun", (110, 64, 36), (236, 236, 230),
                fem=True, grin=True, clothes="collar"),
    "joe": look((40, 80, 160), (104, 68, 48), "crop", (22, 18, 18), (230, 190, 60),
                glasses=("rect", (24, 22, 24)), clothes="sweater"),
    "iris": look((120, 40, 70), (240, 206, 186), "pixie", (200, 200, 205), (170, 140, 200),
                 fem=True, glasses=("cat", (120, 36, 66)), iris=(80, 110, 150), age="old",
                 clothes="cardigan", earrings="pearl"),
    "sam": look((170, 50, 50), (196, 140, 100), "beanie", (40, 30, 24), (36, 36, 40),
                grin=True, knit=(220, 170, 40), pompom=True, clothes="hoodie", age="young"),
    "hugo": look((40, 100, 60), (246, 212, 184), "swept", (222, 186, 110), (236, 236, 232),
                 beard="short", iris=(70, 120, 170), clothes="collar"),
    "lua": look((70, 60, 150), (140, 94, 66), "afro", (80, 48, 30), (230, 110, 160),
                fem=True, grin=True, clothes="scoop", loose=True),
    "duda": look((230, 120, 60), (236, 196, 160), "ponytail", (90, 52, 30), (60, 90, 170),
                 fem=True, grin=True, clothes="crew", age="young"),
    "bento": look((90, 60, 40), (200, 146, 104), "curly", (40, 28, 22), (60, 140, 90),
                  clothes="crew"),
    "rex": look((30, 30, 40), (236, 200, 170), "mohawk", (40, 160, 200), (30, 30, 34),
                grin=True, clothes="jacket", earrings="ring_right"),
    "yuki": look((90, 150, 200), (246, 220, 196), "bob", (24, 22, 26), (240, 160, 180),
                 fem=True, clothes="scoop", earrings="stud"),
    "malik": look((30, 110, 90), (92, 60, 42), "coils", (20, 16, 16), (220, 120, 40),
                  beard="full", grin=True, clothes="crew"),
    "clara": look((150, 120, 60), (242, 208, 182), "ponytail", (210, 170, 90), (70, 80, 90),
                  fem=True, glasses=("thin", (60, 56, 60)), iris=(80, 120, 90), clothes="blazer"),
    "dante": look((120, 30, 40), (222, 172, 128), "spiky", (24, 20, 20), (40, 40, 44),
                  clothes="jacket"),
    "flor": look((200, 80, 130), (170, 116, 80), "bob", (60, 34, 22), (250, 210, 70),
                 fem=True, grin=True, clothes="scoop", flower=True),
    "gus": look((100, 110, 60), (240, 204, 176), "bald", (150, 150, 150), (110, 70, 50),
                beard="full", glasses=("thin", (150, 120, 60)), age="old", clothes="cardigan"),
    "tati": look((60, 40, 90), (124, 82, 58), "ponytail", (20, 16, 16), (200, 60, 90),
                 fem=True, clothes="vneck", earrings="hoop"),
    "nico": look((210, 170, 60), (248, 220, 196), "curly", (200, 120, 50), (50, 120, 200),
                 grin=True, iris=(80, 120, 160), freckles=True, age="young", big_curls=True),
    "jade": look((30, 120, 100), (228, 190, 150), "long", (40, 30, 28), (236, 236, 230),
                 fem=True, clothes="collar", earrings="stud"),
    "beto": look((160, 90, 50), (190, 132, 92), "cap", (30, 24, 20), (240, 200, 60),
                 grin=True, beard="goatee", cap=(30, 90, 60)),
    "vera": look((80, 70, 120), (236, 200, 178), "low_bun", (190, 190, 196), (30, 110, 120),
                 fem=True, glasses=("thin", (90, 60, 120)), age="old", clothes="scoop",
                 necklace=PEARL, earrings="pearl"),
    "leon": look((50, 70, 50), (244, 214, 190), "faux_hawk", (180, 60, 30), (90, 90, 96),
                 iris=(60, 110, 70), clothes="hoodie"),
    "kiara": look((240, 150, 60), (100, 66, 46), "bob", (30, 22, 20), (120, 60, 170),
                  fem=True, grin=True, clothes="vneck", earrings="hoop"),
}


class Geo:
    """Where the parts of a face are (canvas units, x from the middle)."""

    def __init__(self, code, L):
        fem = L["fem"]
        young = L["age"] == "young"
        # A little of each face its own: the same code, the same face.
        rng = random.Random(code + "face")

        def vary(v, k):
            return v + rng.uniform(-k, k)

        self.top = 40.0
        self.hw = vary(47.5 if fem else 50.0, 1.2)
        self.jaw = vary(34.0 if fem else 40.0, 2.0)
        self.jaw_y = 160.0 if fem else 163.0
        self.chin_w = vary(9.0 if fem else 14.0, 1.5)
        self.chin_y = vary(185.0 if fem else 189.0, 1.5)
        if young:
            self.jaw += 2.0
            self.chin_y -= 3.0
            self.jaw_y -= 2.0
        self.ex = vary(20.5, 0.5)
        self.ey = 114.0
        self.eye = vary(1.0, 0.05)
        self.ny = vary(139.0 if not young else 137.5, 1.0)
        self.nose = vary(1.0, 0.1)
        self.my = (155.0 if fem else 157.0) - (1.5 if young else 0.0)
        self.mouth = vary(1.0, 0.06)
        self.brow = vary(0.0, 1.0)
        self.nw = 22.0 if fem else 27.0
        for k, v in L.get("face", {}).items():
            setattr(self, k, getattr(self, k) + v)


def face_cmds(g, grow=0.0, chin=None):
    """The outline of the face; `grow` wider at the sides, `chin` lower."""
    chin = grow if chin is None else chin
    hw, t = g.hw + grow, g.top - grow
    j, jy = g.jaw + grow + chin * 0.25, g.jaw_y + chin * 0.5
    cw, cy = g.chin_w + grow * 0.6 + chin * 0.3, g.chin_y + chin
    return sym([
        ("M", 0, t),
        ("C", hw * 0.56, t, hw, t + 22, hw, 96),
        ("C", hw, 110, hw - 0.5, 122, hw - 2.5, 132),
        ("C", hw - 4.5, 144, j + 5, jy - 8, j, jy),
        ("C", j - 6, jy + 9, cw + 9, cy - 2.5, cw, cy - 1.5),
        ("C", cw * 0.55, cy - 0.3, cw * 0.25, cy, 0, cy),
    ])


class Person:
    def __init__(self, code, L):
        self.code = code
        self.L = L
        self.s = Svg(code)
        self.g = Geo(code, L)
        self.rng = random.Random(code)
        sk = L["skin"]
        self.skin = sk
        self.dark_skin = lum(sk) < 140
        self.hair = L["hair_color"]
        self.top = L["top"]
        self.face_d = d(face_cmds(self.g))

    # --- helpers
    def hair_fill(self, y0=20.0, y1=150.0, c=None, k=0.0):
        c = shade(c or self.hair, k)
        light = shade(c, 0.22 if lum(c) < 90 else 0.14)
        return self.s.lin(CX - 50, y0, CX + 40, y1, [(0, light), (0.45, c), (1, shade(c, -0.32))])

    def strands(self, curves, light=True, width=1.1, op=0.5, c=None):
        """Strokes along the hair: lighter ones catch the light, darker
        ones part it."""
        base = c or self.hair
        if light:
            col = shade(base, 0.38 if lum(base) < 80 else 0.3)
        else:
            col = shade(base, -0.42)
        for cmds in curves:
            self.s.path(cmds, stroke=col, stroke_width=width, stroke_linecap="round", opacity=op)

    def sheen(self, cmds, width=7.0, op=0.32):
        c = shade(self.hair, 0.5 if lum(self.hair) < 80 else 0.4)
        self.s.path(cmds, stroke=c, stroke_width=width, stroke_linecap="round", opacity=op,
                    filter=self.s.blur(2.2))

    def locks(self, items, k=0.3, op=0.5, c=None, blur=None):
        """Tapered strands of hair, each (start, bend, end, width): lighter
        (k > 0) where the light catches the hair, darker (k < 0) between
        locks."""
        base = c or self.hair
        if k > 0 and lum(base) < 70:
            k += 0.12
        col = shade(base, k)
        for p0, p1, p2, w in items:
            self.s.path(lock_d(p0, p1, p2, w), col, opacity=op,
                        filter=self.s.blur(blur) if blur else None)

    # --- the parts, back to front
    def draw(self):
        s, L = self.s, self.L
        disc_open(s, L["bg"])
        # The figure a little larger than drawn: a head that reads small.
        s.group(transform=f"translate({CX} 120) scale({FIGURE}) translate({-CX} -120)")
        self.back_hair()
        self.neck()
        self.clothes()
        self.ears()
        self.face()
        self.features()
        if L["beard"]:
            self.beard()
        self.mouth()
        if L["beard"] in ("full", "short", "goatee"):
            self.moustache()
        self.front_hair()
        if L["glasses"]:
            self.glasses()
        self.earrings()
        s.end()
        disc_close(s, L["bg"])
        return s.svg()

    def neck(self):
        s, g, sk = self.s, self.g, self.skin
        nw = g.nw
        cmds = sym([
            ("M", 0, 140),
            ("L", nw, 140),
            ("C", nw, 168, nw + 1, 186, nw + 4, 202),
            ("Q", nw * 0.5, 236, 0, 236),
        ])
        nd = d(cmds)
        s.path(nd, s.lin(0, 150, 0, 215, [(0, warm(sk, 0.5)), (0.5, warm(sk, 0.26)),
                                          (1, warm(sk, 0.14))]))
        s.group(clip_path=s.clip(nd))
        # The shade of the jaw, darker on the right.
        s.ellipse(CX + 6, g.chin_y - 2, nw + 14, 16, warm(sk, 0.6), opacity=0.6,
                  filter=s.blur(5))
        s.path([("M", nw - 2, 150), ("C", nw, 172, nw + 1, 186, nw + 4, 202)],
               stroke=warm(sk, 0.5), stroke_width=7, opacity=0.45, filter=s.blur(3))
        s.end()

    def shoulders(self, neck_l, neck_r, dip):
        """The outline of the shoulders and chest: from the neck's left
        side, the neckline (`dip`: commands from left to right), round."""
        right = [
            ("C", neck_r[0] + 14, neck_r[1] + 7, 62, 206, 86, 212),
            ("C", 106, 217, 120, 230, 128, 262),
            ("L", -128, 262),
            ("C", -120, 230, -106, 217, -86, 212),
            ("C", -62, 206, -neck_l[0] - 14, neck_l[1] + 7, -neck_l[0], neck_l[1]),
        ]
        return [("M", -neck_l[0], neck_l[1])] + dip + right + [("Z",)]

    def cloth_fill(self, c):
        return self.s.lin(CX - 70, 190, CX + 60, 262,
                          [(0, shade(c, 0.16)), (0.45, c), (1, shade(c, -0.34))])

    def cloth_shading(self, c, clip_d):
        s = self.s
        s.group(clip_path=s.clip(clip_d))
        # The head's shadow on the chest, a light on the left shoulder,
        # folds by the arms.
        s.ellipse(CX + 4, 212, 40, 13, shade(c, -0.5), opacity=0.4, filter=s.blur(6))
        s.ellipse(CX - 78, 218, 26, 10, shade(c, 0.35), opacity=0.35, filter=s.blur(6))
        for cmds in ([("M", 88, 226), ("Q", 98, 238, 100, 262)],
                     [("M", -88, 226), ("Q", -98, 238, -100, 262)]):
            s.path(cmds, stroke=shade(c, -0.45), stroke_width=3, opacity=0.35,
                   filter=s.blur(1.6))
        s.end()

    def clothes(self):
        s, g, L = self.s, self.g, self.L
        top = self.top
        nw = g.nw
        kind = L["clothes"]
        nl = (nw + 2, 196.0)
        if kind in ("crew", "hoodie", "sweater", "turtleneck", "polo"):
            dip = [("Q", 0, 226, nl[0], nl[1])]
        elif kind == "scoop":
            nl = (nw + 9, 198.0)
            dip = [("Q", 0, 242, nl[0], nl[1])]
        elif kind in ("vneck", "cardigan"):
            nl = (nw + 4, 197.0)
            dip = [("Q", -nw * 0.4, 216, 0, 240), ("Q", nw * 0.4, 216, nl[0], nl[1])]
        else:  # collar, flannel, jacket, blazer
            nl = (nw + 3, 196.0)
            dip = [("Q", -nw * 0.3, 218, 0, 244), ("Q", nw * 0.3, 218, nl[0], nl[1])]
        if kind == "hoodie":
            # The hood, round the back of the neck.
            hood = sym([("M", 0, 190), ("C", 20, 186, nw + 22, 180, nw + 30, 196),
                        ("C", nw + 36, 212, 30, 238, 0, 240)])
            s.path(hood, s.lin(0, 180, 0, 240, [(0, shade(top, -0.35)), (1, shade(top, -0.1))]))
        body = self.shoulders(nl, nl, dip)
        body_d = d(body)
        # Under an open neck: a shirt (white), or a tee (dark) under a jacket.
        if kind in ("collar", "flannel", "jacket", "blazer", "cardigan"):
            inner = {"jacket": shade(top, 0.12) if lum(top) > 60 else (58, 58, 64),
                     "blazer": (240, 240, 236), "cardigan": (240, 236, 228)}.get(kind)
            if inner is not None:
                s.path(sym([("M", 0, 190), ("L", nw + 6, 192), ("L", 60, 262), ("L", 0, 262)]),
                       s.lin(0, 190, 0, 262, [(0, shade(inner, -0.25)), (0.3, inner),
                                               (1, shade(inner, -0.15))]))
        s.path(body_d, self.cloth_fill(top))
        if kind == "flannel":
            # Checks: darker bands both ways.
            s.group(clip_path=s.clip(body_d))
            dark = shade(top, -0.55)
            for x in range(-130, 140, 26):
                s.el("rect", x=CX + x, y=180.0, width=9.0, height=90.0, fill=dark, opacity=0.38)
            for y in range(204, 264, 26):
                s.el("rect", x=0.0, y=float(y), width=256.0, height=9.0, fill=dark, opacity=0.38)
            for x in range(-117, 140, 26):
                s.el("rect", x=CX + x, y=180.0, width=2.0, height=90.0, fill=(240, 220, 200),
                     opacity=0.25)
            s.end()
        self.cloth_shading(top, body_d)
        rib = shade(top, 0.22 if lum(top) < 120 else -0.12)
        if kind in ("crew", "sweater", "hoodie"):
            s.path([("M", -nl[0] + 1, nl[1] - 1), ("Q", 0, 224, nl[0] - 1, nl[1] - 1)],
                   stroke=rib, stroke_width=5)
            s.path([("M", -nl[0] + 2, nl[1] + 1.5), ("Q", 0, 228, nl[0] - 2, nl[1] + 1.5)],
                   stroke=shade(top, -0.3), stroke_width=1, opacity=0.5)
        if kind == "sweater":
            # A shirt collar over the sweater.
            for side in (-1, 1):
                s.path([("M", 0, 213), ("L", nw + 6, 196), ("L", nw + 10, 210), ("L", 6, 222), ("Z",)]
                       if side > 0 else mirror_cmds([("M", 0, 213), ("L", nw + 6, 196),
                                                     ("L", nw + 10, 210), ("L", 6, 222), ("Z",)]),
                       (244, 242, 236), stroke=(200, 196, 190), stroke_width=0.8)
        if kind == "hoodie":
            for x in (-11, 11):
                s.path([("M", x, 224), ("Q", x * 1.15, 238, x * 1.05, 252)],
                       stroke=shade(top, 0.55), stroke_width=2.4, stroke_linecap="round")
                s.circle(CX + x * 1.05, 253, 2.0, shade(top, 0.7))
        if kind == "turtleneck":
            tube = sym([("M", 0, 168), ("C", nw * 0.6, 168, nw + 3, 170, nw + 4, 176),
                        ("L", nw + 5, 206), ("Q", nw * 0.5, 218, 0, 218)])
            s.path(tube, s.lin(0, 168, 0, 218, [(0, shade(top, 0.08)), (1, shade(top, -0.2))]))
            for k in range(-4, 5):
                x = k * nw / 4.6
                s.path([("M", x, 172), ("L", x * 1.08, 214)], stroke=shade(top, -0.3),
                       stroke_width=1.2, opacity=0.5)
            s.path(sym([("M", 0, 168), ("C", nw * 0.6, 168, nw + 3, 170, nw + 4, 176),
                        ("C", nw * 0.6, 178, 0, 178, 0, 178)]), shade(top, 0.15))
        if kind == "polo":
            for side in (1, -1):
                flap = [("M", 2, 210), ("L", nw + 3, 195), ("C", nw + 10, 200, nw + 14, 208, nw + 16, 214),
                        ("L", 8, 226), ("Z",)]
                s.path(flap if side > 0 else mirror_cmds(flap), shade(top, 0.06),
                       stroke=shade(top, -0.35), stroke_width=0.9)
            s.path([("M", 0, 222), ("L", 0, 244)], stroke=shade(top, -0.35), stroke_width=1.2)
            for y in (230, 240):
                s.circle(CX, y, 1.6, shade(top, 0.5))
        if kind in ("collar", "flannel"):
            col = top if kind == "collar" else shade(top, -0.05)
            line = shade(top, -0.3)
            for side in (1, -1):
                flap = [("M", 1, 226), ("L", nw + 2, 188), ("C", nw + 7, 194, nw + 12, 202, nw + 18, 210),
                        ("L", 7, 236), ("Z",)]
                s.path(flap if side > 0 else mirror_cmds(flap), shade(col, 0.06),
                       stroke=line, stroke_width=1.0)
            if kind == "collar":
                s.path([("M", 0, 244), ("L", 0, 262)], stroke=line, stroke_width=1.0)
                s.circle(CX, 252, 1.6, shade(top, -0.2))
        if kind in ("jacket", "blazer"):
            lapel_c = shade(top, 0.1 if kind == "blazer" else 0.08)
            for side in (1, -1):
                lap = [("M", nw + 3, 196), ("C", nw + 6, 206, 20, 232, 4, 262), ("L", 26, 262),
                       ("L", 22, 240), ("L", 34, 228), ("C", 36, 216, nw + 12, 200, nw + 8, 194),
                       ("Z",)]
                s.path(lap if side > 0 else mirror_cmds(lap), lapel_c, stroke=shade(top, -0.45),
                       stroke_width=1.0)
            if kind == "jacket":
                # A zip.
                s.path([("M", 30, 236), ("L", 40, 262)], stroke=SILVER, stroke_width=1.6,
                       opacity=0.7)
        if kind == "cardigan":
            for side in (1, -1):
                edge = [("M", nw + 3, 197), ("Q", nw * 0.4, 218, 2, 244), ("L", 2, 262)]
                s.path(edge if side > 0 else mirror_cmds(edge), stroke=shade(top, -0.3),
                       stroke_width=4.0)
            for y in (248, 258):
                s.circle(CX + 6, y, 1.9, shade(top, 0.45))
        if L["necklace"]:
            for k in range(-6, 7):
                t = k / 6.0
                x = t * (nw + 3)
                y = 204 + 16 * (1 - t * t)
                s.circle(CX + x, y, 2.3, L["necklace"], stroke=shade(L["necklace"], -0.25),
                         stroke_width=0.6)

    def ears(self):
        s, g, sk = self.s, self.g, self.skin
        if self.L["hair"] in ("long", "wavy", "sleek", "bob", "afro", "beanie"):
            if self.L["hair"] != "beanie":
                return
        for side in (-1, 1):
            hw = g.hw
            ear = [("M", hw - 5, 104), ("C", hw + 5, 95, hw + 11, 108, hw + 9, 120),
                   ("C", hw + 7.5, 133, hw + 3, 144, hw - 4, 142), ("Z",)]
            inner = [("M", hw + 0.5, 109), ("C", hw + 5.5, 106, hw + 7.5, 116, hw + 4.5, 126),
                     ("C", hw + 3, 132, hw + 4, 136, hw + 1, 138)]
            if side < 0:
                ear, inner = mirror_cmds(ear), mirror_cmds(inner)
            c = warm(sk, 0.06 if side < 0 else 0.16)
            s.path(ear, c)
            s.path(inner, stroke=warm(sk, 0.42), stroke_width=1.6, stroke_linecap="round",
                   opacity=0.75)

    def face(self):
        s, g, sk = self.s, self.g, self.skin
        fd = self.face_d
        s.path(fd, s.rad(CX - 14, 100, 96, [(0, shade(sk, 0.1)), (0.55, sk), (1, warm(sk, 0.2))]))
        s.group(clip_path=s.clip(fd))
        # The shaded side, the jaw's edge.
        s.ellipse(CX + g.hw + 4, 134, 16, 66, warm(sk, 0.42), opacity=0.5, filter=s.blur(6))
        s.ellipse(CX - g.hw - 8, 130, 12, 60, warm(sk, 0.3), opacity=0.3, filter=s.blur(6))
        s.ellipse(CX, g.chin_y + 4, g.jaw, 9, warm(sk, 0.4), opacity=0.45, filter=s.blur(4))
        # Under the cheekbones, the temples.
        for side in (-1, 1):
            x = CX + side * (g.hw - 9)
            s.ellipse(x, 152, 7, 17, warm(sk, 0.4), opacity=0.3 if side > 0 else 0.18,
                      transform=f"rotate({-side * 18} {fmt(x)} 152)", filter=s.blur(4.5))
            s.ellipse(CX + side * (g.hw - 4), 90, 6, 14, warm(sk, 0.35), opacity=0.25,
                      filter=s.blur(4))
        # Eye sockets, cheeks, a light on the forehead and down the nose.
        for side in (-1, 1):
            s.ellipse(CX + side * g.ex, g.ey - 4, 14, 8, warm(sk, 0.3), opacity=0.35,
                      filter=s.blur(3.5))
        blush = mix(sk, (226, 92, 96), 0.5 if not self.dark_skin else 0.3)
        for side in (-1, 1):
            s.ellipse(CX + side * 31, g.ny + 3, 12, 8, blush,
                      opacity=0.5 if self.L["fem"] else 0.25, filter=s.blur(4.5))
        s.ellipse(CX - 12, 76, 22, 11, shade(sk, 0.3), opacity=0.35, filter=s.blur(6))
        s.ellipse(CX - 31, 126, 10, 5, shade(sk, 0.3), opacity=0.3, filter=s.blur(3))
        s.path([("M", -1.5, 112), ("L", -2, 132)], stroke=shade(sk, 0.35), stroke_width=3,
               opacity=0.35, filter=s.blur(1.5))
        s.ellipse(CX - 2, g.chin_y - 9, 8, 4, shade(sk, 0.3), opacity=0.3, filter=s.blur(2.5))
        if self.L["age"] == "old":
            for y in (78, 85):
                s.path([("M", -18, y + 1), ("Q", 0, y - 2, 18, y + 1)], stroke=warm(sk, 0.35),
                       stroke_width=1.1, opacity=0.5)
            for side in (-1, 1):
                fold = [("M", 14, g.ny - 3), ("Q", 22, g.my - 4, 19, g.my + 6)]
                s.path(fold if side > 0 else mirror_cmds(fold), stroke=warm(sk, 0.35),
                       stroke_width=1.4, opacity=0.5)
        if self.L["freckles"]:
            for _ in range(26):
                side = self.rng.choice((-1, 1))
                x = side * self.rng.uniform(8, 36)
                y = self.rng.uniform(120, 140) - abs(x) * 0.12
                s.circle(CX + x, y, self.rng.uniform(0.7, 1.2), warm(sk, 0.32), opacity=0.6)
        s.end()

    def eye(self, side):
        s, g, L = self.s, self.g, self.L
        fem = L["fem"]
        cx, cy = CX + side * g.ex, g.ey
        k = g.eye
        ew = (11.8 if fem else 11.2) * k
        uh = (7.6 if fem else 6.7) * k
        lh = 4.4 * k
        if L["grin"]:
            uh -= 0.5
            lh -= 0.9
        if L["age"] == "old":
            uh -= 0.6

        def P(u, v):
            return (side * u, v)

        def at(cmds):
            return d(cmds, dx=cx - CX, dy=cy)

        shape = [("M",) + P(-ew, 0.8),
                 ("C",) + P(-ew * 0.55, -uh) + P(ew * 0.4, -uh - 0.6) + P(ew, -0.8),
                 ("C",) + P(ew * 0.55, lh * 0.95) + P(-ew * 0.45, lh) + P(-ew, 0.8), ("Z",)]
        sd = at(shape)
        white = (246, 241, 234) if not self.dark_skin else (240, 233, 225)
        s.path(sd, white)
        s.group(clip_path=s.clip(sd))
        ir = (6.7 if fem else 6.2) * k
        ix, iy = cx + side * 0.4, cy - 0.7
        iris = L["iris"]
        s.circle(ix, iy, ir, s.rad(ix, iy + 2, ir * 1.1, [(0, shade(iris, 0.45)), (0.55, iris),
                                                         (1, shade(iris, -0.55))]))
        s.circle(ix, iy, ir - 0.4, "none", stroke=shade(iris, -0.62), stroke_width=1.0)
        s.circle(ix, iy, ir * 0.44, (12, 8, 8))
        # The lid's shadow over the top of the eye.
        s.el("rect", x=cx - 14, y=cy - 11, width=28.0, height=8.5,
             fill=s.lin(0, cy - 9, 0, cy - 1, [(0, warm(self.skin, 0.65), 0.8),
                                               (1, warm(self.skin, 0.65), 0)]))
        s.end()
        s.circle(ix + 2.3, iy - 2.4, 1.7, (255, 255, 255))
        s.circle(ix - 1.9, iy + 1.9, 0.75, (255, 255, 255), opacity=0.7)
        lash = (28, 18, 16)
        lid = [("M",) + P(-ew - 0.6, 1.0),
               ("C",) + P(-ew * 0.55, -uh - 0.3) + P(ew * 0.4, -uh - 0.9) + P(ew + 0.6, -0.9)]
        s.path(at(lid), stroke=lash, stroke_width=2.3 if fem else 1.8, stroke_linecap="round")
        if fem:
            # Thicker towards the outer corner, a few lashes.
            outer = [("M",) + P(ew * 0.1, -uh - 0.5), ("C",) + P(ew * 0.6, -uh - 0.4) + P(ew, -2.5) + P(ew + 1.4, -1.0)]
            s.path(at(outer), stroke=lash, stroke_width=3.0, stroke_linecap="round")
            for u, v, du, dv in ((ew * 0.95, -1.6, 2.6, -2.2), (ew * 0.72, -3.8, 2.0, -2.8),
                                 (ew * 0.45, -5.4, 1.3, -2.8)):
                lsh = [("M",) + P(u, v), ("Q",) + P(u + du * 0.6, v + dv * 0.4) + P(u + du, v + dv)]
                s.path(at(lsh), stroke=lash, stroke_width=1.0, stroke_linecap="round")
        crease = [("M",) + P(-ew * 0.7, -uh + 0.4),
                  ("C",) + P(-ew * 0.3, -uh - 4.4) + P(ew * 0.5, -uh - 4.4) + P(ew * 0.98, -2.8)]
        s.path(at(crease), stroke=warm(self.skin, 0.42), stroke_width=1.1, opacity=0.7)
        lower = [("M",) + P(-ew * 0.55, lh * 0.9), ("Q",) + P(ew * 0.4, lh * 1.3) + P(ew, -0.4)]
        s.path(at(lower), stroke=warm(self.skin, 0.4), stroke_width=1.0, opacity=0.6)
        if L["grin"]:
            # A smile lifts the cheeks under the eyes.
            cheek = [("M",) + P(-ew * 0.3, lh + 2.6), ("Q",) + P(ew * 0.4, lh + 4.2) + P(ew * 0.95, lh + 0.6)]
            s.path(at(cheek), stroke=warm(self.skin, 0.3), stroke_width=1.0, opacity=0.4)
        if L["age"] == "old":
            for kk in (-1, 1):
                crow = [("M",) + P(ew + 2.5, kk * 1.5), ("L",) + P(ew + 7, kk * 3.8)]
                s.path(at(crow), stroke=warm(self.skin, 0.38), stroke_width=0.9, opacity=0.6)
            bag = [("M",) + P(-ew * 0.4, lh + 3), ("Q",) + P(ew * 0.3, lh + 5.5) + P(ew * 0.9, lh + 1.5)]
            s.path(at(bag), stroke=warm(self.skin, 0.32), stroke_width=1.0, opacity=0.5)

    def brow(self, side):
        s, g, L = self.s, self.g, self.L
        fem = L["fem"]
        hair = self.hair
        if L["hair"] == "bald" and lum(hair) < 100:
            c = shade(hair, 0.0)
        elif lum(hair) > 170:
            c = shade(hair, -0.35) if L["age"] != "old" else shade(hair, -0.2)
        else:
            c = shade(hair, -0.15)
        cx, y = CX + side * g.ex, g.ey
        th = 2.6 if fem else 3.8
        lift = 1.0 if L["grin"] else 0.0
        top = [("M", -9.5, y - 12.5 - lift), ("Q", 2, y - 18.5 - lift, 13.5, y - 13.5 - lift)]
        bot = [("Q", 3, y - 15.5 - lift + th, -9.5, y - 12.5 - lift + th), ("Z",)]
        cmds = top + bot
        if side < 0:
            cmds = mirror_cmds(cmds)
        s.path(d(cmds, dx=cx - CX), c)

    def nose(self):
        s, g, sk = self.s, self.g, self.skin
        ny = g.ny
        w = 9.5 if self.dark_skin else 8.5
        if self.L["fem"]:
            w -= 1.0
        s.path([("M", 4, 113), ("Q", 7, 125, 7.5, ny - 5)], stroke=warm(sk, 0.4),
               stroke_width=3.5, opacity=0.45, filter=s.blur(1.6))
        s.ellipse(CX - 1, ny - 5, 5.5, 4.5, shade(sk, 0.35), opacity=0.45, filter=s.blur(1.6))
        s.ellipse(CX + 1, ny + 2.5, w, 2.6, warm(sk, 0.45), opacity=0.45, filter=s.blur(1.4))
        for side in (-1, 1):
            ala = [("M", w, ny - 6), ("C", w + 2.6, ny - 3, w + 1, ny + 1, w - 3.5, ny + 1)]
            s.path(ala if side > 0 else mirror_cmds(ala), stroke=warm(sk, 0.45),
                   stroke_width=1.5, stroke_linecap="round", opacity=0.85)
            s.ellipse(CX + side * 3.8, ny + 0.6, 2.2, 1.1, warm(sk, 0.62), opacity=0.8)

    def features(self):
        for side in (-1, 1):
            self.eye(side)
            self.brow(side)
        self.nose()

    def lips(self):
        sk = self.skin
        if self.L["fem"]:
            base = mix(sk, (196, 72, 84), 0.55 if not self.dark_skin else 0.35)
            if self.dark_skin:
                base = mix(base, (110, 50, 50), 0.3)
        else:
            base = mix(sk, (176, 88, 80), 0.32)
            if self.dark_skin:
                base = mix(base, (90, 50, 44), 0.35)
        return base

    def mouth(self):
        s, g, sk = self.s, self.g, self.skin
        my = g.my
        lip = self.lips()
        dark = warm(lip, 0.4)
        if self.L["grin"]:
            mw = 13.0 * g.mouth
            # An open smile: the upper teeth under a thin upper lip, the
            # lower lip full.
            hole = [("M", -mw, my - 3), ("C", -mw * 0.45, my - 0.6, mw * 0.45, my - 0.6, mw, my - 3),
                    ("C", mw * 0.62, my + 9.5, -mw * 0.62, my + 9.5, -mw, my - 3), ("Z",)]
            hd = d(hole)
            # The lower lip first: the opening's edge is its top.
            lo = [("M", -mw * 0.78, my + 1.6), ("C", -mw * 0.35, my + 7.6, mw * 0.35, my + 7.6, mw * 0.78, my + 1.6),
                  ("C", mw * 0.48, my + 12.8, -mw * 0.48, my + 12.8, -mw * 0.78, my + 1.6), ("Z",)]
            s.path(lo, lip)
            s.ellipse(CX - 2, my + 9.6, 4, 1.1, (255, 255, 255), opacity=0.35)
            s.path(hd, (84, 30, 32))
            s.group(clip_path=s.clip(hd))
            s.ellipse(CX, my + 8.5, mw * 0.5, 3.6, (184, 84, 88))
            teeth = [("M", -mw, my - 4), ("C", -mw * 0.45, my - 1.4, mw * 0.45, my - 1.4, mw, my - 4),
                     ("L", mw, my + 0.5), ("C", mw * 0.45, my + 4.4, -mw * 0.45, my + 4.4, -mw, my + 0.5),
                     ("Z",)]
            s.path(teeth, s.lin(0, my - 2, 0, my + 4, [(0, (250, 248, 242)), (1, (222, 214, 206))]))
            s.path(hd, "none", stroke=(40, 10, 12), stroke_width=2.6, opacity=0.35,
                   filter=s.blur(1.0))
            s.end()
            up = [("M", -mw - 1.2, my - 3.2), ("C", -mw * 0.5, my - 6.6, -2.4, my - 7.2, 0, my - 5.8),
                  ("C", 2.4, my - 7.2, mw * 0.5, my - 6.6, mw + 1.2, my - 3.2),
                  ("C", mw * 0.45, my - 1.4, -mw * 0.45, my - 1.4, -mw - 1.2, my - 3.2), ("Z",)]
            s.path(up, dark)
            for side in (-1, 1):
                dimple = [("M", mw + 1.0, my - 4.6), ("Q", mw + 2.8, my - 2.4, mw + 1.6, my - 0.2)]
                s.path(dimple if side > 0 else mirror_cmds(dimple), stroke=warm(sk, 0.4),
                       stroke_width=1.1, stroke_linecap="round", opacity=0.55)
                if self.L["age"] == "old" or (self.L["age"] == "adult" and not self.L["fem"]):
                    fold = [("M", mw + 4.5, my - 13), ("Q", mw + 7.5, my - 6, mw + 4.5, my + 0.5)]
                    s.path(fold if side > 0 else mirror_cmds(fold), stroke=warm(sk, 0.32),
                           stroke_width=1.6, stroke_linecap="round", opacity=0.3, filter=s.blur(0.6))
            s.ellipse(CX, my + 16, 8, 2.5, warm(sk, 0.35), opacity=0.35, filter=s.blur(1.8))
        else:
            mw = (11.5 if self.L["fem"] else 12.0) * g.mouth
            up = [("M", -mw, my - 1), ("C", -mw * 0.6, my - 4, -mw * 0.25, my - 5, 0, my - 3.6),
                  ("C", mw * 0.25, my - 5, mw * 0.6, my - 4, mw, my - 1),
                  ("Q", 0, my + 1.6, -mw, my - 1), ("Z",)]
            s.path(up, dark)
            lo_h = 7.0 if self.L["fem"] else 5.5
            lo = [("M", -mw * 0.85, my), ("Q", 0, my + 1.8, mw * 0.85, my),
                  ("C", mw * 0.5, my + lo_h, -mw * 0.5, my + lo_h, -mw * 0.85, my), ("Z",)]
            s.path(lo, lip)
            s.path([("M", -mw - 0.8, my - 1.8), ("Q", 0, my + 2.6, mw + 0.8, my - 1.8)],
                   stroke=warm(lip, 0.6), stroke_width=1.3, stroke_linecap="round")
            s.ellipse(CX - 1.5, my + lo_h * 0.5, 3.5, 1.1, (255, 255, 255), opacity=0.3)
            for side in (-1, 1):
                c = [("M", mw - 0.5, my - 0.8), ("Q", mw + 1.6, my - 2, mw + 2.4, my - 3.6)]
                s.path(c if side > 0 else mirror_cmds(c), stroke=warm(sk, 0.4),
                       stroke_width=1.0, stroke_linecap="round", opacity=0.6)
            s.ellipse(CX, my + lo_h + 3, 7, 2.2, warm(sk, 0.35), opacity=0.4, filter=s.blur(1.6))

    def beard_region(self):
        g = self.g
        my = g.my
        style = self.L["beard"]
        if style == "goatee":
            right = [("M", 0, my - 8), ("C", 7, my - 9, 14, my - 7, 17, my - 2),
                     ("C", 19, my + 8, 16, g.chin_y - 2, 7, g.chin_y + 8), ("L", 0, g.chin_y + 9)]
        elif style == "short":
            right = [("M", 0, my - 8), ("C", 7, my - 9, 14, my - 7, 17, my - 2),
                     ("C", 24, my - 7, 32, my - 9, 38, 142), ("C", 44, 136, g.hw - 2, 124, g.hw + 2, 106),
                     ("L", g.hw + 30, 106), ("L", g.hw + 30, 270), ("L", 0, 270)]
        else:
            right = [("M", 0, my - 8.5), ("C", 7, my - 10, 14, my - 8, 17, my - 3),
                     ("C", 24, my - 10, 30, my - 14, 36, 136), ("C", 42, 128, g.hw - 3, 120, g.hw + 2, 104),
                     ("L", g.hw + 30, 104), ("L", g.hw + 30, 270), ("L", 0, 270)]
        return sym(right)

    def beard(self):
        s, g = self.s, self.g
        hair = self.hair
        region = self.beard_region()
        grow, chin = {"full": (2.5, 9.0), "short": (1.0, 4.0), "goatee": (0.5, 5.0)}[self.L["beard"]]
        shape = d(face_cmds(g, grow, chin))
        s.group(filter=s.blur(0.45))
        s.group(clip_path=s.clip(region))
        s.group(clip_path=s.clip(shape))
        s.path(shape, self.hair_fill(110, 200, k=0.02))
        # The light on the left, a shadow under the chin.
        s.ellipse(CX + 30, 170, 30, 40, shade(hair, -0.4), opacity=0.4, filter=s.blur(8))
        # Texture: short strokes, following the face down to the chin.
        for _ in range(220):
            x = self.rng.uniform(-g.hw - 2, g.hw + 2)
            y = self.rng.uniform(g.my - 14, g.chin_y + 8)
            ang = math.atan2(y - 110, x * 0.6) + self.rng.uniform(-0.25, 0.25)
            ln = self.rng.uniform(3, 6)
            light = self.rng.random() < 0.45
            c = shade(hair, 0.4 if lum(hair) < 80 else 0.3) if light else shade(hair, -0.45)
            s.path(f"M{fmt(CX + x)} {fmt(y)} l{fmt(math.cos(ang) * ln)} {fmt(math.sin(ang) * ln)}",
                   stroke=c, stroke_width=1.0, opacity=0.6, stroke_linecap="round")
        s.end()
        s.end()
        s.end()

    def moustache(self):
        s, g = self.s, self.g
        my = g.my
        if self.L["grin"]:
            right = [("M", 0, my - 9), ("C", 8, my - 10.5, 15, my - 8, 18, my - 2.5),
                     ("C", 12, my - 4.5, 6, my - 5.5, 0, my - 4.6)]
        else:
            right = [("M", 0, my - 9), ("C", 8, my - 10.5, 15, my - 8, 17, my),
                     ("C", 12, my - 3, 6, my - 3.8, 0, my - 3)]
        cmds = sym(right)
        s.path(cmds, self.hair_fill(my - 12, my + 2, k=0.05), filter=s.blur(0.5))
        for k in range(-5, 6):
            x = k * 2.8
            s.path([("M", x, my - 8.5), ("L", x * 1.15, my - 4)], stroke=shade(self.hair, 0.3),
                   stroke_width=0.8, opacity=0.4)

    # --- hair
    def hairline_shadow(self, cmds, dy=4.0, op=0.45):
        s = self.s
        s.group(clip_path=s.clip(self.face_d))
        s.path(d(cmds, dy=dy), warm(self.skin, 0.6), opacity=op, filter=s.blur(3))
        s.end()

    def back_hair(self):
        s, L, g = self.s, self.L, self.g
        h = L["hair"]
        fill_dark = self.hair_fill(30, 240, k=-0.18)
        if h in ("long", "sleek"):
            back = sym([("M", 0, 26), ("C", 40, 24, 64, 48, 66, 90), ("C", 68, 130, 66, 170, 74, 206),
                        ("C", 78, 226, 70, 238, 56, 240), ("L", 0, 240)])
            s.path(back, fill_dark)
        elif h == "wavy":
            back = sym([("M", 0, 24), ("C", 44, 22, 70, 48, 70, 90),
                        ("C", 72, 120, 62, 140, 74, 168), ("C", 84, 192, 70, 210, 80, 230),
                        ("C", 84, 242, 66, 246, 56, 244), ("L", 0, 244)])
            s.path(back, fill_dark)
        elif h == "bob":
            back = sym([("M", 0, 26), ("C", 40, 24, 66, 48, 66, 92), ("C", 66, 130, 66, 160, 64, 180),
                        ("C", 62, 188, 52, 190, 40, 186), ("L", 0, 186)])
            s.path(back, fill_dark)
        elif h == "afro":
            self.afro(back=True)
        elif h == "ponytail":
            tail = [("M", 30, 46), ("C", 70, 30, 92, 70, 84, 120), ("C", 78, 160, 92, 196, 80, 228),
                    ("C", 72, 214, 62, 190, 64, 160), ("C", 66, 120, 62, 80, 34, 60), ("Z",)]
            s.path(tail, self.hair_fill(40, 230, k=-0.1))
            self.strands([[("M", 60, 70), ("C", 80, 96, 70, 150, 80, 210)],
                          [("M", 52, 62), ("C", 74, 100, 64, 160, 72, 214)]], op=0.35)
        elif h == "low_bun":
            pass

    def afro(self, back):
        s, L = self.s, self.L
        hair = self.hair
        loose = L.get("loose", False)
        cy, rx, ry = 82.0, 80.0 if not loose else 78.0, 66.0
        n = 22 if not loose else 16
        bump = 7.5 if not loose else 10.0
        pts = []
        for k in range(n):
            a = math.pi * 2 * k / n
            pts.append((CX + math.cos(a) * rx, cy + math.sin(a) * ry * (1.0 if math.sin(a) < 0 else 0.92)))
        # Scalloped edge: a bump between each pair of points.
        dd = f"M{fmt(pts[0][0])} {fmt(pts[0][1])}"
        for k in range(n):
            a0, a1 = pts[k], pts[(k + 1) % n]
            mx, my = (a0[0] + a1[0]) / 2, (a0[1] + a1[1]) / 2
            nx, ny = mx - CX, my - cy
            ln = math.hypot(nx, ny) or 1
            q = (mx + nx / ln * bump, my + ny / ln * bump)
            dd += f" Q{fmt(q[0])} {fmt(q[1])} {fmt(a1[0])} {fmt(a1[1])}"
        dd += " Z"
        if back:
            s.path(dd, s.rad(CX - 26, 50, 110, [(0, shade(hair, 0.22)), (0.5, hair),
                                                 (1, shade(hair, -0.4))]))
            # Curls: little arcs, lighter at the top left.
            rng = random.Random(self.code + "curls")
            s.group(clip_path=s.clip(dd))
            for _ in range(160 if not loose else 110):
                a = rng.uniform(0, math.tau)
                rr = math.sqrt(rng.uniform(0.05, 1.0))
                x = CX + math.cos(a) * rx * rr
                y = cy + math.sin(a) * ry * rr
                light = (x - CX) + (y - cy) < -10
                c = shade(hair, 0.3 if lum(hair) < 60 else 0.22) if light else shade(hair, -0.45)
                r = rng.uniform(2.2, 3.8) if not loose else rng.uniform(3, 5)
                a0 = rng.uniform(0, math.tau)
                x0, y0 = x + math.cos(a0) * r, y + math.sin(a0) * r
                x1, y1 = x + math.cos(a0 + 2.4) * r, y + math.sin(a0 + 2.4) * r
                s.path(f"M{fmt(x0)} {fmt(y0)} A{fmt(r)} {fmt(r)} 0 0 1 {fmt(x1)} {fmt(y1)}",
                       stroke=c, stroke_width=1.3, opacity=0.55, stroke_linecap="round")
            s.end()
        self._afro_d = dd

    def front_hair(self):
        h = self.L["hair"]
        getattr(self, "hair_" + h)()

    def cap_shape(self, top=27.0, side=57.0, bottom=106.0, line=66.0, temple=45.0, round_=False):
        """Short hair: the outer edge over the skull, the hairline inside."""
        if round_:
            crown = ("C", side * 0.56, top, side, top + 22, side, top + 50)
        else:
            crown = ("C", side * 0.6, top - 1, side - 2, top + 13, side, top + 42)
        return sym([
            ("M", 0, top),
            crown,
            ("C", side + 1, top + 56, side - 1, bottom - 12, side - 3, bottom),
            ("L", side - 6.5, bottom + 6), ("L", side - 10, bottom + 5),
            ("C", side - 10, bottom - 8, temple + 1, 92, temple, 84),
            ("C", temple - 5, 74, 26, line, 0, line),
        ])

    def hair_short(self):
        s = self.s
        cmds = self.cap_shape(top=31.0, side=55.0, bottom=106.0, line=67.0)
        sweep = [("M", -23, 38), ("C", -21, 22, 2, 15, 26, 19), ("C", 46, 23, 59, 40, 57, 62),
                 ("C", 50, 54, 40, 51, 30, 53), ("C", 18, 55, 6, 60, -5, 63),
                 ("C", -15, 63, -23, 52, -23, 38), ("Z",)]
        self.hairline_shadow(cmds)
        self.hairline_shadow(sweep, dy=5, op=0.35)
        s.path(cmds, self.hair_fill())
        s.path(sweep, self.hair_fill(14, 64, k=0.04))
        self.locks([((-30, 44), (-36, 56), (-42, 74), 2.6), ((-38, 48), (-44, 60), (-47, 80), 2.0)],
                   k=0.25, op=0.45)
        self.locks([((-18, 36), (6, 19), (42, 34), 3.6), ((-14, 44), (12, 30), (50, 47), 3.0),
                    ((-8, 54), (18, 42), (53, 58), 2.4), ((-20, 28), (-4, 22), (16, 20), 2.0)],
                   k=0.32, op=0.55)
        self.locks([((-21, 31), (-22, 44), (-27, 58), 1.6), ((-6, 46), (16, 35), (46, 51), 1.8),
                    ((0, 58), (20, 49), (44, 59), 1.5)], k=-0.45, op=0.55)
        self.sheen([("M", -14, 30), ("C", -2, 22, 14, 22, 28, 26)], width=6, op=0.3)

    def hair_crop(self):
        s = self.s
        cmds = self.cap_shape(top=35.0, side=53.5, bottom=104.0, line=64.0, temple=46.0, round_=True)
        self.hairline_shadow(cmds, dy=2.5, op=0.3)
        s.path(cmds, self.hair_fill())
        # Close-cropped: a fine grain, a soft light on top.
        rng = random.Random(self.code)
        s.group(clip_path=s.clip(cmds))
        for _ in range(140):
            x, y = rng.uniform(-52, 52), rng.uniform(34, 100)
            s.circle(CX + x, y, 0.7, shade(self.hair, 0.4), opacity=0.35)
        s.end()
        self.sheen([("M", -22, 44), ("C", -10, 38, 10, 38, 22, 44)], width=9, op=0.22)

    def hair_swept(self):
        s = self.s
        cmds = self.cap_shape(top=30.0, side=55.0, bottom=106.0, line=67.0)
        quiff = [("M", -42, 66), ("C", -46, 34, -16, 10, 18, 12), ("C", 44, 14, 61, 33, 57, 60),
                 ("C", 47, 47, 28, 43, 8, 50), ("C", -10, 56, -26, 66, -42, 66), ("Z",)]
        self.hairline_shadow(cmds)
        self.hairline_shadow(quiff, dy=5, op=0.35)
        s.path(cmds, self.hair_fill())
        s.path(quiff, self.hair_fill(10, 66, k=0.04))
        self.locks([((-36, 60), (-24, 22), (24, 15), 4.2), ((-26, 58), (-6, 30), (42, 28), 3.6),
                    ((-14, 56), (10, 39), (52, 47), 3.0), ((-42, 52), (-36, 26), (-6, 15), 2.6)],
                   k=0.3, op=0.55)
        self.locks([((-30, 62), (-14, 36), (30, 24), 1.9), ((-4, 53), (18, 42), (54, 54), 1.9)],
                   k=-0.45, op=0.5)
        self.sheen([("M", -28, 40), ("C", -18, 24, 2, 18, 22, 20)], op=0.35)

    def hair_pixie(self):
        s = self.s
        cmds = self.cap_shape(top=26.0, side=57.0, bottom=112.0, line=68.0)
        self.hairline_shadow(cmds)
        s.path(cmds, self.hair_fill())
        # A soft fringe swept to the side, the ears half hidden.
        fringe = [("M", -48, 70), ("C", -46, 44, -20, 36, 6, 40), ("C", 30, 44, 46, 60, 50, 84),
                  ("C", 40, 76, 28, 74, 16, 76), ("C", 0, 80, -16, 72, -28, 84),
                  ("C", -36, 92, -44, 88, -48, 70), ("Z",)]
        self.hairline_shadow(fringe, dy=4, op=0.35)
        s.path(fringe, self.hair_fill(30, 90, k=0.04))
        for side in (-1, 1):
            tuft = [("M", 50, 92), ("C", 58, 100, 58, 116, 52, 124), ("C", 50, 114, 48, 104, 46, 98),
                    ("Z",)]
            s.path(tuft if side > 0 else mirror_cmds(tuft), self.hair_fill(90, 125))
        self.strands([[("M", -40, 70), ("C", -30, 52, -6, 46, 20, 52)],
                      [("M", -30, 80), ("C", -20, 64, 6, 58, 30, 66)],
                      [("M", -20, 36), ("C", 0, 30, 30, 36, 48, 60)]], op=0.5)
        self.strands([[("M", -36, 84), ("C", -24, 70, 0, 66, 18, 74)]], light=False, op=0.4)

    def hair_spiky(self):
        s = self.s
        base = self.cap_shape(top=34.0, side=55.0, bottom=104.0, line=69.0)
        rng = random.Random(self.code + "spikes")
        cx0, cy0 = 0.0, 86.0
        tufts = []
        for k in range(11):
            a = math.pi * (1.06 + 0.88 * k / 10) + rng.uniform(-0.04, 0.04)
            r0, r1 = 30.0, (63.0 if lum(self.hair) > 60 else 58.0) + rng.uniform(-3, 5)
            bend = a + 0.12
            p0 = (cx0 + math.cos(a - 0.05) * r0, cy0 + math.sin(a - 0.05) * r0)
            p1 = (cx0 + math.cos(bend) * (r0 + r1) * 0.55, cy0 + math.sin(bend) * (r0 + r1) * 0.55)
            p2 = (cx0 + math.cos(a + 0.1) * r1, cy0 + math.sin(a + 0.1) * r1)
            tufts.append((p0, p1, p2, 11.0 + rng.uniform(-1, 1.5)))
        # A messy fringe, pointing down over the forehead.
        fringe = []
        for k in range(7):
            x = -36 + k * 12 + rng.uniform(-2, 2)
            ln = rng.uniform(16, 24)
            fringe.append(((x - 8, 50), (x - 2, 58), (x + 5, 54 + ln), 5.0))
        self.hairline_shadow(base)
        s.group(clip_path=s.clip(self.face_d))
        for p0, p1, p2, w in fringe:
            s.path(lock_d((p0[0], p0[1] + 4), (p1[0], p1[1] + 4), (p2[0] + 1, p2[1] + 4), w),
                   warm(self.skin, 0.6), opacity=0.4, filter=s.blur(2.5))
        s.end()
        fill = self.hair_fill(10, 110)
        s.path(base, fill)
        for p0, p1, p2, w in tufts:
            s.path(lock_d(p0, p1, p2, w), fill)
        for p0, p1, p2, w in fringe:
            s.path(lock_d(p0, p1, p2, w), self.hair_fill(40, 90, k=0.02))
        self.locks([(p0, p1, p2, w * 0.35) for p0, p1, p2, w in tufts[:7]], k=0.32,
                   op=0.5 if lum(self.hair) > 60 else 0.3)
        self.locks([(p0, p1, p2, w * 0.3) for p0, p1, p2, w in fringe[::2]], k=0.3, op=0.45)
        self.locks([((p0[0] + 4, p0[1]), p1, (p2[0] + 2, p2[1]), w * 0.2) for p0, p1, p2, w in tufts[1::2]],
                   k=-0.45, op=0.5)

    def hair_bald(self):
        s, g = self.s, self.g
        sk = self.skin
        s.ellipse(CX - 14, 54, 18, 9, shade(sk, 0.55), opacity=0.55, filter=s.blur(3.5))
        if lum(self.hair) > 100:
            # Grey hair left at the sides and the back.
            hw = g.hw
            for side in (-1, 1):
                tuft = [("M", hw - 9, 72), ("C", hw + 1, 72, hw + 7, 84, hw + 6, 98),
                        ("C", hw + 5, 106, hw + 3, 112, hw - 1, 116),
                        ("C", hw - 3, 104, hw - 4, 92, hw - 9, 72), ("Z",)]
                s.path(tuft if side > 0 else mirror_cmds(tuft), self.hair_fill(70, 116, k=0.04))
                ls = [((hw - 6, 74), (hw + 6, 84), (hw + 4, 104), 1.6),
                      ((hw - 4, 80), (hw + 3, 94), (hw + 1, 112), 1.2),
                      ((hw - 2, 76), (hw + 9, 86), (hw + 8, 96), 1.0)]
                if side < 0:
                    ls = [((-a[0], a[1]), (-b[0], b[1]), (-c[0], c[1]), w) for a, b, c, w in ls]
                self.locks(ls, k=0.35, op=0.7)
                self.locks([((side * (hw - 3), 86), (side * (hw + 2), 98), (side * (hw + 1), 110), 1.0)],
                           k=-0.35, op=0.5)
        else:
            # A shaved head: the shadow of the hair.
            s.group(clip_path=s.clip(self.face_d))
            s.path(self.cap_shape(top=38, side=52, bottom=102, line=66), self.hair, opacity=0.14,
                   filter=s.blur(1.5))
            s.end()

    def hair_mohawk(self):
        s, g = self.s, self.g
        stub = self.cap_shape(top=38, side=52, bottom=102, line=68, round_=True)
        s.group(clip_path=s.clip(self.face_d))
        s.path(stub, mix(warm(self.skin, 0.5), (40, 36, 40), 0.5), opacity=0.3, filter=s.blur(1.2))
        s.end()
        crest = [("M", -12, 66), ("L", -14, 50), ("L", -22, 16), ("L", -9, 34), ("L", -10, 0),
                 ("L", 2, 26), ("L", 8, -6), ("L", 12, 26), ("L", 24, 6), ("L", 16, 40),
                 ("L", 30, 30), ("L", 14, 52), ("L", 12, 66), ("Q", 0, 62, -12, 66), ("Z",)]
        self.hairline_shadow(crest, dy=3)
        s.path(crest, self.hair_fill(0, 70, k=0.05))
        self.strands([[("M", -6, 60), ("L", -8, 24)], [("M", 4, 58), ("L", 8, 16)],
                      [("M", -2, 60), ("L", 2, 8)]], op=0.5)

    def hair_faux_hawk(self):
        s = self.s
        stub = self.cap_shape(top=36, side=53, bottom=104, line=68)
        self.hairline_shadow(stub, dy=2, op=0.2)
        s.path(stub, shade(self.hair, -0.25), opacity=0.55)
        crest = [("M", -22, 68), ("C", -28, 40, -18, 14, 6, 8), ("C", 18, 6, 30, 10, 34, 18),
                 ("C", 26, 18, 22, 22, 30, 30), ("C", 34, 44, 24, 60, 20, 68), ("Q", 0, 62, -22, 68),
                 ("Z",)]
        self.hairline_shadow(crest, dy=3)
        s.path(crest, self.hair_fill(4, 70, k=0.05))
        self.strands([[("M", -14, 62), ("C", -18, 40, -8, 20, 10, 12)],
                      [("M", 0, 62), ("C", -2, 40, 6, 24, 24, 16)],
                      [("M", 12, 64), ("C", 12, 46, 18, 34, 28, 26)]], op=0.5)
        self.sheen([("M", -14, 40), ("C", -10, 26, 0, 18, 12, 14)], width=5, op=0.35)

    def hair_curly(self):
        s = self.s
        big = self.L.get("big_curls", False)
        hair = self.hair
        rng = random.Random(self.code + "curly")
        # A cap of curls with a bumpy edge, curls hanging over the forehead.
        cmds = self.cap_shape(top=28.0, side=56.0, bottom=104.0, line=66.0)
        r = 9.5 if big else 7.5
        bumps = []
        n = 13 if big else 16
        for k in range(n):
            a = math.pi * (1.0 + k / (n - 1))
            bumps.append((math.cos(a) * 56, 86 + math.sin(a) * 57))
        front = [(-34, 64), (-20, 62), (-6, 64), (8, 62), (22, 64), (36, 66)]
        if big:
            front = [(-36, 66), (-18, 64), (0, 66), (18, 63), (36, 68)]
        self.hairline_shadow(cmds)
        s.group(clip_path=s.clip(self.face_d))
        for x, y in front:
            s.circle(CX + x + 1, y + 5, r * 0.9, warm(self.skin, 0.6), opacity=0.4, filter=s.blur(2.5))
        s.end()
        fill = self.hair_fill(14, 110)
        s.path(cmds, fill)
        mass = [f'<path d="{d(cmds)}"/>']
        for x, y in bumps + front:
            cx_, cy_, rr = CX + x + rng.uniform(-1, 1), y + rng.uniform(-1, 1), r * rng.uniform(0.85, 1.1)
            s.circle(cx_, cy_, rr, fill)
            mass.append(f'<circle cx="{fmt(cx_)}" cy="{fmt(cy_)}" r="{fmt(rr)}"/>')
        s.group(clip_path=s.clip("".join(mass)))
        # Curls: little hooks, lit at the top left.
        spots = bumps + front
        for _ in range(46 if not big else 34):
            a = rng.uniform(math.pi * 1.02, math.pi * 1.98)
            rr = rng.uniform(0.0, 1.0)
            spots.append((math.cos(a) * 50 * rr, 84 + math.sin(a) * 52 * rr))
        for x, y in spots:
            rad = r * rng.uniform(0.45, 0.7)
            a0 = rng.uniform(0, math.tau)
            lit = x + (y - 60) < 6
            c = shade(hair, (0.42 if lum(hair) < 80 else 0.3) if lit else -0.3)
            x0, y0 = CX + x + math.cos(a0) * rad, y + math.sin(a0) * rad
            x1, y1 = CX + x + math.cos(a0 + 3.6) * rad, y + math.sin(a0 + 3.6) * rad
            s.path(f"M{fmt(x0)} {fmt(y0)} A{fmt(rad)} {fmt(rad)} 0 1 1 {fmt(x1)} {fmt(y1)}",
                   stroke=c, stroke_width=1.5 if big else 1.2, opacity=0.45, stroke_linecap="round")
        s.end()
        self.sheen([("M", -30, 46), ("C", -18, 34, 4, 30, 20, 34)], width=9, op=0.22)

    def hair_coils(self):
        s = self.s
        hair = self.hair
        cmds = self.cap_shape(top=24.0, side=56.0, bottom=104.0, line=64.0, temple=46.0)
        self.hairline_shadow(cmds, dy=3)
        s.path(cmds, self.hair_fill())
        rng = random.Random(self.code + "coils")
        s.group(clip_path=s.clip(cmds))
        for _ in range(170):
            x, y = rng.uniform(-58, 58), rng.uniform(22, 100)
            r = rng.uniform(1.6, 2.8)
            lit = x + (y - 40) < 0
            c = shade(hair, 0.42 if lit else 0.2)
            s.path(f"M{fmt(CX + x - r)} {fmt(y)} A{fmt(r)} {fmt(r)} 0 0 1 {fmt(CX + x + r)} {fmt(y)}",
                   stroke=c, stroke_width=1.0, opacity=0.5, stroke_linecap="round")
        s.end()
        # A bumpy top edge.
        for k in range(11):
            a = math.pi * (1.05 + 0.9 * k / 10)
            x, y = CX + math.cos(a) * 55, 82 + math.sin(a) * 56
            s.circle(x, y, 5.5, self.hair_fill(k=0.0))

    def hair_bun(self, size=17.0, y=28.0, messy=False):
        s = self.s
        cmds = self.cap_shape(top=33.0, side=53.5, bottom=100.0, line=66.0, temple=45.0)
        bun = f"M{fmt(CX)} {fmt(y - size)}"
        s.circle(CX, y, size, self.hair_fill(y - size, y + size, k=-0.05))
        self.strands([[("M", -size * 0.7, y - 4), ("C", -4, y - size + 2, 8, y - size + 4, size * 0.75, y + 2)],
                      [("M", -size * 0.5, y + 6), ("C", -2, y - 6, 10, y - 4, size * 0.6, y + 8)]], op=0.5)
        if messy:
            self.strands([[("M", -8, y - size), ("C", -16, y - size - 8, -24, y - size - 2, -20, y - 6)],
                          [("M", 10, y - size + 1), ("C", 22, y - size - 6, 24, y - 4, 18, y + 4)]],
                         light=False, width=1.6, op=0.6)
        self.hairline_shadow(cmds)
        s.path(cmds, self.hair_fill())
        # Combed back from a middle parting to the bun.
        self.strands([[("M", 0, 64), ("C", -2, 50, -2, 40, 0, 34)],
                      [("M", -20, 66), ("C", -18, 52, -12, 42, -6, 36)],
                      [("M", 20, 66), ("C", 18, 52, 12, 42, 6, 36)],
                      [("M", -38, 74), ("C", -36, 56, -26, 44, -12, 36)],
                      [("M", 38, 74), ("C", 36, 56, 26, 44, 12, 36)]], op=0.45)
        self.strands([[("M", 0, 64), ("L", 0, 40)]], light=False, op=0.5)
        self.sheen([("M", -30, 50), ("C", -18, 40, 18, 40, 30, 50)], op=0.3)
        if messy:
            for side in (-1, 1):
                lock = [("M", 44, 80), ("C", 50, 96, 46, 112, 50, 126)]
                s.path(lock if side > 0 else mirror_cmds(lock), stroke=self.hair, stroke_width=2.2,
                       stroke_linecap="round")
        else:
            for side in (-1, 1):
                lock = [("M", 46, 84), ("C", 49, 100, 47, 114, 51, 128)]
                s.path(lock if side > 0 else mirror_cmds(lock), stroke=self.hair, stroke_width=1.6,
                       stroke_linecap="round", opacity=0.9)

    def hair_messy_bun(self):
        self.hair_bun(size=19.0, y=26.0, messy=True)

    def hair_low_bun(self):
        s = self.s
        cmds = self.cap_shape(top=30.0, side=55.0, bottom=100.0, line=64.0, temple=45.0)
        self.hairline_shadow(cmds)
        s.path(cmds, self.hair_fill())
        # Waves combed back from a side parting.
        self.strands([[("M", -14, 34), ("C", -24, 44, -38, 56, -44, 80)],
                      [("M", -10, 40), ("C", 6, 42, 30, 52, 44, 76)],
                      [("M", -16, 48), ("C", -26, 56, -36, 66, -40, 84)],
                      [("M", -2, 50), ("C", 12, 52, 30, 62, 38, 80)],
                      [("M", -14, 60), ("C", -2, 60, 14, 64, 28, 70)]], op=0.55)
        self.strands([[("M", -14, 32), ("L", -16, 46)]], light=False, op=0.5)
        self.sheen([("M", -32, 52), ("C", -20, 38, 10, 36, 30, 48)], op=0.3)
        # The bun, a little to the side at the top.
        s.ellipse(CX + 24, 30, 15, 12, self.hair_fill(18, 44, k=-0.04))
        self.strands([[("M", 12, 28), ("C", 18, 20, 32, 20, 38, 30)],
                      [("M", 14, 34), ("C", 22, 28, 32, 28, 36, 36)]], op=0.5)

    def hair_ponytail(self):
        s = self.s
        cmds = self.cap_shape(top=30.0, side=54.5, bottom=100.0, line=66.0, temple=45.0)
        self.hairline_shadow(cmds)
        s.path(cmds, self.hair_fill())
        self.strands([[("M", -6, 66), ("C", -4, 50, 6, 38, 24, 34)],
                      [("M", -30, 70), ("C", -24, 50, 0, 36, 26, 34)],
                      [("M", 20, 68), ("C", 22, 54, 26, 44, 30, 38)],
                      [("M", 40, 76), ("C", 40, 60, 36, 46, 32, 38)]], op=0.5)
        self.strands([[("M", -8, 66), ("C", -6, 52, 4, 42, 20, 36)]], light=False, op=0.45)
        self.sheen([("M", -32, 52), ("C", -20, 38, 4, 32, 22, 34)], op=0.3)
        # The tie, high at the back on the right.
        s.ellipse(CX + 32, 38, 6, 5, shade(self.top, -0.1))
        # A loose lock by the ear.
        lock = [("M", -44, 80), ("C", -50, 96, -46, 112, -50, 126)]
        s.path(lock, stroke=self.hair, stroke_width=2.0, stroke_linecap="round")

    def long_front(self, part=0.0, wavy=False, sleek=False):
        """Long hair over the head and down both sides, in front of the
        shoulders; parted at `part`."""
        s = self.s
        # Two curtains from the parting.
        if wavy:
            right = [("M", part, 26), ("C", 30, 24, 60, 40, 62, 80), ("C", 64, 104, 56, 120, 64, 144),
                     ("C", 72, 166, 60, 184, 70, 206), ("C", 74, 220, 66, 232, 58, 236),
                     ("C", 54, 222, 50, 206, 52, 192), ("C", 52, 176, 42, 160, 46, 140),
                     ("C", 50, 118, 46, 96, 44, 84), ("C", 36, 70, 20, 60, part + 2, 52), ("Z",)]
        else:
            right = [("M", part, 26), ("C", 30, 24, 60, 40, 62, 82), ("C", 64, 120, 60, 160, 66, 200),
                     ("C", 68, 216, 64, 230, 56, 236), ("C", 50, 222, 46, 206, 46, 190),
                     ("C", 46, 160, 47, 120, 46, 96), ("C", 44, 76, 22, 60, part + 2, 54), ("Z",)]
        left = [("M", part, 26), ("C", -30 + part * 0.5, 24, -60, 40, -62, 82),
                ("C", -64, 120, -60, 160, -66, 200), ("C", -68, 216, -64, 230, -56, 236),
                ("C", -50, 222, -46, 206, -46, 190), ("C", -46, 160, -47, 120, -46, 96),
                ("C", -44, 76, -22 + part * 0.4, 60, part - 2, 54), ("Z",)]
        if wavy:
            left = [(c[0],) + tuple(-v if i % 2 == 0 else v for i, v in enumerate(c[1:])) for c in right]
            left[0] = ("M", part, 26)
        for cmds in (left, right):
            self.hairline_shadow(cmds, dy=3)
        for cmds in (left, right):
            s.path(cmds, self.hair_fill(20, 240))
        # Strands down the curtains.
        for side in (-1, 1):
            if wavy:
                ss = [[("M", 30, 34), ("C", 56, 50, 56, 80, 54, 110), ("C", 52, 130, 62, 150, 60, 176),
                       ("C", 58, 196, 66, 210, 62, 226)],
                      [("M", 18, 34), ("C", 46, 48, 50, 80, 50, 104), ("C", 50, 128, 56, 150, 56, 172)]]
            else:
                ss = [[("M", 30, 32), ("C", 52, 44, 58, 80, 56, 120), ("C", 55, 160, 58, 196, 60, 228)],
                      [("M", 16, 34), ("C", 44, 46, 52, 80, 52, 120), ("C", 52, 160, 54, 190, 54, 214)]]
            if side < 0:
                ss = [mirror_cmds(c) for c in ss]
            self.strands(ss, op=0.4 if not sleek else 0.55)
        self.strands([[("M", part, 26), ("L", part, 50)]], light=False, op=0.5)
        self.sheen([("M", -40, 44), ("C", -26, 32, 26, 32, 40, 44)], op=0.38 if sleek else 0.3,
                   width=8 if sleek else 7)

    def hair_long(self):
        self.long_front(part=0.0)

    def hair_sleek(self):
        self.long_front(part=-14.0, sleek=True)

    def hair_wavy(self):
        self.long_front(part=-6.0, wavy=True)

    def hair_afro(self):
        s = self.s
        # Over the top of the head: the hair, its edge round on the forehead.
        region = [("M", -90, -10), ("L", 90, -10), ("L", 90, 98), ("L", 57, 98),
                  ("C", 54, 76, 36, 63, 0, 63), ("C", -36, 63, -54, 76, -57, 98), ("L", -90, 98), ("Z",)]
        line = [("M", -57, 98), ("C", -54, 76, -36, 63, 0, 63), ("C", 36, 63, 54, 76, 57, 98),
                ("L", 57, 70), ("L", -57, 70), ("Z",)]
        s.group(clip_path=s.clip(self.face_d))
        s.path([("M", -57, 98), ("C", -54, 76, -36, 63, 0, 63), ("C", 36, 63, 54, 76, 57, 98)],
               stroke=warm(self.skin, 0.6), stroke_width=6, opacity=0.45, filter=s.blur(2.5),
               transform="translate(0 3)")
        s.end()
        s.group(clip_path=s.clip(region))
        self.afro(back=True)
        s.end()

    def hair_bob(self):
        s = self.s
        # The sides, down to the jaw, and a straight fringe.
        sides = sym([("M", 0, 28), ("C", 38, 26, 62, 46, 62, 90), ("C", 62, 130, 62, 160, 62, 178),
                     ("C", 60, 186, 52, 188, 44, 184), ("C", 46, 160, 47, 130, 46, 100),
                     ("L", 0, 100)])
        s.path(sides, self.hair_fill(20, 190))
        fringe = sym([("M", 0, 30), ("C", 36, 28, 52, 46, 50, 94),
                      ("C", 40, 92, 30, 94, 20, 92), ("C", 10, 95, 0, 92, 0, 92)])
        self.hairline_shadow(fringe, dy=4, op=0.55)
        s.path(fringe, self.hair_fill(26, 96))
        for k in range(-4, 5):
            x = k * 10.5
            s.path([("M", x * 0.8, 40), ("C", x * 0.95, 60, x, 80, x * 1.02, 93)],
                   stroke=shade(self.hair, -0.4), stroke_width=1.0, opacity=0.45)
        self.strands([[("M", 48, 60), ("C", 56, 100, 54, 140, 54, 178)],
                      [("M", -48, 60), ("C", -56, 100, -54, 140, -54, 178)]], op=0.45)
        self.sheen([("M", -36, 50), ("C", -22, 36, 22, 36, 36, 50)], op=0.36)
        if self.L.get("flower"):
            fx, fy = CX + 46, 70
            for k in range(5):
                a = math.tau * k / 5 - math.pi / 2
                s.ellipse(fx + math.cos(a) * 5.5, fy + math.sin(a) * 5.5, 5, 5, (250, 250, 246),
                          stroke=(220, 210, 200), stroke_width=0.6)
            s.circle(fx, fy, 3.4, (246, 190, 50))

    def under_hat(self):
        """The hair a hat leaves showing: by the ears, round their tops."""
        s, g = self.s, self.g
        hw = g.hw
        for side in (-1, 1):
            burn = [("M", hw - 7.5, 80), ("L", hw + 13, 80), ("C", hw + 13, 89, hw + 9, 96, hw + 4, 98),
                    ("C", hw + 1, 101, hw, 107, hw - 1.5, 114), ("L", hw - 5.5, 114),
                    ("C", hw - 6.5, 104, hw - 7.5, 94, hw - 7.5, 80), ("Z",)]
            if side < 0:
                burn = mirror_cmds(burn)
            self.hairline_shadow(burn, dy=2, op=0.3)
            s.path(burn, self.hair_fill(80, 114))
            ls = [((hw - 5, 84), (hw - 4, 98), (hw - 3.5, 110), 0.8), ((hw + 2, 84), (hw + 6, 90), (hw + 4, 96), 0.7)]
            if side < 0:
                ls = [((-a[0], a[1]), (-b[0], b[1]), (-c[0], c[1]), w) for a, b, c, w in ls]
            self.locks(ls, k=0.3, op=0.5)

    def hair_cap(self):
        s, g = self.s, self.g
        cap = self.L["cap"]
        self.under_hat()
        crown = sym([("M", 0, 20), ("C", 34, 20, 58, 36, 60, 72), ("L", 61, 86), ("Q", 30, 80, 0, 80)])
        s.path(crown, s.lin(CX - 40, 20, CX + 50, 90, [(0, shade(cap, 0.18)), (0.5, cap),
                                                         (1, shade(cap, -0.35))]))
        # Seams, the button, a badge.
        for x in (-22, 22):
            s.path([("M", 0, 21), ("Q", x * 1.2, 40, x * 1.5, 80)], stroke=shade(cap, -0.35),
                   stroke_width=1.0, opacity=0.6)
        s.ellipse(CX, 21, 5, 2.5, shade(cap, -0.2))
        s.circle(CX, 54, 9, (246, 242, 230))
        s.circle(CX, 54, 9, "none", stroke=shade(cap, -0.4), stroke_width=1)
        s.text(CX, 58.5, "8", 12, (30, 26, 26))
        # The brim, seen from the front: a band curving down.
        brim = [("M", -62, 84), ("C", -40, 76, 40, 76, 62, 84), ("C", 58, 96, 40, 102, 0, 102),
                ("C", -40, 102, -58, 96, -62, 84), ("Z",)]
        self.hairline_shadow(brim, dy=6, op=0.6)
        s.path(brim, s.lin(0, 78, 0, 102, [(0, shade(cap, -0.1)), (1, shade(cap, -0.4))]))
        s.path([("M", -60, 86), ("C", -40, 79, 40, 79, 60, 86)], stroke=shade(cap, 0.25),
               stroke_width=1.2, opacity=0.6)

    def hair_beanie(self):
        s, g = self.s, self.g
        knit = self.L["knit"]
        self.under_hat()
        dome = sym([("M", 0, 18), ("C", 36, 18, 60, 36, 60, 72), ("L", 60, 80), ("L", 0, 80)])
        s.path(dome, s.lin(CX - 40, 18, CX + 50, 84, [(0, shade(knit, 0.2)), (0.5, knit),
                                                       (1, shade(knit, -0.35))]))
        s.group(clip_path=s.clip(dome))
        for k in range(-6, 7):
            x = k * 9.0
            s.path([("M", x * 0.3, 18), ("Q", x * 0.95, 40, x * 1.05, 80)],
                   stroke=shade(knit, -0.3), stroke_width=1.6, opacity=0.4)
        s.end()
        cuff = [("M", -63, 76), ("C", -40, 66, 40, 66, 63, 76), ("L", 62, 96),
                ("C", 40, 88, -40, 88, -62, 96), ("Z",)]
        self.hairline_shadow(cuff, dy=4, op=0.55)
        cd = d(cuff)
        s.path(cd, s.lin(0, 68, 0, 96, [(0, shade(knit, 0.1)), (1, shade(knit, -0.25))]))
        s.group(clip_path=s.clip(cd))
        for k in range(-14, 15):
            x = k * 4.6
            s.path([("M", x, 64), ("L", x * 1.01, 100)], stroke=shade(knit, -0.35), stroke_width=1.6,
                   opacity=0.45)
        s.end()
        if self.L.get("pompom"):
            s.circle(CX, 16, 11, s.rad(CX - 4, 11, 14, [(0, shade(knit, 0.3)), (1, shade(knit, -0.2))]))

    # --- on top
    def glasses(self):
        s, g = self.s, self.g
        kind, col = self.L["glasses"]
        y = g.ey
        w = 2.4 if kind in ("bold", "rect") else 1.5
        if kind == "round":
            w = 2.0
        lens = []
        for side in (-1, 1):
            x = CX + side * (g.ex + 1)
            if kind == "round":
                lens.append(f"M{fmt(x - 13)} {fmt(y)} a13 13 0 1 0 26 0 a13 13 0 1 0 -26 0 Z")
            elif kind == "cat":
                o = side
                lens.append(
                    f"M{fmt(x - 14 * o)} {fmt(y - 7)} Q{fmt(x)} {fmt(y - 11)} {fmt(x + 16 * o)} {fmt(y - 12)} "
                    f"Q{fmt(x + 14 * o)} {fmt(y + 10)} {fmt(x)} {fmt(y + 10)} "
                    f"Q{fmt(x - 15 * o)} {fmt(y + 9)} {fmt(x - 14 * o)} {fmt(y - 7)} Z")
            else:
                hw_, hh = (15.0, 10.5) if kind != "thin" else (14.5, 11.0)
                r = 5.0 if kind == "rect" else 7.0
                lens.append(
                    f"M{fmt(x - hw_ + r)} {fmt(y - hh)} H{fmt(x + hw_ - r)} "
                    f"Q{fmt(x + hw_)} {fmt(y - hh)} {fmt(x + hw_)} {fmt(y - hh + r)} V{fmt(y + hh - r)} "
                    f"Q{fmt(x + hw_)} {fmt(y + hh)} {fmt(x + hw_ - r)} {fmt(y + hh)} H{fmt(x - hw_ + r)} "
                    f"Q{fmt(x - hw_)} {fmt(y + hh)} {fmt(x - hw_)} {fmt(y + hh - r)} V{fmt(y - hh + r)} "
                    f"Q{fmt(x - hw_)} {fmt(y - hh)} {fmt(x - hw_ + r)} {fmt(y - hh)} Z")
        # The frames' shadow on the face, the lenses, the frames.
        for ld in lens:
            s.path(ld, stroke=warm(self.skin, 0.6), stroke_width=w + 0.6, opacity=0.25,
                   transform="translate(1.5 2.5)", filter=s.blur(1.2))
        for ld in lens:
            s.path(ld, (255, 255, 255), opacity=0.1)
            s.group(clip_path=s.clip(ld))
            s.path(f"M{fmt(CX - 60)} {fmt(y + 20)} l60 -60", stroke=(255, 255, 255),
                   stroke_width=6, opacity=0.18)
            s.end()
            s.path(ld, stroke=col, stroke_width=w)
        bx = g.ex + 1 - (13 if kind == "round" else 14.5)
        s.path([("M", -bx, y - 3), ("Q", 0, y - 7, bx, y - 3)], stroke=col, stroke_width=w)
        for side in (-1, 1):
            arm = [("M", g.ex + 15.5, y - 4), ("L", g.hw + 1, y - 2)]
            s.path(arm if side > 0 else mirror_cmds(arm), stroke=col, stroke_width=w)

    def earrings(self):
        s, g = self.s, self.g
        kind = self.L["earrings"]
        if not kind:
            return
        hidden = self.L["hair"] in ("long", "wavy", "sleek", "bob", "afro")
        if hidden and kind in ("stud", "pearl"):
            return
        for side in (-1, 1):
            if kind == "ring_right" and side < 0:
                continue
            x = CX + side * (g.hw + 4)
            y = 143.0
            if hidden:
                x = CX + side * (g.hw + 1)
                y = 146.0
            if kind in ("hoop", "ring_right"):
                r = 6.5 if kind == "hoop" else 3.6
                s.circle(x, y + r - 1, r, "none", stroke=GOLD if kind == "hoop" else SILVER,
                         stroke_width=1.8)
                s.path(f"M{fmt(x - r * 0.7)} {fmt(y + r * 0.6)} a{fmt(r)} {fmt(r)} 0 0 0 {fmt(r * 1.2)} {fmt(r * 0.9)}",
                       stroke=(255, 250, 230), stroke_width=0.8, opacity=0.6)
            else:
                c = PEARL if kind == "pearl" else GOLD
                s.circle(x, y, 2.6 if kind == "pearl" else 2.0,
                         s.rad(x - 0.8, y - 0.8, 3, [(0, shade(c, 0.6)), (1, shade(c, -0.25))]))


# ----------------------------------------------------------------- render


def avatar_codes():
    src = open(os.path.join(ROOT, "crates/proto/src/lib.rs")).read()
    m = re.search(r"pub const AVATARS: \[&str; (\d+)\] = \[(.*?)\];", src, re.S)
    codes = re.findall(r'"([a-z_]+)"', m.group(2))
    assert len(codes) == int(m.group(1)), "AVATARS: the length doesn't match"
    return codes


def picture(code):
    if code in PEOPLE:
        return Person(code, PEOPLE[code]).draw()
    if code in OBJECTS:
        return draw_object(code)
    raise SystemExit(f"no drawing for {code}")


RENDER_JS = r"""
const { chromium } = require('playwright');
(async () => {
  const [html, out, w, h] = process.argv.slice(2);
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: +w, height: +h }, deviceScaleFactor: 1 });
  await page.goto('file://' + html);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: out, omitBackground: true, fullPage: true });
  await browser.close();
})().catch(e => { console.error(e); process.exit(1); });
"""


def render(svgs, tmp):
    """Rasterises the pictures (in a grid, one page) at S px each."""
    cols = COLS
    rows = (len(svgs) + cols - 1) // cols
    cells = "".join(f'<div class="c">{x}</div>' for x in svgs)
    html = (
        "<!doctype html><html><head><meta charset='utf-8'><style>"
        f"@font-face{{font-family:SLBold;src:url('file://{FONT}');}}"
        "html,body{margin:0;padding:0;background:transparent}"
        f"body{{display:grid;grid-template-columns:repeat({cols},{S}px);width:{cols * S}px}}"
        f".c{{width:{S}px;height:{S}px}} .c svg{{display:block}}"
        f"</style></head><body>{cells}</body></html>"
    )
    page = os.path.join(tmp, "sheet.html")
    with open(page, "w") as f:
        f.write(html)
    js = os.path.join(tmp, "render.js")
    with open(js, "w") as f:
        f.write(RENDER_JS)
    png = os.path.join(tmp, "sheet.png")
    subprocess.run(["node", js, page, png, str(cols * S), str(rows * S)], check=True, cwd=tmp)
    sheet = Image.open(png).convert("RGBA")
    return [sheet.crop(((i % cols) * S, (i // cols) * S, (i % cols + 1) * S, (i // cols + 1) * S))
            for i in range(len(svgs))]


def to_cell(img):
    """Shrinks a picture to a cell (premultiplied, so the clear corners
    don't darken the edge), and gives its clear pixels the colour of the
    edge (so the mipmaps of the atlas don't either)."""
    small = img.convert("RGBa").resize((CELL, CELL), Image.LANCZOS).convert("RGBA")
    px = small.load()
    # The rim's colour, from a pixel on the right edge, middle row.
    edge = None
    for x in range(CELL - 1, CELL // 2, -1):
        if px[x, CELL // 2][3] == 255:
            edge = px[x, CELL // 2][:3]
            break
    for y in range(CELL):
        for x in range(CELL):
            r, g, b, a = px[x, y]
            if a == 0 and edge:
                px[x, y] = edge + (0,)
    return small


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--keep", help="also write the SVGs and full-size pictures here")
    ap.add_argument("--only", help="comma-separated codes (with --keep: a quick look)")
    args = ap.parse_args()
    codes = avatar_codes()
    if args.only:
        codes = [c for c in codes if c in args.only.split(",")]
    svgs = [picture(c) for c in codes]
    with tempfile.TemporaryDirectory() as tmp:
        big = render(svgs, tmp)
    if args.keep:
        os.makedirs(args.keep, exist_ok=True)
        for code, svg, img in zip(codes, svgs, big):
            with open(os.path.join(args.keep, code + ".svg"), "w") as f:
                f.write(svg)
            img.save(os.path.join(args.keep, code + ".png"))
        rows = (len(big) + COLS - 1) // COLS
        sheet = Image.new("RGBA", (COLS * S, rows * S), (24, 18, 14, 255))
        for i, img in enumerate(big):
            sheet.alpha_composite(img, ((i % COLS) * S, (i // COLS) * S))
        sheet.save(os.path.join(args.keep, "_sheet.png"))
    if args.only:
        return
    rows = (len(big) + COLS - 1) // COLS
    atlas = Image.new("RGBA", (COLS * CELL, rows * CELL), (0, 0, 0, 0))
    for i, img in enumerate(big):
        atlas.paste(to_cell(img), ((i % COLS) * CELL, (i // COLS) * CELL))
    atlas.save(OUT, optimize=True)
    print(f"{len(codes)} pictures -> {OUT} ({os.path.getsize(OUT) // 1024} KiB)")


# --------------------------------------------------------------- objects

IVORY = (246, 242, 228)
BLACK = (22, 22, 24)
WOOD = (150, 96, 52)
CLOTH = (34, 110, 62)
GOLD_M = (242, 192, 64)
WHITE = (255, 255, 255)
# The classic set's colours (crates/proto/src/shop.rs): 1 to 7, 9 to 15.
BALL = [(255, 204, 10), (18, 58, 205), (228, 28, 24), (108, 36, 168), (255, 118, 12),
        (12, 142, 58), (112, 62, 30)]


def ball_color(n):
    return IVORY if n == 0 else BLACK if n == 8 else BALL[(n - 1) % 8]


def shadow(s, x, y, rx, ry, op=0.5):
    s.ellipse(x, y, rx, ry, (0, 0, 0), opacity=op, filter=s.blur(max(2.0, ry * 0.45)))


def ball(s, x, y, r, n, spot=None, number=True, shade_op=0.5):
    """A pool ball, `n` (0: the cue ball), lit from the top left."""
    c = ball_color(n)
    clip = s.clip(f'<circle cx="{fmt(x)}" cy="{fmt(y)}" r="{fmt(r)}"/>')
    stripe = n > 8
    base = IVORY if stripe or n == 0 else c
    s.circle(x, y, r, s.rad(x - r * 0.35, y - r * 0.4, r * 1.5,
                            [(0, shade(base, 0.35)), (0.45, base), (1, shade(base, -0.45))]))
    if stripe:
        s.group(clip_path=clip)
        band = (f"M{fmt(x - r * 1.1)} {fmt(y - r * 0.5)} Q{fmt(x)} {fmt(y - r * 0.62)} {fmt(x + r * 1.1)} {fmt(y - r * 0.5)} "
                f"L{fmt(x + r * 1.1)} {fmt(y + r * 0.5)} Q{fmt(x)} {fmt(y + r * 0.62)} {fmt(x - r * 1.1)} {fmt(y + r * 0.5)} Z")
        s.path(band, s.rad(x - r * 0.35, y - r * 0.4, r * 1.5,
                           [(0, shade(c, 0.3)), (0.45, c), (1, shade(c, -0.45))]))
        s.end()
    if n and number:
        s.circle(x, y, r * 0.44, (250, 247, 238))
        txt = str(n)
        size = r * (0.5 if len(txt) == 1 else 0.42)
        s.text(x, y + size * 0.36, txt, size, (24, 22, 24))
    if spot:
        s.circle(x + spot[0] * r, y + spot[1] * r, r * 0.1, (200, 36, 40))
    # Round: darker towards the edge, a highlight, a little light bounced
    # from the cloth at the bottom right.
    s.circle(x, y, r, s.rad(x - r * 0.3, y - r * 0.35, r * 1.4,
                            [(0, BLACK, 0), (0.55, BLACK, 0), (1, BLACK, shade_op)]))
    s.ellipse(x - r * 0.34, y - r * 0.44, r * 0.3, r * 0.17, WHITE, opacity=0.75,
              transform=f"rotate(-32 {fmt(x - r * 0.34)} {fmt(y - r * 0.44)})", filter=s.blur(r * 0.05))
    s.circle(x - r * 0.3, y - r * 0.42, r * 0.07, WHITE, opacity=0.95)
    s.path(f"M{fmt(x + r * 0.2)} {fmt(y + r * 0.86)} A{fmt(r * 0.9)} {fmt(r * 0.9)} 0 0 0 {fmt(x + r * 0.86)} {fmt(y + r * 0.2)}",
           stroke=WHITE, stroke_width=r * 0.06, opacity=0.18, stroke_linecap="round")


def cue(s, tip, butt, w0=7.0, w1=16.0, forearm=(84, 38, 24), points=(230, 196, 140)):
    """A cue from its tip to its butt."""
    (x0, y0), (x1, y1) = tip, butt
    ln = math.hypot(x1 - x0, y1 - y0)
    ux, uy = (x1 - x0) / ln, (y1 - y0) / ln
    nx, ny = -uy, ux

    def pt(t, side):
        w = w0 + (w1 - w0) * t
        return (x0 + ux * ln * t + nx * side * w / 2, y0 + uy * ln * t + ny * side * w / 2)

    def poly(t0, t1):
        pts = [pt(t0, -1), pt(t1, -1), pt(t1, 1), pt(t0, 1)]
        return "M" + " L".join(f"{fmt(a)} {fmt(b)}" for a, b in pts) + " Z"

    maple = (232, 200, 146)
    s.path(poly(0.0, 0.012), (60, 110, 200))
    s.path(poly(0.012, 0.04), (246, 242, 230))
    s.path(poly(0.04, 0.56), maple)
    s.path(poly(0.56, 0.575), (230, 232, 236))
    s.path(poly(0.575, 0.82), forearm)
    # The points of the forearm.
    for side in (-1, 1):
        a, b, c = pt(0.575, side * 0.85), pt(0.575, side * 0.05), pt(0.73, side * 0.5)
        s.path(f"M{fmt(a[0])} {fmt(a[1])} L{fmt(b[0])} {fmt(b[1])} L{fmt(c[0])} {fmt(c[1])} Z", points)
    s.path(poly(0.82, 0.96), (30, 30, 34))
    s.path(poly(0.96, 1.0), (14, 14, 16))
    mx, my = x0 + ux * ln * 0.5, y0 + uy * ln * 0.5
    w = (w0 + w1) / 2
    s.path(poly(0.0, 1.0), s.lin(mx - nx * w / 2, my - ny * w / 2, mx + nx * w / 2, my + ny * w / 2,
                                 [(0, WHITE, 0.4), (0.3, WHITE, 0.1), (0.55, BLACK, 0.0), (1, BLACK, 0.5)]))


def star_d(x, y, R, r, n=5, rot=-90.0):
    pts = []
    for k in range(2 * n):
        a = math.radians(rot + 180.0 * k / n)
        rad = R if k % 2 == 0 else r
        pts.append((x + math.cos(a) * rad, y + math.sin(a) * rad))
    return "M" + " L".join(f"{fmt(a)} {fmt(b)}" for a, b in pts) + " Z", pts


def bevel_star(s, x, y, R, r, c, n=5, rot=-90.0):
    """A star in relief: each point a lit face and a shaded one."""
    dd, pts = star_d(x, y, R, r, n, rot)
    s.path(dd, shade(c, -0.3), transform="translate(3 4)", opacity=0.5, filter=s.blur(3))
    for k in range(0, 2 * n, 2):
        tip = pts[k]
        for nb, kk in ((pts[k - 1], -1), (pts[(k + 1) % (2 * n)], 1)):
            # The face towards the light (top left) is lighter.
            mx, my = (tip[0] + nb[0]) / 2 - x, (tip[1] + nb[1]) / 2 - y
            light = -(mx + my) / (abs(mx) + abs(my) + 1e-6)
            col = shade(c, 0.28 * light) if light > 0 else shade(c, 0.32 * light)
            s.path(f"M{fmt(x)} {fmt(y)} L{fmt(tip[0])} {fmt(tip[1])} L{fmt(nb[0])} {fmt(nb[1])} Z", col,
                   stroke=col, stroke_width=0.4)


def gold(s, x0, y0, x1, y1, c=GOLD_M):
    return s.lin(x0, y0, x1, y1, [(0, shade(c, 0.45)), (0.35, c), (0.7, shade(c, -0.3)),
                                  (1, shade(c, 0.1))])


def crown(s, x, y, w, h, c=GOLD_M):
    """A crown standing on (x, y), `w` wide, `h` high."""
    hw = w / 2
    band_h = h * 0.3
    top = y - h
    pts = [(-hw, y - band_h), (-hw * 1.02, top + h * 0.12), (-hw * 0.55, y - band_h - h * 0.28),
           (0, top), (hw * 0.55, y - band_h - h * 0.28), (hw * 1.02, top + h * 0.12), (hw, y - band_h)]
    dd = "M" + " L".join(f"{fmt(x + a)} {fmt(b)}" for a, b in pts) + " Z"
    s.path(dd, gold(s, x - hw, top, x + hw, y))
    s.path(dd, "none", stroke=shade(c, -0.45), stroke_width=1.2, opacity=0.6)
    for (a, b) in (pts[1], pts[3], pts[5]):
        s.circle(x + a, b, w * 0.06, s.rad(x + a - 1, b - 1, w * 0.07, [(0, (255, 250, 230)), (1, shade(c, -0.1))]))
    s.el("rect", x=x - hw, y=y - band_h, width=w, height=band_h, rx=w * 0.03,
         fill=gold(s, x - hw, y - band_h, x + hw, y))
    s.el("rect", x=x - hw, y=y - band_h, width=w, height=band_h, rx=w * 0.03, fill="none",
         stroke=shade(c, -0.45), stroke_width=1.2, opacity=0.6)
    for k, col in ((-1, (220, 50, 70)), (0, (60, 130, 230)), (1, (40, 170, 100))):
        gx, gy = x + k * hw * 0.55, y - band_h / 2
        s.ellipse(gx, gy, w * 0.07, band_h * 0.3, s.rad(gx - 1.5, gy - 1.5, w * 0.09,
                                                         [(0, shade(col, 0.6)), (0.5, col), (1, shade(col, -0.4))]))


def flame_d(x, y, w, h, lean=0.0):
    """A flame: round at the bottom (x, y), pointed at the top."""
    hw = w / 2
    return (f"M{fmt(x)} {fmt(y)} C{fmt(x - hw * 1.1)} {fmt(y)} {fmt(x - hw * 1.15)} {fmt(y - h * 0.45)} "
            f"{fmt(x - hw * 0.6)} {fmt(y - h * 0.62)} C{fmt(x - hw * 0.4)} {fmt(y - h * 0.72)} "
            f"{fmt(x - hw * 0.1 + lean * w)} {fmt(y - h * 0.8)} {fmt(x + lean * w * 1.6)} {fmt(y - h)} "
            f"C{fmt(x + hw * 0.2 + lean * w)} {fmt(y - h * 0.78)} {fmt(x + hw * 0.5)} {fmt(y - h * 0.72)} "
            f"{fmt(x + hw * 0.7)} {fmt(y - h * 0.55)} C{fmt(x + hw * 1.15)} {fmt(y - h * 0.4)} "
            f"{fmt(x + hw * 1.1)} {fmt(y)} {fmt(x)} {fmt(y)} Z")


def flames(s, x, y, w, h, lean=0.0):
    for k, col in enumerate(((226, 74, 28), (246, 150, 36), (252, 214, 90), (255, 246, 200))):
        f = (1.0, 0.76, 0.52, 0.3)[k]
        s.path(flame_d(x, y - h * 0.02 * k, w * f, h * (f * 0.85 + 0.15), lean), col)


def sparkle(s, x, y, r, c=WHITE, op=0.9):
    dd = (f"M{fmt(x)} {fmt(y - r)} Q{fmt(x + r * 0.12)} {fmt(y - r * 0.12)} {fmt(x + r)} {fmt(y)} "
          f"Q{fmt(x + r * 0.12)} {fmt(y + r * 0.12)} {fmt(x)} {fmt(y + r)} "
          f"Q{fmt(x - r * 0.12)} {fmt(y + r * 0.12)} {fmt(x - r)} {fmt(y)} "
          f"Q{fmt(x - r * 0.12)} {fmt(y - r * 0.12)} {fmt(x)} {fmt(y - r)} Z")
    s.path(dd, c, opacity=op)


def arrow_arc(s, x, y, rx, ry, a0, a1, w, c):
    """A curved arrow round (x, y) from angle a0 to a1 (degrees)."""
    p = lambda a: (x + math.cos(math.radians(a)) * rx, y + math.sin(math.radians(a)) * ry)
    (sx, sy), (ex, ey) = p(a0), p(a1 - 8 * (1 if a1 > a0 else -1))
    sweep = 1 if a1 > a0 else 0
    s.path(f"M{fmt(sx)} {fmt(sy)} A{fmt(rx)} {fmt(ry)} 0 0 {sweep} {fmt(ex)} {fmt(ey)}", stroke=c,
           stroke_width=w, stroke_linecap="round")
    # The head: a triangle along the tangent at the end.
    tx, ty = p(a1)
    bx, by = ex, ey
    dx, dy = tx - bx, ty - by
    ln = math.hypot(dx, dy) or 1
    ux, uy = dx / ln, dy / ln
    hx_, hy = tx + ux * w * 0.8, ty + uy * w * 0.8
    lx, ly = bx - uy * w * 1.3, by + ux * w * 1.3
    rx_, ry_ = bx + uy * w * 1.3, by - ux * w * 1.3
    s.path(f"M{fmt(hx_)} {fmt(hy)} L{fmt(lx)} {fmt(ly)} L{fmt(rx_)} {fmt(ry_)} Z", c)


def obj_eight(s):
    shadow(s, 136, 200, 62, 14)
    ball(s, 128, 124, 74, 8)


def obj_cue_ball(s):
    shadow(s, 146, 186, 50, 11)
    ball(s, 146, 118, 58, 0, spot=(0.0, 0.0))
    cue(s, (100, 162), (-60, 322), 8, 18)


def obj_stripe(s):
    shadow(s, 136, 200, 62, 14)
    ball(s, 128, 124, 74, 9)


def obj_rack(s):
    # The wooden triangle and six balls in it.
    pts = [(128, 34), (214, 180), (42, 180)]
    shadow(s, 132, 190, 86, 10, 0.4)
    tri = "M" + " L".join(f"{a} {b}" for a, b in pts) + " Z"
    s.path(tri, "none", stroke=shade(WOOD, -0.45), stroke_width=17, stroke_linejoin="round",
           transform="translate(2 3)", opacity=0.6)
    s.path(tri, "none", stroke=s.lin(40, 30, 220, 190, [(0, shade(WOOD, 0.3)), (0.5, WOOD),
                                                        (1, shade(WOOD, -0.3))]),
           stroke_width=14, stroke_linejoin="round")
    s.path(tri, "none", stroke=shade(WOOD, 0.4), stroke_width=1.5, stroke_linejoin="round",
           opacity=0.6, transform="translate(-1 -1)")
    r = 22.5
    spots = [(128, 78, 1), (105, 117, 11), (151, 117, 3), (82, 156, 6), (128, 156, 8), (174, 156, 13)]
    for x, y, n in spots:
        ball(s, x, y, r, n, number=False, shade_op=0.55)


def obj_cues(s):
    for a, b in (((40, 40), (232, 232)), ((216, 40), (24, 232))):
        cue(s, a, (a[0] + (b[0] - a[0]) * 1.45, a[1] + (b[1] - a[1]) * 1.45), 7, 15)
    shadow(s, 132, 196, 34, 8)
    ball(s, 128, 164, 34, 8)


def obj_chalk(s):
    blue = (52, 112, 204)
    shadow(s, 132, 200, 74, 14)
    top = "M128 52 L206 90 L128 128 L50 90 Z"
    left = "M50 90 L128 128 L128 206 L50 168 Z"
    right = "M206 90 L128 128 L128 206 L206 168 Z"
    s.path(left, s.lin(50, 90, 128, 206, [(0, shade(blue, -0.15)), (1, shade(blue, -0.35))]))
    s.path(right, s.lin(128, 128, 206, 168, [(0, shade(blue, -0.42)), (1, shade(blue, -0.55))]))
    s.path(top, s.lin(50, 52, 206, 128, [(0, shade(blue, 0.25)), (1, blue)]))
    # The hollow the tips have worn, chalk dust, the paper's edge.
    s.ellipse(128, 90, 28, 13, shade(blue, -0.35))
    s.ellipse(126, 88, 22, 9, s.rad(126, 92, 22, [(0, shade(blue, 0.3)), (1, shade(blue, -0.2))]))
    for x0, y0, x1, y1 in ((50, 168, 128, 206), (128, 206, 206, 168)):
        s.path(f"M{x0} {y0 - 26} L{x1} {y1 - 26}", stroke=WHITE, stroke_width=1.6, opacity=0.25)
    s.path("M50 90 L128 128 L206 90", stroke=shade(blue, 0.45), stroke_width=1.6, opacity=0.6)
    s.path("M128 128 L128 206", stroke=shade(blue, 0.2), stroke_width=1.2, opacity=0.5)


def trophy_cup(s, c, star=True, handles=True, x=128):
    shadow(s, x + 4, 214, 52, 8)
    # The base, the stem, the cup, the handles.
    s.el("rect", x=x - 44.0, y=188.0, width=88.0, height=22.0, rx=4.0,
         fill=s.lin(0, 188, 0, 210, [(0, (96, 60, 36)), (1, (52, 32, 20))]))
    s.el("rect", x=x - 30.0, y=194.0, width=60.0, height=8.0, rx=2.0, fill=gold(s, x - 30, 194, x + 30, 202, c))
    s.path(f"M{x - 26} 188 L{x + 26} 188 L{x + 18} 176 L{x - 18} 176 Z", gold(s, x - 26, 176, x + 26, 188, c))
    s.path(f"M{x - 8} 176 L{x + 8} 176 L{x + 6} 140 L{x - 6} 140 Z", gold(s, x - 8, 140, x + 8, 176, c))
    s.ellipse(x, 140, 16, 5, gold(s, x - 16, 135, x + 16, 145, c))
    if handles:
        for side in (-1, 1):
            h = (f"M{x + side * 40} 62 C{x + side * 86} 58 {x + side * 84} 118 {x + side * 34} 124 "
                 f"L{x + side * 34} 112 C{x + side * 70} 106 {x + side * 70} 72 {x + side * 40} 76 Z")
            s.path(h, gold(s, x - 80, 60, x + 80, 124, c))
    cup = (f"M{x - 54} 46 L{x + 54} 46 C{x + 54} 96 {x + 40} 134 {x} 140 C{x - 40} 134 {x - 54} 96 {x - 54} 46 Z")
    s.path(cup, s.lin(x - 54, 0, x + 54, 0, [(0, shade(c, -0.1)), (0.22, shade(c, 0.55)), (0.45, c),
                                             (0.8, shade(c, -0.35)), (1, shade(c, -0.15))]))
    s.ellipse(x, 46, 54, 9, s.lin(0, 37, 0, 55, [(0, shade(c, -0.4)), (1, shade(c, 0.2))]))
    s.ellipse(x, 47, 48, 6, shade(c, -0.5))
    if star:
        bevel_star(s, x, 88, 24, 10, shade(c, 0.1))


def obj_trophy(s):
    trophy_cup(s, GOLD_M)


def obj_silver_cup(s):
    trophy_cup(s, (200, 206, 214), star=False)
    ball(s, 128, 88, 17, 8)


def obj_star(s):
    shadow(s, 132, 204, 58, 9, 0.35)
    bevel_star(s, 128, 126, 92, 38, GOLD_M, rot=-90)
    sparkle(s, 196, 56, 12)
    sparkle(s, 58, 196, 8, op=0.7)


def obj_crown(s):
    shadow(s, 132, 192, 80, 10)
    crown(s, 128, 184, 168, 124)


def obj_table(s):
    # A table from above: rails, cloth, pockets, diamonds, balls.
    shadow(s, 132, 196, 100, 12, 0.4)
    s.el("rect", x=26.0, y=62.0, width=204.0, height=132.0, rx=16.0,
         fill=s.lin(26, 62, 230, 194, [(0, shade(WOOD, 0.25)), (0.5, WOOD), (1, shade(WOOD, -0.35))]))
    s.el("rect", x=42.0, y=78.0, width=172.0, height=100.0, rx=4.0,
         fill=s.rad(110, 110, 120, [(0, (60, 160, 96)), (1, (30, 112, 64))]))
    s.el("rect", x=42.0, y=78.0, width=172.0, height=100.0, rx=4.0, fill="none", stroke=(20, 80, 40),
         stroke_width=4, opacity=0.6)
    for x, y in ((44, 80), (128, 76), (212, 80), (44, 176), (128, 180), (212, 176)):
        s.circle(x, y, 9, s.rad(x, y, 9, [(0, (0, 0, 0)), (1, (40, 30, 26))]))
    for k in (1, 2, 3, 5, 6, 7):
        x = 42 + 172 * k / 8
        for y in (70, 186):
            s.circle(x, y, 1.8, (244, 236, 210))
    for k in (1, 2, 3):
        y = 78 + 100 * k / 4
        for x in (34, 222):
            s.circle(x, y, 1.8, (244, 236, 210))
    shadow(s, 92, 134, 10, 4, 0.4)
    ball(s, 90, 128, 10, 0, number=False)
    for x, y, n in ((150, 116, 3), (170, 140, 8), (148, 146, 1)):
        ball(s, x, y, 10, n, number=False)
    cue(s, (76, 140), (-80, 210), 3, 7)


def obj_flame(s):
    s.ellipse(128, 150, 70, 70, (255, 140, 40), opacity=0.35, filter=s.blur(20))
    flames(s, 128, 206, 128, 176)


def obj_bolt(s):
    c = (250, 206, 60)
    bolt = "M150 28 L72 140 L122 140 L100 230 L188 106 L136 106 L168 28 Z"
    s.ellipse(128, 128, 60, 80, c, opacity=0.25, filter=s.blur(18))
    s.path(bolt, shade(c, -0.45), transform="translate(4 5)", opacity=0.6)
    s.path(bolt, s.lin(80, 30, 180, 220, [(0, shade(c, 0.5)), (0.5, c), (1, shade(c, -0.25))]))
    s.path("M150 28 L72 140 L122 140", stroke=shade(c, 0.7), stroke_width=2.4, opacity=0.8,
           stroke_linejoin="round")


def obj_medal(s):
    # The ribbon, in a V from the top, then the medal.
    for side, col in ((-1, (200, 44, 52)), (1, (40, 70, 160))):
        rib = (f"M{128 + side * 4} 132 L{128 + side * 52} 18 L{128 + side * 84} 18 L{128 + side * 34} 132 Z")
        s.path(rib, col)
        s.path(f"M{128 + side * 22} 132 L{128 + side * 68} 18", stroke=(250, 220, 120), stroke_width=5)
    s.path("M86 18 L170 18 L128 104 Z", (0, 0, 0), opacity=0.25, filter=s.blur(4))
    shadow(s, 134, 220, 50, 8, 0.4)
    s.circle(131, 158, 58, (0, 0, 0), opacity=0.35, filter=s.blur(4))
    s.circle(128, 154, 58, gold(s, 70, 96, 186, 212))
    s.circle(128, 154, 46, s.lin(82, 108, 174, 200, [(0, shade(GOLD_M, -0.25)), (1, shade(GOLD_M, 0.35))]))
    bevel_star(s, 128, 156, 34, 14, GOLD_M)
    s.circle(128, 154, 52, "none", stroke=shade(GOLD_M, -0.35), stroke_width=1.2, opacity=0.6)


def laurel_branch(s, side, c):
    # Leaves along an arc from the bottom up one side.
    for k in range(9):
        t = k / 8
        a = math.radians(90 - side * (24 + 128 * t))
        x, y = 128 + math.cos(a) * 82, 132 + math.sin(a) * 82
        tang = math.degrees(a) - side * 90
        ln = 30 - 8 * t
        for out in (-1, 1):
            ang = tang + out * 28 * side
            s.ellipse(x, y, ln / 2, ln / 5.2, gold(s, x - 15, y - 15, x + 15, y + 15, c),
                      transform=f"rotate({fmt(ang)} {fmt(x)} {fmt(y)}) translate(0 {fmt(out * ln * 0.18)})")
    pts = []
    for k in range(17):
        t = k / 16
        a = math.radians(90 - side * (20 + 136 * t))
        pts.append((128 + math.cos(a) * 82, 132 + math.sin(a) * 82))
    s.path("M" + " L".join(f"{fmt(a)} {fmt(b)}" for a, b in pts), stroke=shade(c, -0.35), stroke_width=3,
           stroke_linecap="round")


def obj_laurel(s):
    c = GOLD_M
    for side in (-1, 1):
        laurel_branch(s, side, c)
    s.text(131, 166, "1", 92, (0, 0, 0), opacity=0.3, filter=s.blur(3))
    s.text(128, 162, "1", 92, gold(s, 100, 90, 156, 170))


def obj_one(s):
    shadow(s, 136, 200, 62, 14)
    ball(s, 128, 124, 74, 1)


def obj_fifteen(s):
    shadow(s, 136, 200, 62, 14)
    ball(s, 128, 124, 74, 15)


def obj_fire_eight(s):
    s.ellipse(128, 120, 90, 90, (255, 120, 30), opacity=0.35, filter=s.blur(20))
    flames(s, 92, 196, 92, 170, -0.08)
    flames(s, 164, 196, 92, 160, 0.08)
    flames(s, 128, 190, 120, 196, 0.0)
    shadow(s, 132, 214, 52, 8)
    ball(s, 128, 160, 56, 8)


def obj_spin(s):
    shadow(s, 134, 196, 54, 12)
    ball(s, 128, 126, 60, 0, spot=(-0.05, 0.42))
    c = (250, 214, 80)
    arrow_arc(s, 128, 126, 94, 34, 200, 340, 8, c)
    arrow_arc(s, 128, 126, 94, 34, 20, 160, 8, c)


def obj_pocket(s):
    # A corner of the table from above: rails along the top and the
    # right, the pocket in the corner, a ball dropping in.
    s.path("M0 0 L256 0 L256 256 L0 256 Z", s.rad(80, 170, 220, [(0, (56, 156, 90)), (1, (26, 96, 54))]))
    rail = s.lin(0, 0, 256, 256, [(0, shade(WOOD, 0.3)), (1, shade(WOOD, -0.3))])
    s.path("M0 0 L256 0 L256 256 L206 256 L206 50 L0 50 Z", rail)
    # The cushions, in a darker green, cut back at the pocket.
    s.path("M0 50 L150 50 L140 64 L0 64 Z", (30, 106, 58))
    s.path("M206 106 L206 256 L192 256 L192 116 Z", (30, 106, 58))
    for x in (40, 100):
        s.ellipse(x, 25, 4, 2.5, (244, 236, 210))
    for y in (170, 228):
        s.ellipse(231, y, 2.5, 4, (244, 236, 210))
    s.circle(186, 70, 40, s.rad(186, 70, 40, [(0, (0, 0, 0)), (0.75, (16, 12, 10)), (1, (50, 36, 26))]))
    s.circle(186, 70, 40, "none", stroke=(24, 18, 14), stroke_width=4)
    s.group(clip_path=s.clip('<circle cx="186" cy="70" r="38"/>'))
    ball(s, 168, 88, 30, 5)
    s.circle(186, 70, 40, s.rad(196, 60, 46, [(0, BLACK, 0.0), (0.45, BLACK, 0.1), (1, BLACK, 0.9)]))
    s.end()
    shadow(s, 92, 182, 26, 7, 0.4)
    ball(s, 84, 170, 26, 0)
    for k in range(3):
        o = (k - 1) * 13
        s.path(f"M{fmt(122 + o * 0.7)} {fmt(146 + o * 0.7)} l26 -26", stroke=WHITE, stroke_width=3,
               opacity=0.5, stroke_linecap="round")


def obj_lamp(s):
    # The lamp over the table: chains, a green shade, its light below.
    s.path("M128 0 L128 40", stroke=(170, 150, 110), stroke_width=4)
    s.path("M70 236 L102 108 L154 108 L186 236 Z", s.lin(0, 108, 0, 236, [(0, (255, 236, 170), 0.55),
                                                                           (1, (255, 236, 170), 0.0)]))
    s.ellipse(128, 236, 92, 22, (40, 140, 76))
    s.ellipse(128, 236, 92, 22, s.rad(128, 236, 92, [(0, (255, 240, 180), 0.5), (1, (255, 240, 180), 0.0)], sy=0.24))
    for x, n in ((104, 3), (140, 8), (164, 1)):
        ball(s, x, 228, 9, n, number=False)
    shade_c = (36, 120, 70)
    lamp = "M128 40 C150 40 162 56 166 78 C180 84 196 96 200 112 L56 112 C60 96 76 84 90 78 C94 56 106 40 128 40 Z"
    s.path(lamp, s.lin(56, 40, 200, 112, [(0, shade(shade_c, 0.3)), (0.4, shade_c), (1, shade(shade_c, -0.4))]))
    s.path("M56 112 L200 112 L196 118 L60 118 Z", (230, 192, 90))
    s.ellipse(128, 116, 60, 7, (255, 246, 200))
    s.ellipse(128, 116, 70, 14, (255, 240, 170), opacity=0.5, filter=s.blur(6))
    s.path("M96 62 C104 50 116 46 126 46", stroke=WHITE, stroke_width=4, opacity=0.35, stroke_linecap="round")
    s.ellipse(128, 40, 10, 5, (200, 170, 100))


def obj_tip(s):
    # A cue's tip against a cube of chalk, blue dust in the air.
    blue = (52, 112, 204)
    cx, cy = 160, 92
    top = f"M{cx} {cy - 44} L{cx + 50} {cy - 20} L{cx} {cy + 4} L{cx - 50} {cy - 20} Z"
    left = f"M{cx - 50} {cy - 20} L{cx} {cy + 4} L{cx} {cy + 60} L{cx - 50} {cy + 36} Z"
    right = f"M{cx + 50} {cy - 20} L{cx} {cy + 4} L{cx} {cy + 60} L{cx + 50} {cy + 36} Z"
    s.path(left, s.lin(cx - 50, 0, cx, 0, [(0, shade(blue, -0.12)), (1, shade(blue, -0.3))]))
    s.path(right, shade(blue, -0.48))
    s.path(top, s.lin(cx - 50, cy - 44, cx + 50, cy, [(0, shade(blue, 0.3)), (1, blue)]))
    s.ellipse(cx, cy - 20, 18, 8, shade(blue, -0.35))
    s.path(f"M{cx - 50} {cy - 20} L{cx} {cy + 4} L{cx + 50} {cy - 20}", stroke=shade(blue, 0.45),
           stroke_width=1.4, opacity=0.6)
    for k in range(16):
        a = math.radians(110 + k * 23)
        r = 18 + (k * 29) % 30
        s.circle(112 + math.cos(a) * r * 0.8, 136 + math.sin(a) * r * 0.6, 1.5 + (k % 3) * 0.9,
                 shade(blue, 0.35), opacity=0.6)
    cue(s, (116, 128), (-90, 334), 18, 34)


def obj_diamond(s):
    s.ellipse(128, 120, 80, 70, (180, 230, 255), opacity=0.25, filter=s.blur(18))
    top_y, gird_y, bot_y = 62, 104, 210
    L, R = 34, 222
    tl, tr = 78, 178
    facets = [
        # The crown.
        ((L, gird_y), (tl, top_y), (103, gird_y), (220, 246, 255)),
        ((tl, top_y), (128, top_y), (103, gird_y), (160, 220, 250)),
        ((128, top_y), (153, gird_y), (103, gird_y), (236, 250, 255)),
        ((128, top_y), (tr, top_y), (153, gird_y), (150, 210, 245)),
        ((tr, top_y), (R, gird_y), (153, gird_y), (110, 180, 230)),
        # The pavilion.
        ((L, gird_y), (103, gird_y), (128, bot_y), (130, 200, 245)),
        ((103, gird_y), (153, gird_y), (128, bot_y), (200, 238, 255)),
        ((153, gird_y), (R, gird_y), (128, bot_y), (80, 150, 215)),
    ]
    s.path(f"M{L} {gird_y} L{tl} {top_y} L{tr} {top_y} L{R} {gird_y} L128 {bot_y} Z", (0, 0, 0),
           opacity=0.35, transform="translate(4 6)", filter=s.blur(4))
    for a, b, c, col in facets:
        s.path(f"M{a[0]} {a[1]} L{b[0]} {b[1]} L{c[0]} {c[1]} Z", col, stroke=(240, 252, 255),
               stroke_width=1.2, stroke_linejoin="round")
    s.path(f"M{L} {gird_y} L{R} {gird_y}", stroke=WHITE, stroke_width=2)
    for a, b in (((78, gird_y), (128, bot_y)), ((178, gird_y), (128, bot_y))):
        s.path(f"M{a[0]} {a[1]} L{b[0]} {b[1]}", stroke=WHITE, stroke_width=1, opacity=0.6)
    sparkle(s, 186, 66, 16)
    sparkle(s, 70, 172, 10, op=0.7)


def obj_shield(s):
    c = (40, 64, 140)
    sh = "M128 28 C160 40 186 40 210 34 C214 120 190 186 128 230 C66 186 42 120 46 34 C70 40 96 40 128 28 Z"
    s.path(sh, (0, 0, 0), opacity=0.4, transform="translate(4 6)", filter=s.blur(4))
    s.path(sh, gold(s, 46, 28, 210, 230))
    inner = "M128 44 C156 54 178 54 196 50 C198 122 178 176 128 212 C78 176 58 122 60 50 C78 54 100 54 128 44 Z"
    s.path(inner, s.lin(60, 44, 196, 212, [(0, shade(c, 0.25)), (1, shade(c, -0.35))]))
    ball(s, 128, 118, 44, 8)
    for side in (-1, 1):
        sparkle(s, 128 + side * 46, 178 - 16, 6, (250, 220, 120), op=0.9)


def obj_target(s):
    shadow(s, 132, 222, 70, 8, 0.4)
    for k, col in enumerate(((236, 236, 230), (210, 44, 44), (236, 236, 230), (210, 44, 44), (236, 236, 230))):
        r = 98 - k * 19
        s.circle(128, 124, r, s.rad(110, 106, r * 1.6, [(0, shade(col, 0.15)), (1, shade(col, -0.25))]))
    s.circle(128, 124, 98, "none", stroke=(60, 40, 40), stroke_width=2, opacity=0.4)
    ball(s, 128, 124, 22, 8, number=False)


def obj_rocket(s):
    for k in range(18):
        x, y = (k * 97) % 230 + 13, (k * 61) % 220 + 18
        s.circle(x, y, 1.2 + (k % 3) * 0.6, WHITE, opacity=0.5 + 0.1 * (k % 4))
    s.group(transform="rotate(40 128 128)")
    flames(s, 128, 252, 40, 70)
    body = "M128 24 C160 52 166 100 162 176 L94 176 C90 100 96 52 128 24 Z"
    s.path(body, s.lin(94, 0, 162, 0, [(0, (190, 196, 210)), (0.35, (250, 250, 252)), (1, (150, 156, 172))]))
    red = (214, 52, 52)
    s.path("M128 24 C144 38 152 54 156 70 L100 70 C104 54 112 38 128 24 Z",
           s.lin(100, 0, 156, 0, [(0, shade(red, 0.2)), (1, shade(red, -0.3))]))
    for side in (-1, 1):
        fin = f"M{128 + side * 33} 132 L{128 + side * 58} 178 L{128 + side * 58} 196 L{128 + side * 34} 176 Z"
        s.path(fin, shade(red, -0.1 if side < 0 else -0.3))
    s.path("M112 176 L144 176 L138 190 L118 190 Z", (90, 92, 100))
    ball(s, 128, 106, 20, 8, number=True)
    s.circle(128, 106, 21, "none", stroke=(120, 126, 140), stroke_width=4)
    s.end()


def obj_rosette(s):
    blue = (40, 90, 200)
    for side in (-1, 1):
        tail = (f"M{128 + side * 6} 150 L{128 + side * 46} 236 L{128 + side * 30} 228 "
                f"L{128 + side * 22} 246 L{128 + side * -10} 160 Z")
        s.path(tail, shade(blue, -0.15 if side < 0 else -0.3))
    n = 28
    pts = []
    for k in range(2 * n):
        a = math.tau * k / (2 * n)
        r = 92 if k % 2 == 0 else 80
        pts.append((128 + math.cos(a) * r, 112 + math.sin(a) * r))
    pleat = "M" + " L".join(f"{fmt(a)} {fmt(b)}" for a, b in pts) + " Z"
    s.path(pleat, (0, 0, 0), opacity=0.35, transform="translate(3 5)", filter=s.blur(4))
    s.path(pleat, s.rad(110, 94, 110, [(0, shade(blue, 0.3)), (1, shade(blue, -0.35))]))
    for k in range(n):
        a = math.tau * k / n
        s.path(f"M{fmt(128 + math.cos(a) * 54)} {fmt(112 + math.sin(a) * 54)} L{fmt(128 + math.cos(a) * 88)} "
               f"{fmt(112 + math.sin(a) * 88)}", stroke=shade(blue, -0.45), stroke_width=1.4, opacity=0.6)
    s.circle(128, 112, 54, (240, 240, 246))
    s.circle(128, 112, 46, gold(s, 82, 66, 174, 158))
    s.circle(128, 112, 46, "none", stroke=shade(GOLD_M, -0.4), stroke_width=1.4)
    s.text(128, 134, "1", 60, shade(GOLD_M, -0.55))


def obj_king_eight(s):
    shadow(s, 134, 210, 58, 12)
    ball(s, 128, 148, 66, 8)
    crown(s, 128, 92, 104, 72)


def obj_cues_star(s):
    for a, b in (((40, 40), (232, 232)), ((216, 40), (24, 232))):
        cue(s, a, (a[0] + (b[0] - a[0]) * 1.45, a[1] + (b[1] - a[1]) * 1.45), 7, 15)
    s.circle(128, 128, 50, (0, 0, 0), opacity=0.3, filter=s.blur(8))
    bevel_star(s, 128, 128, 56, 23, GOLD_M)


def obj_planet(s):
    for k in range(20):
        x, y = (k * 89) % 236 + 10, (k * 53) % 228 + 14
        s.circle(x, y, 1.0 + (k % 3) * 0.6, WHITE, opacity=0.45 + 0.1 * (k % 4))
    ring = s.lin(20, 0, 236, 0, [(0, (200, 160, 90)), (0.5, (250, 220, 150)), (1, (170, 120, 70))])
    # The ring behind the ball, the ball, the ring in front.
    s.ellipse(128, 132, 112, 28, "none", stroke=ring, stroke_width=11, transform="rotate(-18 128 132)")
    ball(s, 128, 132, 62, 8)
    s.group(clip_path=s.clip('<rect x="0" y="132" width="256" height="140" transform="rotate(-18 128 132)"/>'))
    s.ellipse(128, 132, 112, 28, "none", stroke=ring, stroke_width=11, transform="rotate(-18 128 132)")
    s.end()


def obj_comet(s):
    for k in range(14):
        x, y = (k * 101) % 230 + 13, (k * 67) % 220 + 18
        s.circle(x, y, 1.0 + (k % 3) * 0.5, WHITE, opacity=0.5)
    tail = "M168 84 C120 100 70 150 26 236 C80 176 128 140 186 120 Z"
    s.path(tail, s.lin(176, 100, 30, 230, [(0, (120, 200, 255), 0.9), (1, (120, 200, 255), 0.0)]))
    tail2 = "M172 96 C130 116 90 160 60 220 C100 170 136 144 182 116 Z"
    s.path(tail2, s.lin(176, 104, 60, 220, [(0, WHITE, 0.9), (1, WHITE, 0.0)]))
    s.circle(170, 98, 50, (140, 210, 255), opacity=0.4, filter=s.blur(10))
    ball(s, 170, 98, 42, 0)


def obj_break(s):
    # The break: the cue ball into the rack, balls flying.
    s.circle(150, 98, 40, (255, 255, 230), opacity=0.25, filter=s.blur(10))
    flying = [(196, 52, 3, 22), (214, 120, 6, 20), (176, 168, 2, 20), (102, 50, 5, 18), (150, 28, 4, 16),
              (214, 196, 7, 18)]
    for x, y, n, r in flying:
        dx, dy = x - 150, y - 98
        ln = math.hypot(dx, dy)
        ux, uy = dx / ln, dy / ln
        for k in range(2):
            o = (k - 0.5) * r * 0.9
            s.path(f"M{fmt(x - ux * r * 1.3 - uy * o)} {fmt(y - uy * r * 1.3 + ux * o)} "
                   f"l{fmt(-ux * r * 1.6)} {fmt(-uy * r * 1.6)}", stroke=WHITE, stroke_width=2.4,
                   opacity=0.5, stroke_linecap="round")
        ball(s, x, y, r, n, number=False)
    ball(s, 146, 104, 26, 8, number=False)
    ball(s, 120, 112, 24, 1, number=False)
    for k in range(3):
        o = (k - 1) * 14
        s.path(f"M{fmt(84 + o * 0.6)} {fmt(176 + o * 0.4)} l-26 22", stroke=WHITE, stroke_width=3,
               opacity=0.55, stroke_linecap="round")
    ball(s, 88, 168, 26, 0)


OBJECTS = {
    "pic_eight": (CLOTH, obj_eight),
    "pic_cue_ball": (CLOTH, obj_cue_ball),
    "pic_stripe": (CLOTH, obj_stripe),
    "pic_rack": (CLOTH, obj_rack),
    "pic_cues": ((92, 58, 34), obj_cues),
    "pic_chalk": ((92, 58, 34), obj_chalk),
    "pic_trophy": ((58, 46, 110), obj_trophy),
    "pic_star": ((30, 62, 120), obj_star),
    "pic_crown": ((58, 46, 110), obj_crown),
    "pic_table": (CLOTH, obj_table),
    "pic_flame": ((52, 34, 40), obj_flame),
    "pic_bolt": ((36, 44, 78), obj_bolt),
    "pic_medal": ((36, 58, 110), obj_medal),
    "pic_laurel": ((40, 84, 60), obj_laurel),
    "pic_one": ((30, 80, 150), obj_one),
    "pic_fifteen": ((20, 110, 120), obj_fifteen),
    "pic_fire_eight": ((50, 26, 30), obj_fire_eight),
    "pic_spin": ((30, 90, 130), obj_spin),
    "pic_pocket": ((26, 90, 52), obj_pocket),
    "pic_lamp": ((28, 32, 40), obj_lamp),
    "pic_tip": ((70, 54, 96), obj_tip),
    "pic_diamond": ((40, 50, 110), obj_diamond),
    "pic_shield": ((120, 32, 44), obj_shield),
    "pic_target": ((40, 70, 90), obj_target),
    "pic_rocket": ((28, 30, 72), obj_rocket),
    "pic_rosette": ((196, 140, 56), obj_rosette),
    "pic_king_eight": ((90, 40, 110), obj_king_eight),
    "pic_cues_star": ((20, 64, 74), obj_cues_star),
    "pic_planet": ((24, 22, 56), obj_planet),
    "pic_comet": ((20, 40, 84), obj_comet),
    "pic_break": ((30, 120, 70), obj_break),
    "pic_silver_cup": ((30, 60, 90), obj_silver_cup),
}


def draw_object(code):
    bg, fn = OBJECTS[code]
    s = Svg(code)
    disc_open(s, bg)
    fn(s)
    disc_close(s, bg)
    return s.svg()


if __name__ == "__main__":
    main()
