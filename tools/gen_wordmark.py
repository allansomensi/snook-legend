#!/usr/bin/env python3
"""Generates the game's wordmark (crates/client/assets/wordmark.png): the
name in Inter Display Black, in gold lit from above, with a fine dark edge
and a soft shadow, cropped to the letters (plus the shadow's room). The
menu draws it at the height of its title, mipmapped.

Requirements: `pip install pillow numpy` and Inter Display Black (from the
Inter release, https://github.com/rsms/inter; set INTER to its
extras/ttf folder).

    INTER=~/Downloads/Inter-4.1/extras/ttf python3 tools/gen_wordmark.py
"""
import os

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates/client/assets/wordmark.png")
INTER = os.path.expanduser(os.environ.get("INTER", "~/Downloads/Inter/extras/ttf"))

TEXT = "SNOOK LEGEND"
SIZE = 256  # font size: capitals about 186 px tall
TRACK = 0.02  # extra room between letters, in em
PAD = 28  # room around the letters for the edge and the shadow


def main():
    font = ImageFont.truetype(os.path.join(INTER, "InterDisplay-Black.ttf"), SIZE)
    # The letters, one by one with a little tracking, as a mask.
    widths = [font.getlength(c) for c in TEXT]
    w = int(sum(widths) + TRACK * SIZE * (len(TEXT) - 1)) + 2 * PAD
    h = SIZE + 2 * PAD
    mask = Image.new("L", (w, h), 0)
    d = ImageDraw.Draw(mask)
    x = PAD
    for c, cw in zip(TEXT, widths):
        d.text((x, PAD), c, font=font, fill=255)
        x += cw + TRACK * SIZE
    box = mask.getbbox()
    top, bottom = box[1], box[3]

    m = np.asarray(mask, dtype=np.float32) / 255.0
    # Gold lit from above: pale at the top of the capitals, deep at the foot.
    ys = np.clip((np.arange(h, dtype=np.float32) - top) / max(bottom - top, 1), 0.0, 1.0)[:, None]
    stops = [
        (0.0, (255, 236, 176)),
        (0.42, (238, 196, 102)),
        (0.58, (214, 160, 66)),
        (1.0, (178, 120, 42)),
    ]
    rgb = np.zeros((h, w, 3), dtype=np.float32)
    for (a, ca), (b, cb) in zip(stops, stops[1:]):
        k = np.clip((ys - a) / (b - a), 0.0, 1.0)
        inside = (ys >= a) & (ys <= b)
        for i in range(3):
            rgb[:, :, i] = np.where(inside, ca[i] + (cb[i] - ca[i]) * k, rgb[:, :, i])
    # A fine dark edge (the letters grown by a few pixels, under them).
    edge = np.asarray(mask.filter(ImageFilter.MaxFilter(7)), dtype=np.float32) / 255.0
    # A soft shadow below.
    shadow_img = Image.fromarray((edge * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(9))
    shadow = np.roll(np.asarray(shadow_img, dtype=np.float32) / 255.0, 8, axis=0)

    out = np.zeros((h, w, 4), dtype=np.float32)
    # Shadow, then edge, then the gold, each over the last.
    layers = [
        (np.zeros((h, w, 3)) + 0.0, shadow * 0.55),
        (np.zeros((h, w, 3)) + [38, 22, 8], edge * 0.9),
        (rgb, m),
    ]
    for color, alpha in layers:
        a = alpha[:, :, None]
        out_a = out[:, :, 3:4]
        new_a = a + out_a * (1.0 - a)
        out[:, :, :3] = np.where(
            new_a > 0, (color * a + out[:, :, :3] * out_a * (1.0 - a)) / np.maximum(new_a, 1e-6), 0.0
        )
        out[:, :, 3:4] = new_a
    rgba = np.dstack([np.clip(out[:, :, :3], 0, 255), np.clip(out[:, :, 3] * 255, 0, 255)]).astype(np.uint8)
    img = Image.fromarray(rgba, "RGBA")
    # Cropped to what shows (the shadow's faint tail dropped)…
    a = np.asarray(img)[:, :, 3]
    ys, xs = np.where(a > 3)
    img = img.crop((xs.min(), ys.min(), xs.max() + 1, ys.max() + 1))
    cap_top, cap_h = top - ys.min(), bottom - top
    # …made to fit 2048 x 256 (sides a power of two, so every driver can
    # mipmap it), in its top-left corner.
    k = min(1.0, (W - 2) / img.width, (H - 2) / img.height)
    size = (round(img.width * k), round(img.height * k))
    img = img.convert("RGBa").resize(size, Image.LANCZOS).convert("RGBA")
    canvas = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    canvas.paste(img, (0, 0))
    bleed(canvas).save(OUT, optimize=True)
    print(f"{size[0]}x{size[1]} in {W}x{H}, capitals from y={cap_top * k:.1f}, {cap_h * k:.1f} tall")


W, H = 2048, 256


def bleed(img):
    """The clear pixels take the colour of the nearest ink (so filtering and
    mipmaps don't darken the edges with black)."""
    px = np.asarray(img, dtype=np.float32).copy()
    known = px[:, :, 3] > 0
    color = px[:, :, :3]
    for _ in range(64):
        new = ~known
        acc = np.zeros_like(color)
        n = np.zeros(known.shape, dtype=np.float32)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            src = np.roll(np.roll(known, dy, 0), dx, 1)
            acc += np.roll(np.roll(color, dy, 0), dx, 1) * src[:, :, None]
            n += src
        fill = new & (n > 0)
        if not fill.any():
            break
        color[fill] = acc[fill] / n[fill][:, None]
        known = known | fill
    px[:, :, :3] = color
    return Image.fromarray(px.astype(np.uint8), "RGBA")


if __name__ == "__main__":
    main()
