"""Draws the z++ icon at each size and writes assets/zpp.ico plus a 256 px PNG.

Run: uv run --with pillow python assets/make_icon.py
"""

import io
import struct
from pathlib import Path

from PIL import Image, ImageDraw

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SUPERSAMPLE = 8
SMALL_MAX = 24  # at this size and below, draw one plus and thicker strokes

GRADIENT_START = (109, 40, 217)  # violet, top left
GRADIENT_END = (14, 165, 233)  # sky blue, bottom right
LETTER = (255, 255, 255)
PLUS = (251, 191, 36)  # amber
CORNER = 0.22

OUT_DIR = Path(__file__).resolve().parent


def gradient(px: int) -> Image.Image:
    img = Image.new("RGB", (px, px))
    pixels = img.load()
    for y in range(px):
        for x in range(px):
            t = (x + y) / (2 * (px - 1))
            pixels[x, y] = tuple(round(a + (b - a) * t) for a, b in zip(GRADIENT_START, GRADIENT_END))
    return img


def draw_z(d: ImageDraw.ImageDraw, px: int, left: float, right: float, top: float, bottom: float, stroke: float) -> None:
    s = lambda v: v * px  # noqa: E731
    d.rectangle([s(left), s(top), s(right), s(top + stroke)], fill=LETTER)
    d.rectangle([s(left), s(bottom - stroke), s(right), s(bottom)], fill=LETTER)
    slant = stroke * 1.35
    d.polygon(
        [
            (s(right - slant), s(top + stroke)),
            (s(right), s(top + stroke)),
            (s(left + slant), s(bottom - stroke)),
            (s(left), s(bottom - stroke)),
        ],
        fill=LETTER,
    )


def draw_plus(d: ImageDraw.ImageDraw, px: int, cx: float, cy: float, arm: float, thick: float) -> None:
    s = lambda v: v * px  # noqa: E731
    d.rectangle([s(cx - arm), s(cy - thick / 2), s(cx + arm), s(cy + thick / 2)], fill=PLUS)
    d.rectangle([s(cx - thick / 2), s(cy - arm), s(cx + thick / 2), s(cy + arm)], fill=PLUS)


def render(size: int) -> Image.Image:
    px = size * SUPERSAMPLE
    tile = gradient(px).convert("RGBA")
    mask = Image.new("L", (px, px), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, px - 1, px - 1], radius=CORNER * px, fill=255)
    tile.putalpha(mask)
    d = ImageDraw.Draw(tile)
    if size <= SMALL_MAX:
        draw_z(d, px, left=0.14, right=0.60, top=0.20, bottom=0.80, stroke=0.17)
        draw_plus(d, px, cx=0.79, cy=0.50, arm=0.15, thick=0.10)
    else:
        draw_z(d, px, left=0.17, right=0.61, top=0.22, bottom=0.78, stroke=0.13)
        draw_plus(d, px, cx=0.80, cy=0.39, arm=0.09, thick=0.055)
        draw_plus(d, px, cx=0.80, cy=0.61, arm=0.09, thick=0.055)
    return tile.resize((size, size), Image.LANCZOS)


def write_ico(images: list[Image.Image], path: Path) -> None:
    """ICO with PNG-encoded entries (supported since Windows Vista)."""
    blobs = []
    for img in images:
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        blobs.append(buf.getvalue())
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    for img, blob in zip(images, blobs):
        w = img.width if img.width < 256 else 0
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    path.write_bytes(header + entries + b"".join(blobs))


def main() -> None:
    images = [render(s) for s in SIZES]
    write_ico(images, OUT_DIR / "zpp.ico")
    images[-1].save(OUT_DIR / "zpp-256.png")
    preview = Image.new("RGBA", (sum(SIZES) + 10 * len(SIZES), 256), (40, 40, 40, 255))
    x = 0
    for img in images:
        preview.alpha_composite(img, (x, 256 - img.height))
        x += img.width + 10
    preview.save(OUT_DIR.parent / "target" / "icon-preview.png")


if __name__ == "__main__":
    main()
