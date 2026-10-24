#!/usr/bin/env python3
"""Generate Kestrel's bundled cosmetic textures (deterministic).

Capes: 64x32 Minecraft cape layout (animated capes stack frames vertically).
Skies: 3x2 atlas (north, east, south / west, up, down). Each texel is
sampled from a function of its 3D view direction, using the exact corner
table from SkyboxRenderer.java, so faces join without seams.

Usage: python tools/gen_assets.py [output_resources_dir]
Requires Pillow and NumPy.
"""
import os
import sys

import numpy as np
from PIL import Image

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    os.path.dirname(__file__), "..", "client", "src", "main", "resources", "assets", "kestrel", "textures")

R = 1.0
# (col, row, TL, BL, BR, TR) — must match SkyboxRenderer.render().
FACES = [
    (0, 0, (-R, R, -R), (-R, -R, -R), (R, -R, -R), (R, R, -R)),   # north
    (1, 0, (R, R, -R), (R, -R, -R), (R, -R, R), (R, R, R)),       # east
    (2, 0, (R, R, R), (R, -R, R), (-R, -R, R), (-R, R, R)),       # south
    (0, 1, (-R, R, R), (-R, -R, R), (-R, -R, -R), (-R, R, -R)),   # west
    (1, 1, (-R, R, R), (-R, R, -R), (R, R, -R), (R, R, R)),       # up
    (2, 1, (-R, -R, -R), (-R, -R, R), (R, -R, R), (R, -R, -R)),   # down
]


# ---------------------------------------------------------------- noise
def hash3(ix, iy, iz, seed):
    h = (ix * 374761393 + iy * 668265263 + iz * 2147483647 + seed * 144665) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def value_noise(p, seed):
    """Smooth 3D value noise; p has shape (..., 3)."""
    fl = np.floor(p)
    f = p - fl
    f = f * f * (3 - 2 * f)
    i = fl.astype(np.int64)
    out = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (np.where(dx, f[..., 0], 1 - f[..., 0]) *
                     np.where(dy, f[..., 1], 1 - f[..., 1]) *
                     np.where(dz, f[..., 2], 1 - f[..., 2]))
                out = out + w * hash3(i[..., 0] + dx, i[..., 1] + dy, i[..., 2] + dz, seed)
    return out


def fbm(p, seed, octaves=5):
    total, amp, norm = 0.0, 0.5, 0.0
    for o in range(octaves):
        total = total + amp * value_noise(p * (2 ** o), seed + o * 31)
        norm += amp
        amp *= 0.5
    return total / norm


def stars(d, seed, density, size):
    """Point stars from a hashed 3D grid on the unit sphere."""
    g = d * size
    i = np.floor(g).astype(np.int64)
    r = hash3(i[..., 0], i[..., 1], i[..., 2], seed)
    centre = (i + 0.5) / size
    dist = np.linalg.norm(d - centre / np.linalg.norm(centre, axis=-1, keepdims=True), axis=-1) * size
    on = r > 1 - density
    bright = (r - (1 - density)) / density
    return np.where(on, np.clip(1.4 - dist * 3.0, 0, 1) * (0.4 + 0.6 * bright), 0.0)


def mix(a, b, t):
    t = np.clip(t, 0, 1)[..., None]
    return a * (1 - t) + b * t


# ---------------------------------------------------------------- skies
def sky_dusk(d):
    y = d[..., 1]
    top = np.array([0.10, 0.12, 0.30])
    mid = np.array([0.55, 0.35, 0.55])
    hor = np.array([1.00, 0.55, 0.25])
    low = np.array([0.12, 0.08, 0.12])
    col = np.where((y >= 0)[..., None], mix(hor, mix(mid, top, (y - 0.25) / 0.75), y / 0.25), mix(hor, low, -y / 0.15))
    # Sun glow towards the west (-X), low on the horizon.
    sun = np.array([-0.93, 0.08, 0.35]); sun /= np.linalg.norm(sun)
    s = np.clip((d @ sun), 0, 1)
    col = col + (s ** 40)[..., None] * np.array([1.0, 0.75, 0.45]) + (s ** 6)[..., None] * np.array([0.35, 0.15, 0.05])
    clouds = fbm(d * 3.0 + np.array([0, 7, 0]), 11)
    band = np.clip(1 - np.abs(y - 0.18) / 0.18, 0, 1)
    col = mix(col, np.array([0.95, 0.55, 0.45]), np.clip((clouds - 0.55) * 3, 0, 1) * band * 0.7)
    col = col + (stars(d, 3, 0.02, 90) * np.clip((y - 0.45) * 2, 0, 1))[..., None]
    return col


def sky_starry(d):
    y = d[..., 1]
    base = mix(np.array([0.05, 0.07, 0.16]), np.array([0.01, 0.01, 0.04]), (y + 0.1) / 1.0)
    horizon = np.clip(1 - np.abs(y) / 0.25, 0, 1) ** 2
    base = base + horizon[..., None] * np.array([0.06, 0.08, 0.16])
    # Milky way: band around a tilted great circle.
    axis = np.array([0.3, 0.55, 0.78]); axis /= np.linalg.norm(axis)
    band = np.exp(-((d @ axis) / 0.22) ** 2)
    dust = fbm(d * 6.0, 5)
    base = base + (band * np.clip(dust - 0.35, 0, 1) * 0.55)[..., None] * np.array([0.75, 0.7, 0.95])
    st = stars(d, 1, 0.035, 140) + stars(d, 2, 0.01, 60) * 1.3 + band * stars(d, 4, 0.12, 220) * 0.6
    col = base + st[..., None] * np.array([1.0, 0.97, 0.92])
    return np.where((y < -0.05)[..., None], base * 0.5, col)


def sky_nebula(d):
    y = d[..., 1]
    n1 = fbm(d * 2.2, 21)
    n2 = fbm(d * 4.5 + 3.1, 37)
    base = np.array([0.02, 0.01, 0.05])
    magenta = np.array([0.75, 0.15, 0.55])
    teal = np.array([0.05, 0.55, 0.65])
    col = base + np.clip(n1 - 0.45, 0, 1)[..., None] * 2.2 * magenta + np.clip(n2 - 0.5, 0, 1)[..., None] * 2.0 * teal
    col = col + (stars(d, 9, 0.03, 120) + stars(d, 10, 0.008, 50) * 1.5)[..., None]
    return np.where((y < -0.05)[..., None], col * 0.6, col)


def render_sky(fn, face_px):
    atlas = np.zeros((face_px * 2, face_px * 3, 3))
    t = (np.arange(face_px) + 0.5) / face_px
    u, v = np.meshgrid(t, t)
    for col, row, tl, bl, br, tr in FACES:
        tl, bl, br, tr = map(np.array, (tl, bl, br, tr))
        top = tl[None, None] + u[..., None] * (tr - tl)[None, None]
        bottom = bl[None, None] + u[..., None] * (br - bl)[None, None]
        p = top + v[..., None] * (bottom - top)
        d = p / np.linalg.norm(p, axis=-1, keepdims=True)
        atlas[row * face_px:(row + 1) * face_px, col * face_px:(col + 1) * face_px] = fn(d)
    img = np.clip(atlas, 0, 1) ** (1 / 1.1)
    return Image.fromarray((img * 255 + 0.5).astype(np.uint8), "RGB")


# ---------------------------------------------------------------- capes
def cape_canvas():
    return np.zeros((32, 64, 4))


def paint_cape(c, front):
    """front: (17-1)x(11-1) = 16x10 RGBA design for the outer face; mirrored on the inner face."""
    c[1:17, 1:11] = front
    c[1:17, 12:22] = front[:, ::-1] * np.array([0.7, 0.7, 0.7, 1])
    edge = front.mean(axis=(0, 1))
    c[0, 1:11] = edge                 # top
    c[0, 11:21] = edge * [0.8, 0.8, 0.8, 1]  # bottom
    c[1:17, 0] = front[:, 0]          # sides
    c[1:17, 11] = front[:, -1]
    return c


def gradient_design(top, bottom, accent, pattern):
    yy, xx = np.mgrid[0:16, 0:10]
    t = (yy / 15.0)[..., None]
    rgb = np.array(top) * (1 - t) + np.array(bottom) * t
    mask = pattern(xx, yy)
    rgb = rgb * (1 - mask[..., None]) + np.array(accent) * mask[..., None]
    border = ((xx == 0) | (xx == 9) | (yy == 15)).astype(float) * 0.25
    rgb = rgb * (1 - border[..., None])
    return np.concatenate([np.clip(rgb, 0, 1), np.ones((16, 10, 1))], axis=-1)


def save_cape_frames(name, frames):
    img = np.concatenate(frames, axis=0)
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA").save(
        os.path.join(OUT, "capes", name + ".png"), optimize=True)


def main():
    os.makedirs(os.path.join(OUT, "capes"), exist_ok=True)
    os.makedirs(os.path.join(OUT, "sky"), exist_ok=True)

    designs = {
        "ember": gradient_design((0.95, 0.45, 0.12), (0.45, 0.06, 0.05), (1.0, 0.85, 0.35),
                                 lambda x, y: ((np.abs(x - 4.5) + (15 - y) * 0.35) < 2.2).astype(float)),
        "glacier": gradient_design((0.80, 0.95, 1.0), (0.20, 0.45, 0.75), (1.0, 1.0, 1.0),
                                   lambda x, y: (((x + y) % 5) == 0).astype(float) * 0.6),
        "midnight": gradient_design((0.10, 0.10, 0.22), (0.02, 0.02, 0.06), (0.91, 0.51, 0.23),
                                    lambda x, y: ((y == 4) | (y == 5)).astype(float)),
        "verdant": gradient_design((0.35, 0.80, 0.35), (0.05, 0.30, 0.12), (0.85, 1.0, 0.55),
                                   lambda x, y: ((np.abs(x - 4.5) < 0.6) & (y > 2)).astype(float) * 0.8),
    }
    for name, d in designs.items():
        save_cape_frames(name, [paint_cape(cape_canvas(), d)])

    # Aurora: 8-frame animated shimmer.
    frames = []
    for f in range(8):
        yy, xx = np.mgrid[0:16, 0:10]
        wave = 0.5 + 0.5 * np.sin((xx * 0.7 + yy * 0.35) + f * (2 * np.pi / 8))
        rgb = (np.array([0.05, 0.08, 0.20])[None, None] * (1 - wave[..., None])
               + np.array([0.20, 0.95, 0.70])[None, None] * wave[..., None] * (1 - yy[..., None] / 20))
        rgb = rgb + (wave[..., None] ** 6) * np.array([0.6, 0.3, 0.9])
        design = np.concatenate([np.clip(rgb, 0, 1), np.ones((16, 10, 1))], axis=-1)
        frames.append(paint_cape(cape_canvas(), design))
    save_cape_frames("aurora", frames)

    face = 512
    for name, fn in (("dusk", sky_dusk), ("starry", sky_starry), ("nebula", sky_nebula)):
        render_sky(fn, face).save(os.path.join(OUT, "sky", name + ".png"), optimize=True)
        print("sky", name)
    print("assets written to", os.path.abspath(OUT))


if __name__ == "__main__":
    main()
