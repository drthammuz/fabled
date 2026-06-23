#!/usr/bin/env python3
"""Generate a subtle, tileable scratch/scuff detail texture for the synth floor.

The synth floor GLB samples one flat colormap swatch, so the floor reads as a single
colour. This bakes a near-white multiplier map (faint darker scratches + low-frequency
mottling) that a dedicated floor material tiles across each tile via uv_transform, giving
just enough texture to discern the floor without changing its colour scheme.

Output: assets/models/factions/synth/Textures/floor_detail.png
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets" / "models" / "factions" / "synth" / "Textures" / "floor_detail.png"
SIZE = 512


def tileable_mottle(rng: np.random.Generator) -> np.ndarray:
    """Low-frequency tileable brightness variation in [-1, 1] via integer harmonics."""
    yy, xx = np.meshgrid(np.linspace(0, 2 * np.pi, SIZE, endpoint=False),
                         np.linspace(0, 2 * np.pi, SIZE, endpoint=False), indexing="ij")
    acc = np.zeros((SIZE, SIZE), dtype=np.float64)
    for _ in range(6):
        kx, ky = rng.integers(1, 5), rng.integers(1, 5)
        ph = rng.uniform(0, 2 * np.pi)
        acc += np.sin(kx * xx + ky * yy + ph)
    return acc / np.abs(acc).max()


def main() -> None:
    rng = np.random.default_rng(7)

    # Base near-white multiplier with faint mottling (±0.03).
    mottle = tileable_mottle(rng)
    base = np.clip(0.93 + mottle * 0.03, 0.0, 1.0)
    img = Image.fromarray((base * 255).astype(np.uint8), mode="L").convert("RGB")

    # Thin scratches, drawn with ±SIZE wrapped copies so the texture tiles seamlessly.
    draw = ImageDraw.Draw(img, "RGBA")
    for _ in range(40):
        x0 = rng.uniform(0, SIZE)
        y0 = rng.uniform(0, SIZE)
        ang = rng.uniform(0, np.pi)
        length = rng.uniform(40, 200)
        x1 = x0 + np.cos(ang) * length
        y1 = y0 + np.sin(ang) * length
        dark = int(rng.uniform(150, 205))          # subtle: never near-black
        alpha = int(rng.uniform(40, 90))
        width = 1 if rng.random() < 0.7 else 2
        for ox in (-SIZE, 0, SIZE):
            for oy in (-SIZE, 0, SIZE):
                draw.line(
                    (x0 + ox, y0 + oy, x1 + ox, y1 + oy),
                    fill=(dark, dark, dark, alpha),
                    width=width,
                )

    # A few faint scuff specks.
    for _ in range(120):
        cx, cy = rng.uniform(0, SIZE), rng.uniform(0, SIZE)
        r = rng.uniform(1, 4)
        g = int(rng.uniform(170, 215))
        draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(g, g, g, int(rng.uniform(20, 50))))

    img = img.filter(ImageFilter.GaussianBlur(0.5))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT)
    print(f"wrote {OUT} ({SIZE}x{SIZE})")


if __name__ == "__main__":
    main()
