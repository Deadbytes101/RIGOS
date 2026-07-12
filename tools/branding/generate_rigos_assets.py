#!/usr/bin/env python3
"""Build the RIGOS wordmarks and favicons from deterministic pixel geometry.

No font files, vector stock, gradients, 3D effects, or image-synthesis model are
used. Pillow is the only dependency. Running the script twice produces the
same raster output.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import shutil
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw

INK = (10, 11, 14, 255)
BONE = (239, 235, 216, 255)
BLUE = (27, 118, 255, 255)
AMBER = (233, 161, 32, 255)
CLEAR = (0, 0, 0, 0)

GLYPHS = {
    "R": ["1111100", "1000010", "1000010", "1111100", "1010000", "1001000", "1000100", "1000010", "1000010"],
    "I": ["1111111", "0011100", "0011100", "0011100", "0011100", "0011100", "0011100", "0011100", "1111111"],
    "G": ["0111110", "1100011", "1000000", "1000000", "1001111", "1000011", "1000011", "1100011", "0111110"],
    "S": ["0111110", "1100011", "1000000", "1100000", "0111110", "0000011", "0000011", "1100011", "0111110"],
}

OPTIONS = {
    1: ("pcb-socket", "Circuit-letter wordmark with a mechanical socket core."),
    2: ("hex-forge", "Dense mining-rig silhouette with heavy enclosure hardware."),
    3: ("vga-chaos", "Hard pixel grid, exposed traces, crooked baseline."),
}


@dataclass(frozen=True)
class Theme:
    foreground: tuple[int, int, int, int]
    counter: tuple[int, int, int, int]
    keyline: tuple[int, int, int, int]


LIGHT = Theme(INK, BONE, BONE)
DARK = Theme(BONE, INK, INK)


def rough_rect(draw, box, fill, rng, amount=3):
    x0, y0, x1, y1 = box
    points = [
        (x0 + rng.randint(-amount, amount), y0 + rng.randint(-amount, amount)),
        (x1 + rng.randint(-amount, amount), y0 + rng.randint(-amount, amount)),
        (x1 + rng.randint(-amount, amount), y1 + rng.randint(-amount, amount)),
        (x0 + rng.randint(-amount, amount), y1 + rng.randint(-amount, amount)),
    ]
    draw.polygon(points, fill=fill)


def rough_line(draw, points, fill, width, rng, jitter=2):
    points = [(x + rng.randint(-jitter, jitter), y + rng.randint(-jitter, jitter)) for x, y in points]
    draw.line(points, fill=fill, width=width, joint="curve")


def glyph(draw, letter, origin, cell, theme, rng, *, skew=0, roughness=3):
    pattern = GLYPHS[letter]
    ox, oy = origin
    rows, cols = len(pattern), len(pattern[0])
    outline = max(3, cell // 10)
    for underlay in (True, False):
        for row, bits in enumerate(pattern):
            shift = int((rows - row - 1) * skew / max(rows - 1, 1))
            for col, bit in enumerate(bits):
                if bit != "1":
                    continue
                x0, y0 = ox + col * cell + shift, oy + row * cell
                box = (x0, y0, x0 + cell, y0 + cell)
                if underlay:
                    draw.rectangle((box[0] - outline, box[1] - outline, box[2] + outline, box[3] + outline), fill=theme.keyline)
                else:
                    rough_rect(draw, box, theme.foreground, rng, roughness)
    return ox, oy, ox + cols * cell + max(0, skew), oy + rows * cell


def pins(draw, box, theme, rng, *, pitch, length, sides="tb"):
    x0, y0, x1, y1 = box
    thickness = max(5, length // 3)
    if "t" in sides or "b" in sides:
        for x in range(x0 + pitch, x1 - pitch // 2, pitch):
            if "t" in sides:
                rough_rect(draw, (x, y0 - length, x + thickness, y0 + 2), theme.foreground, rng, 2)
            if "b" in sides:
                rough_rect(draw, (x, y1 - 2, x + thickness, y1 + length), theme.foreground, rng, 2)
    if "l" in sides or "r" in sides:
        for y in range(y0 + pitch, y1 - pitch // 2, pitch):
            if "l" in sides:
                rough_rect(draw, (x0 - length, y, x0 + 2, y + thickness), theme.foreground, rng, 2)
            if "r" in sides:
                rough_rect(draw, (x1 - 2, y, x1 + length, y + thickness), theme.foreground, rng, 2)


def traces(draw, anchor, theme, rng, *, mirror=False, scale=1.0):
    ax, ay = anchor
    direction = -1 if mirror else 1
    for idx, rise in enumerate((0, 24, 48)):
        y = ay + int(rise * scale)
        reach = int((78 + idx * 18) * scale)
        step = int((26 + idx * 5) * scale)
        points = [(ax, y), (ax + direction * step, y), (ax + direction * step, y - int(26 * scale)), (ax + direction * reach, y - int(26 * scale))]
        rough_line(draw, points, BLUE, max(5, int(10 * scale)), rng)
        px, py = points[-1]
        pad = max(8, int(18 * scale))
        rough_rect(draw, (px - pad // 2, py - pad // 2, px + pad // 2, py + pad // 2), BLUE, rng, 2)


def octagon(cx, cy, radius, cut):
    return [
        (cx - radius + cut, cy - radius), (cx + radius - cut, cy - radius),
        (cx + radius, cy - radius + cut), (cx + radius, cy + radius - cut),
        (cx + radius - cut, cy + radius), (cx - radius + cut, cy + radius),
        (cx - radius, cy + radius - cut), (cx - radius, cy - radius + cut),
    ]


def socket_o(draw, center, radius, theme, rng, *, brutal=False):
    cx, cy = center
    cut = radius // (3 if brutal else 4)
    draw.polygon(octagon(cx, cy, radius, cut), fill=theme.keyline)
    draw.polygon(octagon(cx, cy, radius - 8, max(6, cut - 4)), fill=theme.foreground)
    draw.polygon(octagon(cx, cy, int(radius * 0.68), int(cut * 0.68)), fill=CLEAR)
    ring, seg = int(radius * 0.52), int(radius * 0.22)
    rough_rect(draw, (cx - seg, cy - ring - 11, cx + seg, cy - ring + 15), BLUE, rng)
    rough_rect(draw, (cx - seg, cy + ring - 15, cx + seg, cy + ring + 11), BLUE, rng)
    rough_rect(draw, (cx - ring - 11, cy - seg, cx - ring + 15, cy + seg), BLUE, rng)
    rough_rect(draw, (cx + ring - 15, cy - seg, cx + ring + 11, cy + seg), BLUE, rng)
    pad_len, pad_thick = int(radius * 0.28), int(radius * 0.16)
    rough_rect(draw, (cx - pad_thick // 2, cy - radius + 14, cx + pad_thick // 2, cy - radius + pad_len), AMBER, rng)
    rough_rect(draw, (cx - pad_thick // 2, cy + radius - pad_len, cx + pad_thick // 2, cy + radius - 14), AMBER, rng)
    rough_rect(draw, (cx - radius + 14, cy - pad_thick // 2, cx - radius + pad_len, cy + pad_thick // 2), AMBER, rng)
    rough_rect(draw, (cx + radius - pad_len, cy - pad_thick // 2, cx + radius - 14, cy + pad_thick // 2), AMBER, rng)
    draw.polygon(octagon(cx, cy, int(radius * 0.32), max(8, int(cut * 0.25))), fill=theme.keyline)
    draw.polygon(octagon(cx, cy, int(radius * 0.25), max(6, int(cut * 0.18))), fill=theme.foreground)
    box = (cx - radius, cy - radius, cx + radius, cy + radius)
    pins(draw, box, theme, rng, pitch=max(32, radius // 3), length=max(24, radius // 6), sides="tblr")
    return box


def damage(image, seed, amount, max_size):
    rng = random.Random(seed)
    alpha = image.getchannel("A")
    box = alpha.getbbox()
    if not box:
        return
    draw = ImageDraw.Draw(image)
    x0, y0, x1, y1 = box
    for _ in range(amount):
        x, y = rng.randrange(x0, x1), rng.randrange(y0, y1)
        if alpha.getpixel((x, y)) < 200:
            continue
        w, h = rng.randint(2, max_size), rng.randint(1, max(2, max_size // 2))
        draw.rectangle((x, y, x + w, y + h), fill=CLEAR)


def crop(image, padding=28):
    box = image.getchannel("A").getbbox()
    if not box:
        return image
    x0, y0, x1, y1 = box
    return image.crop((max(0, x0 - padding), max(0, y0 - padding), min(image.width, x1 + padding), min(image.height, y1 + padding)))


def wordmark(option, theme):
    seed = 0x5249474F + option * 101 + (0 if theme is LIGHT else 10000)
    rng = random.Random(seed)
    image = Image.new("RGBA", (1900, 600), CLEAR)
    draw = ImageDraw.Draw(image)

    if option == 1:
        cell, y = 46, 90
        r = glyph(draw, "R", (60, y), cell, theme, rng, skew=8)
        pins(draw, r, theme, rng, pitch=42, length=25, sides="tb")
        traces(draw, (r[0] + 10, r[3] - 90), theme, rng, scale=.75)
        i = glyph(draw, "I", (420, y), cell, theme, rng, roughness=2)
        pins(draw, i, theme, rng, pitch=45, length=18, sides="lr")
        g = glyph(draw, "G", (620, y), cell, theme, rng, skew=-5)
        for n in range(3):
            rough_rect(draw, (g[2] - 115 + n * 34, g[3] - 34, g[2] - 92 + n * 34, g[3] - 11), BLUE, rng, 2)
        socket_o(draw, (1355, 295), 205, theme, rng)
        s = glyph(draw, "S", (1510, y), cell, theme, rng, skew=4)
        pins(draw, s, theme, rng, pitch=42, length=25, sides="b")
        traces(draw, (s[2] - 12, s[3] - 88), theme, rng, mirror=True, scale=.68)
    elif option == 2:
        cell, y = 48, 84
        r = glyph(draw, "R", (80, y), cell, theme, rng, skew=14, roughness=4)
        pins(draw, r, theme, rng, pitch=44, length=29, sides="tb")
        traces(draw, (r[0] + 8, r[3] - 90), theme, rng, scale=.78)
        rough_rect(draw, (r[0] + 5, r[1] + 12, r[0] + 22, r[1] + 95), AMBER, rng)
        i = glyph(draw, "I", (435, y), cell, theme, rng, roughness=3)
        for yy in range(i[1] + 100, i[3] - 80, 45):
            rough_rect(draw, (i[0] + 102, yy, i[0] + 151, yy + 9), theme.counter, rng, 2)
        g = glyph(draw, "G", (620, y), cell, theme, rng, skew=-10, roughness=4)
        pins(draw, g, theme, rng, pitch=48, length=22, sides="t")
        rough_line(draw, [(g[0] + 20, g[1] + 35), (g[0] + 92, g[1] + 35), (g[0] + 122, g[1] + 7)], theme.counter, 9, rng)
        socket_o(draw, (1340, 300), 212, theme, rng, brutal=True)
        s = glyph(draw, "S", (1510, y), cell, theme, rng, skew=8, roughness=4)
        pins(draw, s, theme, rng, pitch=43, length=28, sides="tb")
        traces(draw, (s[2] - 8, s[3] - 90), theme, rng, mirror=True, scale=.72)
        rough_rect(draw, (s[2] - 76, s[3] - 35, s[2] - 14, s[3] - 16), AMBER, rng)
    else:
        cell = 44
        r = glyph(draw, "R", (70, 106), cell, theme, rng, roughness=2)
        pins(draw, r, theme, rng, pitch=40, length=31, sides="t")
        traces(draw, (r[0] + 4, r[3] - 76), theme, rng, scale=.92)
        i = glyph(draw, "I", (412, 93), cell, theme, rng, roughness=2)
        pins(draw, i, theme, rng, pitch=39, length=17, sides="lr")
        for yy in range(i[1] + 100, i[3] - 80, 42):
            rough_rect(draw, (i[0] + 88, yy, i[0] + 145, yy + 8), theme.counter, rng, 1)
        g = glyph(draw, "G", (596, 114), cell, theme, rng, roughness=2)
        for n in range(3):
            rough_rect(draw, (g[2] - 104 + n * 31, g[3] - 29, g[2] - 84 + n * 31, g[3] - 9), BLUE, rng, 1)
        socket_o(draw, (1338, 302), 199, theme, rng, brutal=True)
        s = glyph(draw, "S", (1518, 101), cell, theme, rng, roughness=2)
        pins(draw, s, theme, rng, pitch=39, length=30, sides="b")
        traces(draw, (s[2] - 4, s[3] - 76), theme, rng, mirror=True, scale=.82)
        for x, yy, width in ((310, 472, 95), (324, 489, 128), (707, 92, 120), (1638, 460, 80)):
            rough_rect(draw, (x, yy, x + width, yy + 8), BLUE if yy > 450 else theme.counter, rng, 1)

    damage(image, seed + 991, 65 if option != 3 else 35, 7)
    return crop(image)


def icon(option, theme, size=512):
    seed = 0x5249474F + option * 1009 + (0 if theme is LIGHT else 17000)
    rng = random.Random(seed)
    image = Image.new("RGBA", (size, size), CLEAR)
    draw = ImageDraw.Draw(image)
    margin = 62 if option != 2 else 54
    cx = cy = size // 2
    radius = size // 2 - margin
    cut = 55 if option == 1 else (72 if option == 2 else 42)
    draw.polygon(octagon(cx, cy, radius, cut), fill=theme.keyline)
    draw.polygon(octagon(cx, cy, radius - 10, max(10, cut - 6)), fill=theme.foreground)
    draw.polygon(octagon(cx, cy, radius - 50, max(8, cut - 24)), fill=CLEAR)
    box = (cx - radius, cy - radius, cx + radius, cy + radius)
    pins(draw, box, theme, rng, pitch=54 if option != 3 else 46, length=34 if option != 2 else 42, sides="tblr")
    cell = 35 if option != 2 else 37
    r = glyph(draw, "R", (135 if option != 2 else 126, 118 if option == 3 else 112), cell, theme, rng, skew=6 if option == 1 else (13 if option == 2 else 0), roughness=2 if option == 3 else 3)
    traces(draw, (r[0] + 5, r[3] - 76), theme, rng, scale=.7 if option == 2 else .62)
    for px, py in ((cx, margin + 3), (cx, size - margin - 3), (margin + 3, cy), (size - margin - 3, cy)):
        w, h = ((18, 55) if px == cx else (55, 18))
        rough_rect(draw, (px - w // 2, py - h // 2, px + w // 2, py + h // 2), AMBER, rng, 2)
    for px, py in ((margin + 36, margin + 36), (size - margin - 36, margin + 36), (margin + 36, size - margin - 36), (size - margin - 36, size - margin - 36)):
        rough_rect(draw, (px - 13, py - 13, px + 13, py + 13), theme.keyline, rng, 2)
        rough_rect(draw, (px - 7, py - 7, px + 7, py + 7), BLUE, rng, 1)
    if option == 2:
        rough_line(draw, [(100, 92), (178, 92), (205, 65)], AMBER, 12, rng)
        rough_line(draw, [(412, 418), (350, 418), (324, 445)], AMBER, 12, rng)
    elif option == 3:
        rough_line(draw, [(390, 84), (430, 84), (430, 150)], BLUE, 10, rng)
        rough_line(draw, [(80, 388), (80, 433), (146, 433)], BLUE, 10, rng)
    damage(image, seed + 314, 34 if option != 3 else 20, 6)
    return image


def save(image, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, "PNG", optimize=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def patch_readme(path):
    start = "<!-- RIGOS_BRAND_START -->"
    end = "<!-- RIGOS_BRAND_END -->"
    block = '''<!-- RIGOS_BRAND_START -->
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/current/rigos-wordmark-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="assets/branding/current/rigos-wordmark.png">
    <img alt="RIGOS" src="assets/branding/current/rigos-wordmark.png" width="860">
  </picture>
</p>

<p align="center"><code>NO MAGIC. READ THE MACHINE.</code></p>
<!-- RIGOS_BRAND_END -->
'''
    old = path.read_text(encoding="utf-8") if path.exists() else ""
    if start in old and end in old:
        before, rest = old.split(start, 1)
        _, after = rest.split(end, 1)
        new = before.rstrip() + "\n\n" + block.rstrip() + after
    else:
        new = block.rstrip() + "\n\n" + old.lstrip()
    path.write_text(new.rstrip() + "\n", encoding="utf-8")


def write_branding_doc(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('''RIGOS BRAND ASSETS
==================

STATE
-----
Default mark: OPTION 03 // VGA-CHAOS
Format: transparent PNG / RGBA
Master: deterministic pixel geometry
Palette: INK + BONE + ELECTRIC BLUE + AMBER

The raster assets are built from hand-authored grid geometry and a fixed-seed
rasterizer. The generator has no diffusion or image-synthesis stage.

OPTIONS
-------
01 // PCB-SOCKET
    Readable circuit-letter system. The O is a socket / mining-chip core.

02 // HEX-FORGE
    Heavier mechanical mass. More amber hardware and enclosure structure.

03 // VGA-CHAOS
    Hard pixel grid, exposed traces, crooked baseline, least corporate.
    This is the selected README mark.

BUILD PIPELINE
--------------
```mermaid
flowchart LR
    A["HAND-AUTHORED GRID"] --> B["FIXED-SEED RASTERIZER"]
    B --> C["TRANSPARENT RGBA"]
    C --> D["LIGHT / DARK WORDMARK"]
    C --> E["16 / 32 / 64 / 128 / 256 ICONS"]
    D --> F["README"]
    E --> G["WEB / UI / FAVICON"]
```

RULES
-----
- No gradient.
- No 3D.
- No stock-font dependency.
- No smoothing on favicon output.
- Keep the transparent canvas.
- Do not redraw into generic corporate geometry.
''', encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    root = args.repo_root.resolve()
    out = root / "assets" / "branding"
    if out.exists():
        shutil.rmtree(out)
    manifest = {"generator": "deterministic Pillow geometry; no diffusion stage", "selected_option": 3, "options": {}}
    for number, (slug, description) in OPTIONS.items():
        target = out / f"option-{number}-{slug}"
        files = {
            "rigos-wordmark.png": wordmark(number, LIGHT),
            "rigos-wordmark-dark.png": wordmark(number, DARK),
            "rigos-icon.png": icon(number, LIGHT),
            "rigos-icon-dark.png": icon(number, DARK),
        }
        for name, image in files.items():
            save(image, target / name)
        base_icon = files["rigos-icon.png"]
        for size in (16, 32, 64, 128, 256):
            save(base_icon.resize((size, size), Image.Resampling.NEAREST), target / f"favicon-{size}.png")
        manifest["options"][str(number)] = {
            "name": slug.upper(),
            "description": description,
            "sha256": {name: digest(target / name) for name in files},
        }
    current = out / "current"
    current.mkdir(parents=True)
    selected = out / "option-3-vga-chaos"
    for source in selected.iterdir():
        if source.is_file():
            shutil.copy2(source, current / source.name)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    write_branding_doc(root / "docs" / "BRANDING.md")
    patch_readme(root / "README.md")


if __name__ == "__main__":
    main()
