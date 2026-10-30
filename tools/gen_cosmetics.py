#!/usr/bin/env python3
"""Generate Kestrel's expanded cosmetic collection (deterministic, original art).

Writes into the client mod's resources:
  textures/capes/<id>.png          static capes: 256x128 (4x HD cape sheet)
                                   animated capes: 128x64 frames stacked vertically
  textures/capes/thumbs/<id>.png   outer face of every cape (animated: frames side by side)
  textures/sky/<id>.png            3x2 sky atlases, 512 px faces (seam-free, see gen_assets.py)
  textures/sky/thumbs/<id>.png     384x128 north/east/south strip (also for the 3 older skies)
  capes.json, skies.json           catalogs (ids, names, categories, frame rates)

Every design here is drawn procedurally by this script; no third-party art.

Usage: python tools/gen_cosmetics.py [resources_dir]   (needs NumPy + Pillow)
"""
import json
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(__file__))
from gen_assets import FACES, fbm, mix, stars, value_noise  # noqa: E402

RES = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    os.path.dirname(__file__), "..", "client", "src", "main", "resources", "assets", "kestrel")
TEX = os.path.join(RES, "textures")

TAU = 2 * np.pi


# ============================================================== helpers
def c(hexstr):
    h = hexstr.lstrip("#")
    return np.array([int(h[i:i + 2], 16) / 255.0 for i in (0, 2, 4)])


def hsv(h, s, v):
    """Vectorised HSV -> RGB, h in [0,1)."""
    h = np.mod(h, 1.0) * 6
    i = np.floor(h).astype(int) % 6
    f = h - np.floor(h)
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    r = np.choose(i, [v, q, p, p, t, v])
    g = np.choose(i, [t, v, v, q, p, p])
    b = np.choose(i, [p, p, t, v, v, q])
    return np.stack([r, g, b], axis=-1)


def grid(h, w):
    """Normalised coordinates of the cape's outer face: u across (0..1), v down (0..1)."""
    v, u = np.mgrid[0:h, 0:w]
    return (u + 0.5) / w, (v + 0.5) / h


def glow(dist, width):
    return np.exp(-(dist / width) ** 2)


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def mask_image(h, w, draw_fn, ss=4):
    """Anti-aliased mask (0..1) from PIL drawing at ss x supersampling."""
    img = Image.new("L", (w * ss, h * ss), 0)
    draw_fn(ImageDraw.Draw(img), w * ss, h * ss)
    img = img.resize((w, h), Image.LANCZOS)
    return np.asarray(img, dtype=float) / 255.0


def kestrel_emblem(d, W, H, cx=0.5, cy=0.42, size=0.62):
    """Kestrel mark: a stylised falcon wing chevron above a 'K'."""
    s = size * W
    ox, oy = cx * W, cy * H
    # Wing chevron.
    wing = [(ox - 0.5 * s, oy - 0.38 * s), (ox, oy - 0.10 * s), (ox + 0.5 * s, oy - 0.38 * s),
            (ox + 0.5 * s, oy - 0.24 * s), (ox, oy + 0.04 * s), (ox - 0.5 * s, oy - 0.24 * s)]
    d.polygon(wing, fill=255)
    # 'K'.
    t = 0.09 * s
    top, bot = oy + 0.12 * s, oy + 0.62 * s
    left = ox - 0.22 * s
    d.rectangle([left, top, left + t, bot], fill=255)
    mid = (top + bot) / 2
    d.polygon([(left + t, mid - t * 0.6), (ox + 0.24 * s, top), (ox + 0.24 * s + t * 1.3, top),
               (left + t * 1.4, mid)], fill=255)
    d.polygon([(left + t * 1.4, mid - t * 0.2), (ox + 0.24 * s + t * 1.3, bot), (ox + 0.24 * s, bot),
               (left + t, mid + t * 0.6)], fill=255)


def rgba(rgb, a=1.0):
    rgb = np.clip(rgb, 0, 1)
    alpha = np.broadcast_to(np.asarray(a, dtype=float), rgb.shape[:2])[..., None]
    return np.concatenate([rgb, alpha], axis=-1)


# ============================================================== cape sheet
def cape_sheet(front, scale):
    """Paint a full cape sheet (64x32 units * scale) around an outer-face design.

    front: (16*scale, 10*scale, 4) RGBA. Layout (in 64x32 units): outer face at
    (1,1) 10x16, inner face at (12,1) 10x16, top (1,0) 10x1, bottom (11,0) 10x1,
    sides (0,1) and (11,1) 1x16.
    """
    s = scale
    sheet = np.zeros((32 * s, 64 * s, 4))
    sheet[s:17 * s, s:11 * s] = front
    inner = front[:, ::-1].copy()
    inner[..., :3] = inner[..., :3] * 0.62 + front[..., :3].mean(axis=(0, 1)) * 0.08
    sheet[s:17 * s, 12 * s:22 * s] = inner
    edge = front[:s].mean(axis=0, keepdims=True)
    sheet[0:s, s:11 * s] = np.repeat(front[:s][::-1] * [0.85, 0.85, 0.85, 1], 1, axis=0)
    sheet[0:s, 11 * s:21 * s] = np.repeat(front[-s:] * [0.7, 0.7, 0.7, 1], 1, axis=0)
    sheet[s:17 * s, 0:s] = front[:, :s][:, ::-1] * [0.8, 0.8, 0.8, 1]
    sheet[s:17 * s, 11 * s:12 * s] = front[:, -s:][:, ::-1] * [0.8, 0.8, 0.8, 1]
    del edge
    sheet[..., 3] = np.where(sheet[..., 3] > 0, 1.0, 0.0)
    return sheet


def to_img(a, mode="RGBA"):
    return Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), mode)


def bordered(rgb, u, v, width=0.045, amount=0.35):
    edge = (u < width) | (u > 1 - width) | (v > 1 - width * 0.62)
    return rgb * (1 - amount * edge[..., None])


# ---------------------------------------------------------------- static designs
# Each takes (u, v, H, W) of the outer face and returns RGB.
def d_kestrel_classic(u, v, H, W):
    base = mix(c("2a2d33"), c("15171b"), v)
    m = mask_image(H, W, lambda d, w, h: kestrel_emblem(d, w, h))
    rgb = mix(base, c("e8833a"), m)
    stripe = (np.abs(v - 0.9) < 0.025)
    rgb = mix(rgb, c("e8833a"), stripe * 0.9)
    return bordered(rgb, u, v)


def d_kestrel_gold(u, v, H, W):
    sheen = 0.5 + 0.5 * np.sin((u * 2.2 - v * 1.4) * TAU * 0.5)
    base = mix(c("6b4a12"), c("e9c15a"), 0.35 + 0.5 * sheen * (1 - v * 0.6))
    m = mask_image(H, W, lambda d, w, h: kestrel_emblem(d, w, h))
    rgb = mix(base, c("2b1d07"), m * 0.9)
    return bordered(rgb, u, v, amount=0.45)


def d_kestrel_stealth(u, v, H, W):
    base = mix(c("1a1a1c"), c("0b0b0c"), v)
    m = mask_image(H, W, lambda d, w, h: kestrel_emblem(d, w, h))
    rgb = mix(base, c("2f3136"), m)
    return rgb


def d_onyx(u, v, H, W):
    rgb = mix(c("141416"), c("08080a"), v)
    line = (np.abs(u - 0.5) > 0.42) & (np.abs(u - 0.5) < 0.45) | (np.abs(v - 0.06) < 0.012)
    return mix(rgb, c("3a3d44"), line * 1.0)


def d_ivory(u, v, H, W):
    rgb = mix(c("f4f1e8"), c("d9d3c3"), v)
    trim = (u < 0.06) | (u > 0.94) | (v > 0.95)
    rgb = mix(rgb, c("c9a24a"), trim * 1.0)
    diamond = np.abs(u - 0.5) * 0.625 + np.abs(v - 0.3) < 0.085
    inner = np.abs(u - 0.5) * 0.625 + np.abs(v - 0.3) < 0.05
    return mix(rgb, c("c9a24a"), (diamond & ~inner) * 1.0)


def d_sunset(u, v, H, W):
    rgb = mix(mix(c("ffb347"), c("ff5e7e"), v / 0.55), c("5b2a86"), (v - 0.55) / 0.45)
    sun = np.hypot((u - 0.5) * 0.625, v - 0.6)
    rgb = mix(rgb, c("ffe7a1"), smooth(0.2, 0.18, sun))
    bands = (sun < 0.2) & (np.mod(v * 40, 2) < 0.8) & (v > 0.6)
    return mix(rgb, c("ff5e7e"), bands * 0.9)


def d_ocean(u, v, H, W):
    rgb = mix(c("3fd2c7"), c("0b2e59"), v)
    wave = np.sin(u * TAU * 2 + v * 14) * 0.5 + 0.5
    return rgb + (wave ** 12)[..., None] * 0.25


def d_mint(u, v, H, W):
    rgb = mix(c("b8f5d0"), c("2f9e6e"), v)
    dots = (np.mod(u * 10, 1) - 0.5) ** 2 + (np.mod(v * 16, 1) - 0.5) ** 2 < 0.04
    return mix(rgb, c("ffffff"), dots * 0.35)


def d_neon_grid(u, v, H, W):
    rgb = mix(c("12062b"), c("05010f"), v)
    gx = np.abs(np.mod(u * 6, 1) - 0.5)
    gy = np.abs(np.mod(v * 9, 1) - 0.5)
    lines = glow(0.5 - np.maximum(gx, gy), 0.05)
    rgb = rgb + lines[..., None] * c("21e6ff") * 0.9
    return rgb + glow(np.abs(v - 0.5), 0.04)[..., None] * c("ff2bd6") * 0.6


def d_neon_circuit(u, v, H, W):
    rgb = c("0d0d14") * np.ones(u.shape)[..., None]
    rng = np.random.RandomState(7)
    lines = np.zeros(u.shape)
    for _ in range(9):
        x = rng.uniform(0.1, 0.9)
        y0, y1 = sorted(rng.uniform(0.05, 0.95, 2))
        lines = np.maximum(lines, glow(np.abs(u - x), 0.012) * ((v > y0) & (v < y1)))
        lines = np.maximum(lines, glow(np.abs(v - y1), 0.008) * (np.abs(u - x) < 0.18))
        lines = np.maximum(lines, glow(np.hypot((u - x) * 1.6, v - y0), 0.02))
    return rgb + lines[..., None] * c("ff3ea5") * 1.3


def d_rgb_bands(u, v, H, W):
    return hsv(u * 0.6 + v * 0.8, 0.85, 0.95)


def d_hexagon(u, v, H, W):
    x, y = u * 10 / 1.2, v * 16 / 1.2
    q = (np.sqrt(3) / 3 * x - y / 3)
    r = 2 / 3 * y
    # Distance to hex centres (axial rounding).
    rx, ry = np.round(q), np.round(r)
    dx, dy = q - rx, r - ry
    d = np.maximum(np.abs(dx), np.maximum(np.abs(dy), np.abs(dx + dy)))
    base = mix(c("1f2a44"), c("0e1424"), v)
    edge = smooth(0.38, 0.48, d)
    shade = 0.15 * np.sin((rx * 1.7 + ry * 2.3))
    return mix(base + shade[..., None] * 0.3, c("4fb3ff"), edge)


def d_triangles(u, v, H, W):
    x, y = u * 5, v * 8
    cell = np.floor(x) + np.floor(y) * 7 + (np.mod(x, 1) > np.mod(y, 1)) * 31
    val = np.mod(np.sin(cell * 12.9898) * 43758.5453, 1)
    return mix(c("ff7a59"), c("7a1f5c"), val * 0.8 + v * 0.3)


def d_checker(u, v, H, W):
    chk = (np.floor(u * 5) + np.floor(v * 8)) % 2
    return mix(c("26282c"), c("d6d8dc"), chk * 0.85)


def d_inferno(u, v, H, W):
    n = value_noise(np.stack([u * 5, v * 7, np.zeros_like(u) + 3.0], -1), 5)
    flame = (1 - v) * 1.3 - np.abs(u - 0.5) * 0.9 + n * 0.6 - 0.35
    rgb = mix(c("2a0500"), c("b22100"), smooth(-0.1, 0.25, flame))
    rgb = mix(rgb, c("ff8a00"), smooth(0.2, 0.45, flame))
    return mix(rgb, c("ffe066"), smooth(0.45, 0.7, flame))


def d_frost(u, v, H, W):
    rgb = mix(c("e8f7ff"), c("5d97c9"), v)

    def flakes(d, w, h):
        lw = max(1, w // 26)
        for cx, cy, r in ((0.5, 0.3, 0.3), (0.26, 0.7, 0.17), (0.75, 0.62, 0.15), (0.7, 0.88, 0.09)):
            x0, y0, R = cx * w, cy * h, r * w
            for k in range(6):
                a = k * np.pi / 3
                ex, ey = x0 + np.cos(a) * R, y0 + np.sin(a) * R
                d.line([(x0, y0), (ex, ey)], fill=255, width=lw)
                for f in (0.5, 0.75):
                    bx, by = x0 + np.cos(a) * R * f, y0 + np.sin(a) * R * f
                    for s in (-1, 1):
                        b = a + s * np.pi / 4
                        d.line([(bx, by), (bx + np.cos(b) * R * 0.25, by + np.sin(b) * R * 0.25)], fill=255, width=lw)
    m = mask_image(H, W, flakes)
    return mix(rgb, c("ffffff"), m)


def d_thunder(u, v, H, W):
    rgb = mix(c("3c3f58"), c("12131d"), v)
    clouds = value_noise(np.stack([u * 4, v * 3, np.zeros_like(u)], -1), 9)
    rgb = mix(rgb, c("5c6079"), smooth(0.5, 0.8, clouds) * (v < 0.4))
    m = mask_image(H, W, lambda d, w, h: d.polygon(
        [(w * .55, h * .18), (w * .32, h * .55), (w * .5, h * .55), (w * .38, h * .9), (w * .72, h * .45),
         (w * .54, h * .45), (w * .66, h * .18)], fill=255))
    return mix(rgb, c("fff3a0"), m) + glow(np.hypot(u - 0.5, v - 0.55), 0.25)[..., None] * c("6c6aff") * 0.25


def d_grove(u, v, H, W):
    rgb = mix(c("3b7a3a"), c("173b20"), v)
    n = value_noise(np.stack([u * 8, v * 12, np.ones_like(u)], -1), 17)
    leaves = smooth(0.55, 0.7, n)
    rgb = mix(rgb, c("8fd16a"), leaves * 0.6)
    vine = glow(np.abs(u - 0.5 - 0.12 * np.sin(v * TAU * 1.5)), 0.03)
    return mix(rgb, c("c6f08a"), vine)


def d_pixel_sword(u, v, H, W):
    rgb = mix(c("2b3a67"), c("16203d"), v)
    px, py = np.floor(u * 10).astype(int), np.floor(v * 16).astype(int)
    art = ["..........", "........##", ".......#w#", "......#w#.", ".....#w#..", "....#w#...",
           "...#w#....", "#.#w#.....", ".#b#......", "..#b#.....", ".#.#b#....", "#....#....",
           "..........", "..........", "..........", ".........."]
    out = rgb.copy()
    for yy, row in enumerate(art):
        for xx, ch in enumerate(row):
            m = (px == xx) & (py == yy + 2) if yy + 2 < 16 else np.zeros_like(px, bool)
            col = {"#": c("1b1b1f"), "w": c("d9e6f2"), "b": c("8a5a2b")}.get(ch)
            if col is not None:
                out[m] = col
    return out


def d_pixel_moon(u, v, H, W):
    px, py = np.floor(u * 10), np.floor(v * 16)
    rgb = mix(c("1b1438"), c("070514"), v)
    moon = (np.hypot(px - 5.5, py - 5) < 3.2) & ~(np.hypot(px - 7, py - 4) < 2.8)
    rgb = mix(rgb, c("ffe9a8"), moon * 1.0)
    st = np.mod(np.sin(px * 12.9898 + py * 78.233) * 43758.5453, 1) > 0.93
    return mix(rgb, c("ffffff"), st * 0.9)


def d_galaxy(u, v, H, W):
    p = np.stack([u * 3, v * 4.8, np.zeros_like(u) + 1.7], -1)
    n = fbm(p, 23, 4)
    rgb = c("06041a") + smooth(0.45, 0.8, n)[..., None] * c("7a3fd1") + smooth(0.55, 0.85, fbm(p * 1.7, 41, 3))[..., None] * c("1fb5c9")
    st = np.mod(np.sin(np.floor(u * W) * 12.9898 + np.floor(v * H) * 78.233) * 43758.5453, 1) > 0.985
    return rgb + st[..., None] * 0.9


def d_obsidian_rune(u, v, H, W):
    rgb = mix(c("1a0f2e"), c("0a0614"), v)
    n = value_noise(np.stack([u * 6, v * 9, np.zeros_like(u)], -1), 3)
    rgb = rgb + smooth(0.6, 0.8, n)[..., None] * c("2a1450")

    def runes(d, w, h):
        lw = max(1, w // 40)
        for i, (x, y) in enumerate(((0.5, 0.2), (0.5, 0.45), (0.5, 0.7))):
            cx, cy = x * w, y * h
            s = w * 0.16
            d.line([(cx - s, cy - s), (cx, cy + s), (cx + s, cy - s)], fill=255, width=lw * 3)
            d.line([(cx - s * 0.6, cy), (cx + s * 0.6, cy)], fill=255, width=lw * 3)
            if i == 1:
                d.ellipse([cx - s * 1.5, cy - s * 1.5, cx + s * 1.5, cy + s * 1.5], outline=255, width=lw * 2)
    m = mask_image(H, W, runes)
    return rgb + m[..., None] * c("b46bff") * 1.2


def d_royal(u, v, H, W):
    rgb = mix(c("7a0d1f"), c("3d0510"), v)
    damask = np.sin(u * TAU * 3) * np.sin(v * TAU * 4)
    rgb = rgb * (1 + 0.12 * damask)[..., None]
    trim = (u < 0.05) | (u > 0.95) | (v > 0.96) | (v < 0.03)
    rgb = mix(rgb, c("d9a937"), trim * 1.0)
    m = mask_image(H, W, lambda d, w, h: d.polygon(
        [(w * .5, h * .18), (w * .7, h * .28), (w * .7, h * .48), (w * .5, h * .6), (w * .3, h * .48),
         (w * .3, h * .28)], fill=255))
    return mix(rgb, c("d9a937"), m)


def d_carbon(u, v, H, W):
    x, y = u * 10 * 2, v * 16 * 2
    weave = ((np.floor(x) + np.floor(y)) % 2)
    shade = np.where(weave == 1, np.mod(x, 1), np.mod(y, 1))
    rgb = mix(c("1c1d20"), c("3a3c41"), shade * 0.8)
    return rgb * (1 + 0.15 * np.sin((u - v) * TAU))[..., None]


STATIC = [
    # id, name, category, design
    ("kestrel_classic", "Kestrel Classic", "kestrel", d_kestrel_classic),
    ("kestrel_gold", "Kestrel Gold", "kestrel", d_kestrel_gold),
    ("kestrel_stealth", "Kestrel Stealth", "kestrel", d_kestrel_stealth),
    ("onyx", "Onyx", "static", d_onyx),
    ("ivory", "Ivory", "static", d_ivory),
    ("sunset_fade", "Sunset Fade", "static", d_sunset),
    ("ocean_fade", "Ocean Fade", "static", d_ocean),
    ("mint", "Mint", "static", d_mint),
    ("neon_grid", "Neon Grid", "static", d_neon_grid),
    ("neon_circuit", "Neon Circuit", "static", d_neon_circuit),
    ("rgb_bands", "RGB Bands", "static", d_rgb_bands),
    ("hexagon", "Hexagon", "static", d_hexagon),
    ("low_poly", "Low Poly", "static", d_triangles),
    ("checker", "Monochrome Checker", "static", d_checker),
    ("inferno", "Inferno", "static", d_inferno),
    ("frost", "Frost", "static", d_frost),
    ("thunder", "Thunder", "static", d_thunder),
    ("grove", "Grove", "static", d_grove),
    ("pixel_blade", "Pixel Blade", "static", d_pixel_sword),
    ("pixel_moon", "Pixel Moon", "static", d_pixel_moon),
    ("galaxy", "Galaxy", "special", d_galaxy),
    ("obsidian_rune", "Obsidian Rune", "special", d_obsidian_rune),
    ("royal_crest", "Royal Crest", "special", d_royal),
    ("carbon", "Carbon Fiber", "static", d_carbon),
]


# ---------------------------------------------------------------- animated designs
# Each takes (u, v, t, H, W) with t in [0, 1) (one loop) and returns RGB.
def a_rgb_cycle(u, v, t, H, W):
    return hsv(t + v * 0.15, 0.8, 0.95 - 0.25 * v)


def a_rgb_wave(u, v, t, H, W):
    return hsv(u * 0.5 + v * 0.7 - t, 0.9, 1.0)


def a_plasma(u, v, t, H, W):
    p = TAU * t
    val = (np.sin(u * 9 + p) + np.sin(v * 11 - p) + np.sin((u + v) * 7 + p) + np.sin(np.hypot(u - 0.5, v - 0.5) * 18 - p)) / 4
    return hsv(0.72 + 0.18 * val, 0.75, 0.55 + 0.45 * (val * 0.5 + 0.5))


def a_waves(u, v, t, H, W):
    rgb = mix(c("63e3ff"), c("062a5c"), v)
    crest = np.zeros(u.shape)
    for k in range(4):
        y = 0.15 + k * 0.22 + 0.04 * np.sin(u * TAU * 1.5 + TAU * t + k)
        crest = np.maximum(crest, glow(v - y, 0.025) * (v > y - 0.06))
    return mix(rgb, c("e8fbff"), crest)


def a_flame(u, v, t, H, W):
    p = np.stack([u * 4, v * 5 + t * 5, np.full_like(u, 0.0)], -1)
    # Loop: blend two noise fields so frame 0 == frame N.
    n = value_noise(p, 7) * (1 - t) + value_noise(p - np.array([0, 5, 0]), 7) * t
    flame = (1 - v) * 1.4 - np.abs(u - 0.5) * 1.1 + n * 0.8 - 0.5
    rgb = mix(c("1c0200"), c("c42200"), smooth(-0.1, 0.25, flame))
    rgb = mix(rgb, c("ff9a1f"), smooth(0.2, 0.45, flame))
    return mix(rgb, c("fff1a6"), smooth(0.45, 0.75, flame))


def a_energy(u, v, t, H, W):
    rgb = mix(c("0a0f2e"), c("02030c"), v)
    out = rgb.copy()
    for k in range(3):
        ph = TAU * (t + k / 3)
        x = 0.5 + 0.28 * np.sin(v * 9 + ph) * np.sin(v * 3.1 + k)
        out = out + glow(u - x, 0.03)[..., None] * c("44d9ff") * (0.6 + 0.4 * np.sin(ph + v * 6))[..., None]
    core = glow(np.hypot((u - 0.5) * 1.6, v - 0.5), 0.18 + 0.04 * np.sin(TAU * t))
    return out + core[..., None] * c("7fe9ff") * 0.6


def a_matrix(u, v, t, H, W):
    cols = 10
    col = np.floor(u * cols)
    speed = 1 + np.mod(col * 7, 3)
    head = np.mod(t * speed + col * 0.37, 1.0)
    dist = np.mod(head - v, 1.0)
    trail = np.exp(-dist * 6) * (np.abs(np.mod(u * cols, 1) - 0.5) < 0.3)
    flick = 0.7 + 0.3 * np.mod(np.sin(np.floor(v * 16) * 91.3 + col * 13.1) * 437.5, 1)
    rgb = c("020803") + (trail * flick)[..., None] * c("39ff6a")
    return rgb + glow(dist, 0.015)[..., None] * c("d6ffe0") * (np.abs(np.mod(u * cols, 1) - 0.5) < 0.3)[..., None]


def a_starfall(u, v, t, H, W):
    rgb = mix(c("140a33"), c("03020b"), v)
    out = rgb.copy()
    rng = np.random.RandomState(4)
    for _ in range(14):
        x0, ph, sp = rng.uniform(0, 1), rng.uniform(0, 1), rng.choice([1, 2])
        y = np.mod(ph + t * sp, 1.0) * 1.3 - 0.15
        x = x0 + (y - ph) * 0.12
        d = np.hypot((u - x) * 0.62, v - y)
        tail = np.exp(-np.maximum(0, (y - v)) * 14) * (np.abs((u - x) * 0.62) < 0.02) * (v < y)
        out = out + glow(d, 0.02)[..., None] * c("fff2c4") + tail[..., None] * c("ffcf6b") * 0.5
    twinkle = np.mod(np.sin(np.floor(u * W) * 12.9898 + np.floor(v * H) * 78.233) * 43758.5453, 1)
    out = out + ((twinkle > 0.97) * (0.5 + 0.5 * np.sin(TAU * (t * 2 + twinkle * 5))))[..., None] * 0.6
    return out


def a_pulse_rings(u, v, t, H, W):
    r = np.hypot((u - 0.5) * 0.625 * 1.6, v - 0.45)
    rings = np.zeros(u.shape)
    for k in range(3):
        rr = np.mod(t + k / 3, 1.0) * 0.8
        rings = np.maximum(rings, glow(r - rr, 0.025) * (1 - rr / 0.8))
    rgb = mix(c("101828"), c("05070d"), v)
    return rgb + rings[..., None] * c("ff5ad1") + glow(r, 0.06)[..., None] * c("ffd1f3") * 0.5


def a_kaleido(u, v, t, H, W):
    x, y = (u - 0.5) * 1.0, (v - 0.5) * 1.6
    a = np.arctan2(y, x) + TAU * t / 4
    r = np.hypot(x, y)
    a = np.abs(np.mod(a, TAU / 6) - TAU / 12)
    px, py = r * np.cos(a), r * np.sin(a)
    sq = np.maximum(np.abs(np.mod(px * 6 - t * 6, 1) - 0.5), np.abs(np.mod(py * 6, 1) - 0.5))
    return hsv(0.55 + 0.25 * r + t, 0.7, 0.25 + 0.75 * smooth(0.3, 0.45, sq))


def a_kestrel_pulse(u, v, t, H, W):
    base = mix(c("23262c"), c("101115"), v)
    m = mask_image(H, W, lambda d, w, h: kestrel_emblem(d, w, h))
    halo = mask_image(H, W, lambda d, w, h: kestrel_emblem(d, w, h, size=0.78))
    pulse = 0.5 + 0.5 * np.sin(TAU * t)
    rgb = base + (halo - m).clip(0, 1)[..., None] * c("e8833a") * (0.25 + 0.5 * pulse)
    sweep = glow(v - np.mod(t, 1.0) * 1.2 + 0.1, 0.05)
    rgb = mix(rgb, c("e8833a") * (0.75 + 0.25 * pulse), m)
    return rgb + (sweep * m)[..., None] * c("fff0dc") * 0.8


def a_aurora_flow(u, v, t, H, W):
    rgb = mix(c("06101e"), c("010308"), v)
    out = rgb.copy()
    for k, col in enumerate((c("3dffb4"), c("7c5cff"), c("45d4ff"))):
        x = 0.5 + 0.3 * np.sin(v * 4 + TAU * t + k * 2.1)
        curtain = glow(u - x, 0.08) * smooth(0.0, 0.3, v) * (1 - v * 0.6)
        out = out + curtain[..., None] * col * 0.7
    return out


def a_lava_lamp(u, v, t, H, W):
    field = np.zeros(u.shape)
    for k in range(5):
        cx = 0.5 + 0.28 * np.sin(TAU * t + k * 1.3)
        cy = np.mod(k / 5 + t, 1.0)
        field = field + 0.012 / (((u - cx) * 0.62) ** 2 + (v - cy) ** 2 + 0.002)
        field = field + 0.012 / (((u - cx) * 0.62) ** 2 + (v - cy + 1) ** 2 + 0.002)
    blob = smooth(0.9, 1.2, field)
    rgb = mix(c("2a0b3a"), c("120419"), v)
    return mix(rgb, mix(c("ff6b3d"), c("ffcf3d"), v), blob)


ANIMATED = [
    # id, name, category, fps, design
    ("rgb_cycle", "RGB Cycle", "animated", 12, a_rgb_cycle),
    ("rgb_wave", "RGB Wave", "animated", 15, a_rgb_wave),
    ("plasma", "Plasma", "animated", 12, a_plasma),
    ("tidal", "Tidal Waves", "animated", 12, a_waves),
    ("wildfire", "Wildfire", "animated", 15, a_flame),
    ("energy_core", "Energy Core", "animated", 15, a_energy),
    ("code_rain", "Code Rain", "animated", 12, a_matrix),
    ("starfall", "Starfall", "animated", 12, a_starfall),
    ("pulse_rings", "Pulse Rings", "animated", 15, a_pulse_rings),
    ("kaleidoscope", "Kaleidoscope", "animated", 12, a_kaleido),
    ("aurora_flow", "Aurora Flow", "animated", 12, a_aurora_flow),
    ("lava_lamp", "Lava Lamp", "special", 12, a_lava_lamp),
    ("kestrel_pulse", "Kestrel Pulse", "kestrel", 15, a_kestrel_pulse),
]
ANIM_FRAMES = 24
STATIC_SCALE = 4
ANIM_SCALE = 2


def gen_capes():
    out = os.path.join(TEX, "capes")
    thumbs = os.path.join(out, "thumbs")
    os.makedirs(thumbs, exist_ok=True)
    entries = []
    s = STATIC_SCALE
    for cid, name, cat, fn in STATIC:
        H, W = 16 * s, 10 * s
        u, v = grid(H, W)
        front = rgba(fn(u, v, H, W))
        to_img(cape_sheet(front, s)).save(os.path.join(out, cid + ".png"), optimize=True)
        to_img(front).save(os.path.join(thumbs, cid + ".png"), optimize=True)
        entries.append({"id": cid, "name": name, "texture": "capes/%s.png" % cid, "category": cat})
    s = ANIM_SCALE
    for cid, name, cat, fps, fn in ANIMATED:
        H, W = 16 * s, 10 * s
        u, v = grid(H, W)
        sheets, fronts = [], []
        for f in range(ANIM_FRAMES):
            front = rgba(fn(u, v, f / ANIM_FRAMES, H, W))
            fronts.append(front)
            sheets.append(cape_sheet(front, s))
        to_img(np.concatenate(sheets, axis=0)).save(os.path.join(out, cid + ".png"), optimize=True)
        to_img(np.concatenate(fronts, axis=1)).save(os.path.join(thumbs, cid + ".png"), optimize=True)
        entries.append({"id": cid, "name": name, "texture": "capes/%s.png" % cid, "category": cat, "fps": fps})
        print("cape", cid)
    return entries


# ============================================================== skies
def sky_gradient(y, stops):
    """stops: list of (y, rgb) sorted by y ascending (y in -1..1)."""
    ys = np.array([s[0] for s in stops])
    cols = np.array([s[1] for s in stops])
    out = np.zeros(y.shape + (3,))
    for ch in range(3):
        out[..., ch] = np.interp(y, ys, cols[:, ch])
    return out


def sun_glow(d, direction, tight, wide, col, col_wide):
    sd = np.array(direction, dtype=float)
    sd /= np.linalg.norm(sd)
    s = np.clip(d @ sd, 0, 1)
    return (s ** tight)[..., None] * col + (s ** wide)[..., None] * col_wide


def disc(d, direction, radius_deg, soft=0.15):
    sd = np.array(direction, dtype=float)
    sd /= np.linalg.norm(sd)
    ang = np.degrees(np.arccos(np.clip(d @ sd, -1, 1)))
    return smooth(radius_deg * (1 + soft), radius_deg * (1 - soft), ang)


def clouds(d, seed, scale, cover, y_lo=0.0, y_hi=0.6):
    y = d[..., 1]
    # Project onto a flat cloud plane so clouds look layered, not spherical.
    k = 1.0 / np.maximum(y + 0.12, 0.05)
    p = np.stack([d[..., 0] * k * scale, d[..., 2] * k * scale, np.full_like(y, seed * 0.37)], -1)
    n = fbm(p, seed, 5)
    m = smooth(1 - cover, 1 - cover + 0.25, n)
    fade = smooth(y_lo, y_lo + 0.08, y) * (1 - smooth(y_hi, y_hi + 0.3, y))
    return m * fade, n


def starfield(d, seed, amount=1.0):
    return (stars(d, seed, 0.03, 140) + stars(d, seed + 1, 0.01, 60) * 1.3) * amount


def ground(y, col, below=-0.02):
    return (y < below)[..., None], col


def s_golden_hour(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("2a1d16")), (-0.05, c("6b4630")), (0, c("ffb54a")), (0.12, c("ffcf7a")),
                           (0.35, c("8fb7e0")), (1, c("3e6fb5"))])
    col = col + sun_glow(d, (0.2, 0.06, -1), 600, 8, c("fff1c2") * 1.4, c("ff9a3c") * 0.45)
    cl, _ = clouds(d, 3, 1.1, 0.42)
    return mix(col, c("ffd29a") * 0.9 + sun_glow(d, (0.2, 0.06, -1), 3, 3, c("ff7a3c") * 0.3, 0), cl * 0.75)


def s_pink_sunrise(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("2c2238")), (-0.05, c("5e4766")), (0, c("ffb3c6")), (0.15, c("ffd6e0")),
                           (0.45, c("b9a7e6")), (1, c("5a64b8"))])
    col = col + sun_glow(d, (1, 0.03, 0.2), 500, 10, c("fffaf0") * 1.3, c("ff8fb1") * 0.35)
    cl, _ = clouds(d, 8, 1.4, 0.35)
    return mix(col, c("ffe3ec"), cl * 0.6)


def s_crimson_sunset(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("120507")), (-0.04, c("3b0c0f")), (0, c("ff4b2b")), (0.1, c("ff8a3d")),
                           (0.3, c("7a1f3d")), (1, c("1b0c26"))])
    col = col + sun_glow(d, (-1, 0.04, 0.3), 400, 6, c("ffe2b0") * 1.3, c("ff3b1f") * 0.5)
    cl, n = clouds(d, 12, 0.9, 0.5)
    return mix(col, mix(c("2a0a12"), c("ff6a3d"), n), cl * 0.8)


def s_desert_dusk(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("3a2414")), (-0.03, c("8a5a32")), (0, c("f4a259")), (0.2, c("c67b9d")),
                           (0.55, c("4b3f8f")), (1, c("161635"))])
    col = col + sun_glow(d, (0.6, 0.02, 0.8), 800, 12, c("fff4d6") * 1.2, c("f48f42") * 0.35)
    return col + (starfield(d, 30, 0.6) * smooth(0.45, 0.8, y))[..., None]


def s_galaxy_core(d):
    y = d[..., 1]
    axis = np.array([0.2, 0.75, 0.62]); axis /= np.linalg.norm(axis)
    band = np.exp(-((d @ axis) / 0.2) ** 2)
    core = np.clip(d @ np.array([0.0, 0.25, -0.97]), 0, 1) ** 40
    dust = fbm(d * 7, 51, 5)
    col = c("030308") + band[..., None] * (c("2a2250") + np.clip(dust - 0.35, 0, 1)[..., None] * c("e6d1ff") * 0.9)
    col = col - (band * smooth(0.55, 0.75, fbm(d * 11, 53, 4)))[..., None] * 0.35
    col = col + core[..., None] * c("ffd9a0") * 0.7
    st = starfield(d, 60, 1.0) + band * stars(d, 62, 0.15, 240) * 0.8
    return np.clip(col, 0, None) + st[..., None]


def s_deep_space(d):
    col = c("02030a") + fbm(d * 3, 71, 4)[..., None] * c("0a1030") * 0.6
    st = stars(d, 72, 0.012, 160) * 1.4 + stars(d, 73, 0.004, 70) * 1.8
    return col + st[..., None] * np.array([0.9, 0.95, 1.0])


def s_orion_nebula(d):
    n1 = fbm(d * 2.4, 81)
    n2 = fbm(d * 5 + 2.3, 83)
    col = c("040208") + smooth(0.45, 0.85, n1)[..., None] * c("e23a4a") * 1.1 + smooth(0.5, 0.85, n2)[..., None] * c("3a7bff") * 0.9
    col = col + smooth(0.62, 0.9, n1 * n2 * 1.8)[..., None] * c("ffe0e8") * 0.5
    return col + starfield(d, 84, 1.0)[..., None]


def s_violet_nebula(d):
    n1 = fbm(d * 2.0 + 7, 91)
    n2 = fbm(d * 4.2, 93)
    col = c("06020f") + smooth(0.4, 0.85, n1)[..., None] * c("9b3cff") + smooth(0.55, 0.85, n2)[..., None] * c("ff5ac8") * 0.7
    return col + starfield(d, 94, 1.0)[..., None]


def aurora(d, seed, cols):
    y = d[..., 1]
    ang = np.arctan2(d[..., 2], d[..., 0])
    out = np.zeros(d.shape)
    for k, col in enumerate(cols):
        wave = 0.32 + 0.1 * np.sin(ang * 3 + k * 1.7) + 0.05 * np.sin(ang * 9 + k)
        streaks = 0.6 + 0.4 * np.sin(ang * 60 + value_noise(np.stack([ang * 3, np.full_like(ang, k), y], -1), seed) * 6)
        curtain = smooth(wave - 0.02, wave + 0.04, y) * (1 - smooth(wave + 0.04, wave + 0.32, y))
        out = out + (curtain * streaks)[..., None] * col
    return out


def s_aurora_borealis(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("020408")), (-0.03, c("071018")), (0, c("0e2430")), (0.4, c("06101c")), (1, c("02050c"))])
    col = col + starfield(d, 100, 0.8)[..., None]
    return col + aurora(d, 101, (c("3dffa0"), c("2dd4bf") * 0.7, c("8affd1") * 0.5)) * 0.9


def s_aurora_violet(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("05030a")), (0, c("1a1030")), (0.4, c("0b0718")), (1, c("05030c"))])
    col = col + starfield(d, 110, 0.8)[..., None]
    return col + aurora(d, 111, (c("c06bff"), c("ff6bd6") * 0.8, c("6b8bff") * 0.6))


def s_cumulus_day(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("5d6e7e")), (-0.02, c("9fb2c4")), (0, c("cfe4f5")), (0.25, c("7fb4e8")), (1, c("2f6fc2"))])
    col = col + sun_glow(d, (0.4, 0.75, -0.5), 900, 30, c("fffbe6") * 1.3, c("ffffff") * 0.25)
    cl, n = clouds(d, 120, 1.3, 0.45)
    shade = mix(c("ffffff"), c("aab6c6"), smooth(0.55, 0.9, n))
    return mix(col, shade, cl * 0.95)


def s_overcast(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("4a4f55")), (0, c("a7adb3")), (1, c("8a9097"))])
    n = fbm(d * 2.5, 130, 5)
    return col * (0.85 + 0.25 * n)[..., None]


def s_stormfront(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("101214")), (0, c("2e3238")), (0.3, c("1d2026")), (1, c("0d0f12"))])
    cl, n = clouds(d, 140, 0.8, 0.7, 0.0, 0.9)
    col = mix(col, mix(c("3d434d"), c("0e1013"), n), cl)
    flash = sun_glow(d, (-0.6, 0.35, 0.7), 8, 3, c("8fa6ff") * 0.25, c("4a5a8a") * 0.1)
    return col + flash * cl[..., None]


def s_blood_moon(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("0b0203")), (0, c("3a0b0d")), (0.3, c("1a0507")), (1, c("070102"))])
    md = (0.3, 0.45, -0.85)
    m = disc(d, md, 7.0, 0.05)
    surface = 0.75 + 0.25 * fbm(d * 40, 150, 3)
    col = col + sun_glow(d, md, 30, 4, c("ff2a1a") * 0.4, c("8a0a0a") * 0.25)
    col = mix(col, c("c4281c") * surface[..., None], m)
    return col + (starfield(d, 151, 0.5) * (1 - m))[..., None] * c("ffb0a0")


def s_eclipse(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("030305")), (0, c("1a1a2e")), (0.4, c("0a0a14")), (1, c("030306"))])
    sd = np.array([0.0, 0.55, -0.83]); sd /= np.linalg.norm(sd)
    ang = np.degrees(np.arccos(np.clip(d @ sd, -1, 1)))
    corona = np.exp(-np.maximum(ang - 5, 0) / 2.5) * (ang > 4.6)
    rays = 0.7 + 0.3 * np.sin(np.arctan2(d[..., 0], d[..., 2] + 0.0001) * 40)
    col = col + (corona * rays)[..., None] * c("fff3d6") * 1.6
    col = col * (1 - smooth(5.2, 4.8, ang))[..., None]
    return col + starfield(d, 161, 0.8)[..., None]


def s_synthwave(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("0b0018")), (-0.01, c("1d0640")), (0, c("ff3fa4")), (0.12, c("8a2be2")), (0.5, c("2a0a5e")), (1, c("0a0220"))])
    sd = (0.0, 0.12, -1)
    sun = disc(d, sd, 14, 0.03)
    sv = d[..., 1]
    stripes = (np.mod((0.2 - sv) * 60, 2) < 1.0) & (sv < 0.1)
    sun_col = mix(c("ffe14d"), c("ff3fa4"), smooth(0.25, 0.0, sv))
    col = mix(col, sun_col, sun * (1 - stripes))
    # Neon grid below the horizon.
    k = 1.0 / np.maximum(-y, 0.02)
    gx, gz = d[..., 0] * k, d[..., 2] * k
    lines = np.maximum(glow(np.abs(np.mod(gx, 2) - 1) - 1, 0.03 * k ** 0.5), glow(np.abs(np.mod(gz, 2) - 1) - 1, 0.03 * k ** 0.5))
    floor = (y < 0)[..., None]
    grid_col = c("0b0018") + np.clip(lines, 0, 1)[..., None] * c("21e6ff") * smooth(-1, -0.02, -y)[..., None]
    return np.where(floor, grid_col, col + (starfield(d, 171, 0.6) * smooth(0.3, 0.7, y))[..., None])


def s_neon_dusk(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("05030f")), (0, c("2b0a3d")), (0.08, c("ff2bd6")), (0.25, c("4a1aa8")), (1, c("070b2a"))])
    cl, n = clouds(d, 180, 1.0, 0.4, 0.02, 0.5)
    col = mix(col, mix(c("21e6ff"), c("ff2bd6"), n), cl * 0.35)
    return col + (starfield(d, 181, 0.8) * smooth(0.3, 0.6, y))[..., None]


def s_frozen_twilight(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("1b2a3a")), (0, c("bfe6ff")), (0.25, c("7aaee0")), (1, c("1c2f5c"))])
    col = col + aurora(d, 191, (c("b8f7ff") * 0.6,)) * 0.6
    return col + (starfield(d, 192, 0.7) * smooth(0.4, 0.8, y))[..., None]


def s_twin_moons(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("1b1630")), (0, c("f2a7c3")), (0.2, c("9b8bd6")), (1, c("2d2a6e"))])
    for md, r, cl in (((0.5, 0.4, -0.75), 9, c("f6f0ff")), ((-0.3, 0.6, -0.74), 5, c("ffd8a8"))):
        m = disc(d, md, r, 0.04)
        shade = 0.8 + 0.2 * fbm(d * 30, int(r) * 7, 3)
        col = mix(col + sun_glow(d, md, 20, 6, cl * 0.15, cl * 0.08), cl * shade[..., None], m)
    cl, _ = clouds(d, 200, 1.1, 0.35)
    return mix(col, c("ffe6f0"), cl * 0.45)


def s_candy(d):
    y = d[..., 1]
    return sky_gradient(y, [(-1, c("b7a8e8")), (0, c("ffd1e6")), (0.4, c("bfe3ff")), (1, c("8ec9ff"))])


def s_minimal_azure(d):
    y = d[..., 1]
    return sky_gradient(y, [(-1, c("9fb6c9")), (0, c("e6f3ff")), (0.3, c("8cc4f2")), (1, c("3b82c9"))])


def s_minimal_mono(d):
    y = d[..., 1]
    return sky_gradient(y, [(-1, c("2a2b2e")), (0, c("d9dadd")), (0.5, c("7a7d84")), (1, c("3a3c41"))])


def s_end_void(d):
    col = c("0d0814") + fbm(d * 4, 210, 4)[..., None] * c("2a1840") * 0.8
    speck = stars(d, 211, 0.05, 200) * 0.8
    return col + speck[..., None] * c("d9c2ff")


def s_nether_haze(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("2a0604")), (0, c("6b1a0e")), (0.4, c("3a0a07")), (1, c("1a0403"))])
    n = fbm(d * 3, 220, 5)
    col = col * (0.75 + 0.5 * n)[..., None]
    return col + smooth(0.62, 0.8, fbm(d * 9, 221, 3))[..., None] * c("ff6a1f") * 0.25


def s_blocky_noon(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("6e8bb8")), (0, c("b9d3ff")), (0.3, c("7fa8ff")), (1, c("5a87ff"))])
    sd = np.array([0.0, 0.92, -0.39]); sd /= np.linalg.norm(sd)
    # Square sun (axis-aligned in the sky's tangent plane).
    t1 = np.cross(sd, [1.0, 0, 0]); t1 /= np.linalg.norm(t1)
    t2 = np.cross(sd, t1)
    a, b = d @ t1, d @ t2
    sun = (np.maximum(np.abs(a), np.abs(b)) < 0.07) & ((d @ sd) > 0)
    col = mix(col, c("fffbd0"), sun * 1.0) + sun_glow(d, sd, 40, 6, c("ffffff") * 0.2, c("ffffff") * 0.08)
    # Blocky clouds: quantised cloud plane.
    k = 1.0 / np.maximum(y, 0.03)
    px, pz = np.floor(d[..., 0] * k * 3), np.floor(d[..., 2] * k * 3)
    n = np.mod(np.sin(px * 12.9898 + pz * 78.233) * 43758.5453, 1)
    n2 = value_noise(np.stack([px * 0.3, pz * 0.3, np.zeros_like(px)], -1), 230)
    cloud = ((n2 > 0.55) & (y > 0.05)) * (1 - smooth(0.55, 0.9, y))
    return mix(col, c("ffffff") * (0.92 + 0.08 * n)[..., None], cloud)


def s_kestrel_ember(d):
    y = d[..., 1]
    col = sky_gradient(y, [(-1, c("120c09")), (-0.02, c("2b1a10")), (0, c("e8833a")), (0.15, c("8a4f24")), (0.45, c("2a2d33")), (1, c("101115"))])
    col = col + sun_glow(d, (0, 0.05, -1), 500, 8, c("ffd6a8") * 1.2, c("e8833a") * 0.4)
    # Wing-shaped constellation of bright stars above the sunset.
    pts = [(-0.5, 0.55), (-0.3, 0.5), (-0.12, 0.42), (0, 0.38), (0.12, 0.42), (0.3, 0.5), (0.5, 0.55), (0, 0.3), (0, 0.2)]
    for x, yy in pts:
        dd = np.array([x, yy, -1.0])
        col = col + (disc(d, dd, 0.5, 0.3)[..., None] * c("ffe2c4") * 1.2) + sun_glow(d, dd, 3000, 400, c("ffb070") * 0.4, 0)
    return col + (starfield(d, 240, 0.6) * smooth(0.35, 0.7, y))[..., None]


SKIES = [
    # id, name, category, fn
    ("golden_hour", "Golden Hour", "sunsets", s_golden_hour),
    ("pink_sunrise", "Pink Sunrise", "sunsets", s_pink_sunrise),
    ("crimson_sunset", "Crimson Sunset", "sunsets", s_crimson_sunset),
    ("desert_dusk", "Desert Dusk", "sunsets", s_desert_dusk),
    ("galaxy_core", "Galaxy Core", "space", s_galaxy_core),
    ("deep_space", "Deep Space", "space", s_deep_space),
    ("crimson_nebula", "Crimson Nebula", "space", s_orion_nebula),
    ("violet_nebula", "Violet Nebula", "space", s_violet_nebula),
    ("aurora_borealis", "Aurora Borealis", "atmospheric", s_aurora_borealis),
    ("violet_aurora", "Violet Aurora", "atmospheric", s_aurora_violet),
    ("cumulus", "Cumulus Day", "atmospheric", s_cumulus_day),
    ("overcast", "Overcast", "atmospheric", s_overcast),
    ("stormfront", "Stormfront", "dark", s_stormfront),
    ("blood_moon", "Blood Moon", "dark", s_blood_moon),
    ("eclipse", "Eclipse", "dark", s_eclipse),
    ("synthwave", "Synthwave", "neon", s_synthwave),
    ("neon_dusk", "Neon Dusk", "neon", s_neon_dusk),
    ("frozen_twilight", "Frozen Twilight", "fantasy", s_frozen_twilight),
    ("twin_moons", "Twin Moons", "fantasy", s_twin_moons),
    ("cotton_candy", "Cotton Candy", "minimal", s_candy),
    ("azure", "Azure", "minimal", s_minimal_azure),
    ("monochrome", "Monochrome", "minimal", s_minimal_mono),
    ("void_realm", "Void Realm", "blocky", s_end_void),
    ("ember_haze", "Ember Haze", "blocky", s_nether_haze),
    ("blocky_noon", "Blocky Noon", "blocky", s_blocky_noon),
    ("kestrel_ember", "Kestrel Ember", "kestrel", s_kestrel_ember),
]
OLD_SKIES = [("dusk", "Dusk", "sunsets"), ("starry", "Starry Night", "space"), ("nebula", "Nebula", "space")]
SKY_FACE = 512


def render_sky(fn, face_px):
    atlas = np.zeros((face_px * 2, face_px * 3, 3))
    t = (np.arange(face_px) + 0.5) / face_px
    u, v = np.meshgrid(t, t)
    for col, row, tl, bl, br, tr in FACES:
        tl, bl, br, tr = map(np.array, (tl, bl, br, tr))
        top = tl[None, None] + u[..., None] * (tr - tl)[None, None]
        bottom = bl[None, None] + u[..., None] * (br - bl)[None, None]
        p = top + v[..., None] * (bottom - top)
        dd = p / np.linalg.norm(p, axis=-1, keepdims=True)
        atlas[row * face_px:(row + 1) * face_px, col * face_px:(col + 1) * face_px] = fn(dd)
    img = np.clip(atlas, 0, 1) ** (1 / 1.1)
    # Ordered dither to avoid banding in smooth gradients, deterministic.
    bayer = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) / 16.0 - 0.5
    tile = np.tile(bayer, (img.shape[0] // 4 + 1, img.shape[1] // 4 + 1))[:img.shape[0], :img.shape[1]]
    img = img * 255 + tile[..., None] * 0.9 + 0.5
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB")


def sky_thumb(atlas_img):
    w, h = atlas_img.size
    strip = atlas_img.crop((0, 0, w, h // 2))
    return strip.resize((384, 128), Image.LANCZOS)


def gen_skies(only=None):
    out = os.path.join(TEX, "sky")
    thumbs = os.path.join(out, "thumbs")
    os.makedirs(thumbs, exist_ok=True)
    entries = [{"id": i, "name": n, "category": cat} for i, n, cat in OLD_SKIES]
    for i, n, cat in OLD_SKIES:
        sky_thumb(Image.open(os.path.join(out, i + ".png")).convert("RGB")).save(os.path.join(thumbs, i + ".png"), optimize=True)
    for sid, name, cat, fn in SKIES:
        entries.append({"id": sid, "name": name, "category": cat})
        if only and sid not in only:
            continue
        img = render_sky(fn, SKY_FACE)
        img.save(os.path.join(out, sid + ".png"), optimize=True)
        sky_thumb(img).save(os.path.join(thumbs, sid + ".png"), optimize=True)
        print("sky", sid, os.path.getsize(os.path.join(out, sid + ".png")) // 1024, "KB")
    return entries


def main():
    only = set(sys.argv[2:]) if len(sys.argv) > 2 else None
    old = json.load(open(os.path.join(RES, "capes.json")))
    keep = [e for e in old["capes"] if e["id"] in ("ember", "glacier", "midnight", "verdant", "aurora", "teto")]
    cats = {"ember": "static", "glacier": "static", "midnight": "static", "verdant": "static", "aurora": "animated", "teto": "special"}
    for e in keep:
        e["category"] = cats[e["id"]]
        e["name"] = e["name"].replace(" (animated)", "")
    os.makedirs(os.path.join(TEX, "capes", "thumbs"), exist_ok=True)
    for e in keep:  # thumbnails of the older capes from their textures
        if e["id"] == "teto":
            src = Image.open(os.path.join(TEX, "capes", "teto.png")).convert("RGBA")
            s = src.size[0] // 64
            src.crop((s, s, 11 * s, 17 * s)).save(os.path.join(TEX, "capes", "thumbs", "teto.png"), optimize=True)
            continue
        src = Image.open(os.path.join(TEX, e["texture"])).convert("RGBA")
        s = src.size[0] // 64
        fh = 32 * s
        frames = [src.crop((s, f * fh + s, 11 * s, f * fh + 17 * s)) for f in range(src.size[1] // fh)]
        strip = Image.new("RGBA", (10 * s * len(frames), 16 * s))
        for k, f in enumerate(frames):
            strip.paste(f, (k * 10 * s, 0))
        strip.save(os.path.join(TEX, "capes", "thumbs", e["id"] + ".png"), optimize=True)
    new = gen_capes() if not only else []
    if not only:
        catalog = {"_comment": old["_comment"] + " 'category' groups capes in the menus (static, animated, special, kestrel); "
                   "'fps' sets the frame rate of an animated PNG strip (otherwise the player's animation setting is used).",
                   "capes": keep + new}
        with open(os.path.join(RES, "capes.json"), "w") as f:
            json.dump(catalog, f, indent=2)
            f.write("\n")
    skies = gen_skies(only)
    with open(os.path.join(RES, "skies.json"), "w") as f:
        json.dump({"_comment": "Bundled skies (3x2 atlases in textures/sky/<id>.png, thumbnails in textures/sky/thumbs/). "
                   "See documentation/SKIES.md.", "skies": skies}, f, indent=2)
        f.write("\n")
    write_preview_table()
    print("done")


def write_preview_table():
    """Embed every catalog thumbnail in the launcher (bundled_previews.rs)."""
    capes = json.load(open(os.path.join(RES, "capes.json")))["capes"]
    skies = json.load(open(os.path.join(RES, "skies.json")))["skies"]
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    out = os.path.join(root, "launcher", "crates", "kestrel-launcher", "src", "bundled_previews.rs")
    rel = "../../../../client/src/main/resources/assets/kestrel/textures/"

    def table(name, kind, ids):
        rows = "".join('    ("%s", include_bytes!("%s%s/thumbs/%s.png")),\n' % (i, rel, kind, i) for i in ids)
        return "pub(crate) const %s: &[(&str, &[u8])] = &[\n%s];\n" % (name, rows)

    with open(out, "w") as f:
        f.write("// @generated by tools/gen_cosmetics.py from capes.json and skies.json; do not edit.\n"
                "//! Catalog thumbnails embedded for the launcher's previews (cape: outer face,\n"
                "//! animated capes have their frames side by side; sky: north/east/south strip).\n\n")
        f.write(table("CAPE_THUMBS", "capes", [c["id"] for c in capes]))
        f.write(table("SKY_THUMBS", "sky", [s["id"] for s in skies]))


if __name__ == "__main__":
    main()
