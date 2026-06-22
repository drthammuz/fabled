"""Bake an sRGB base_color tint into a colormap (linear-correct).

A faction whose look is currently `cyber_colormap.png x base_color tint` can be
made folder-owned by baking the tint into its own colormap, then loading it with
base_color = white. Bevy multiplies base_color (linear) by the decoded (linear)
texture, so we bake in LINEAR space to reproduce the look pixel-for-pixel.

Usage: python tools/bake_tinted_colormap.py <src.png> <dst.png> R G B
       R G B = sRGB tint in 0..1 (e.g. 0.68 0.55 0.42)
"""
import sys
import numpy as np
from PIL import Image


def srgb_to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(c):
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055)


def main() -> None:
    src, dst = sys.argv[1], sys.argv[2]
    tint = np.array([float(sys.argv[3]), float(sys.argv[4]), float(sys.argv[5])])
    tint_lin = srgb_to_linear(tint)
    img = Image.open(src).convert("RGBA")
    a = np.asarray(img).astype(np.float32) / 255.0
    lin = srgb_to_linear(a[..., :3]) * tint_lin
    a[..., :3] = np.clip(linear_to_srgb(lin), 0, 1)
    out = (np.clip(a, 0, 1) * 255.0).round().astype(np.uint8)
    Image.fromarray(out, "RGBA").save(dst)
    print(f"baked {src} x sRGB{tuple(tint)} -> {dst}")


if __name__ == "__main__":
    main()
