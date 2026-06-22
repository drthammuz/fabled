"""Recolor the dungeon colormap into the priesthood faction copy.

Reads the pristine dungeon atlas and writes a recolored copy into the
priesthood folder: brown dirt/rubble hues -> cool stone gray. Reproducible
regardless of the current state of the destination (always sources dungeon).
"""
import numpy as np
from PIL import Image

src = "assets/models/dungeon/Textures/colormap.png"
path = "assets/models/factions/priesthood/Textures/colormap.png"
img = Image.open(src).convert("RGBA")
arr = np.asarray(img).astype(np.float32) / 255.0
rgb = arr[..., :3]
out = rgb.copy()

r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]

# Priesthood is all-stone: it wants NO warm/earthy tones anywhere. Target browns,
# tans, oranges AND dark/low-saturation rubble rocks by CHANNEL ORDERING rather
# than a saturation threshold. Earthy tones are "warm": red >= green >= blue with
# a real red-over-blue spread. Cool stone (blue >= red) and greens (green > red)
# are left untouched, so walls keep their blue-gray stone look.
warm = (r >= g - 0.02) & (g >= b - 0.02) & ((r - b) > 0.045)
# Exclude near-pure reds (banner/accent): brown keeps green between r and b.
not_red = (g - b) > 0.02
earthy = warm & not_red

# Neutralize to cool stone-gray: keep luminance, drop saturation, slight blue lift.
lum = 0.299 * r + 0.587 * g + 0.114 * b
gray = np.stack([lum * 0.97, lum * 0.99, lum * 1.04], axis=-1)
gray = np.clip(gray, 0, 1)

for c in range(3):
    out[..., c] = np.where(earthy, gray[..., c], rgb[..., c])

arr[..., :3] = out
res = (np.clip(arr, 0, 1) * 255.0).round().astype(np.uint8)
Image.fromarray(res, "RGBA").save(path)
print("recolored pixels:", int(earthy.sum()), "of", earthy.size)
