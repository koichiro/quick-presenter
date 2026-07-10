#!/usr/bin/env python3
"""Generate platform icon assets from the 1024px Quick Presenter source icon."""
from __future__ import annotations

import argparse
import io
from pathlib import Path

try:
    from PIL import Image
except ImportError as exc:
    raise SystemExit("Pillow is required. Install it with: python3 -m pip install Pillow") from exc

PNG_SIZES = (16, 24, 32, 48, 64, 71, 128, 150, 256, 300, 512, 1024, 1080)
WINDOWS_ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
MACOS_ARTWORK_SCALE = 0.84
MACOS_ICONSET_ENTRIES = (
    ("icp4", 16),
    ("icp5", 32),
    ("icp6", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
    ("ic11", 32),
    ("ic12", 64),
    ("ic13", 256),
    ("ic14", 512),
)


def resized(source: Image.Image, size: int) -> Image.Image:
    image = source.resize((size, size), Image.Resampling.LANCZOS)
    pixels = image.load()
    for y in range(size):
        for x in range(size):
            *_, a = pixels[x, y]
            if a <= 2:
                pixels[x, y] = (0, 0, 0, 0)
    return image


def generate_pngs(source: Image.Image, output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for size in PNG_SIZES:
        resized(source, size).save(output_dir / f"quick-presenter-icon-{size}.png")


def generate_windows_ico(source: Image.Image, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    images = [resized(source, size) for size in WINDOWS_ICO_SIZES]
    images[-1].save(output_path, append_images=images[:-1])


def macos_resized(source: Image.Image, size: int) -> Image.Image:
    artwork_size = round(size * MACOS_ARTWORK_SCALE)
    artwork = resized(source, artwork_size)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    offset = ((size - artwork_size) // 2, (size - artwork_size) // 2)
    canvas.alpha_composite(artwork, offset)
    return canvas


def generate_macos_icns(source: Image.Image, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    chunks: list[bytes] = []
    for type_code, size in MACOS_ICONSET_ENTRIES:
        buffer = io.BytesIO()
        macos_resized(source, size).save(buffer, format="PNG")
        payload = buffer.getvalue()
        chunks.append(type_code.encode("ascii") + (len(payload) + 8).to_bytes(4, "big") + payload)

    total_size = 8 + sum(len(chunk) for chunk in chunks)
    output_path.write_bytes(b"icns" + total_size.to_bytes(4, "big") + b"".join(chunks))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source",
        default="assets/icons/source/quick-presenter-icon-1024.png",
        help="1024x1024 source PNG",
    )
    parser.add_argument("--png-dir", default="assets/icons/png", help="output directory for PNG icons")
    parser.add_argument(
        "--windows-ico",
        default="assets/icons/windows/quick-presenter.ico",
        help="output path for the Windows ICO file",
    )
    parser.add_argument(
        "--macos-icns",
        default="assets/icons/macos/QuickPresenter.icns",
        help="output path for the macOS ICNS file",
    )
    args = parser.parse_args()

    source_path = Path(args.source)
    source = Image.open(source_path).convert("RGBA")
    if source.size != (1024, 1024):
        raise SystemExit(f"Source icon must be 1024x1024, got {source.size[0]}x{source.size[1]}")

    generate_pngs(source, Path(args.png_dir))
    generate_windows_ico(source, Path(args.windows_ico))
    generate_macos_icns(source, Path(args.macos_icns))


if __name__ == "__main__":
    main()
