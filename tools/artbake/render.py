#!/usr/bin/env python3
"""Bake-off renderer: JSON palette-index grid -> RGBA PNG (stdlib only).

Usage: render.py <sprite.json> <out.png>
sprite.json: {"grid": [[64 ints], x64]} with palette indices 0..15.
Index 0 is transparent. Exits non-zero with a clear message on bad input.
"""
import json
import struct
import sys
import zlib

N = None  # derived from the grid (any square size; bake-offs use 64 or 128)
PALETTE = [  # name, r, g, b  (index 0 handled as transparent)
    ("transparent", 0x00, 0x00, 0x00),
    ("ink",       0x0B, 0x0E, 0x1B),
    ("midnight",  0x23, 0x29, 0x46),
    ("gold",      0xC9, 0xA2, 0x27),
    ("palegold",  0xF4, 0xD3, 0x5E),
    ("bone",      0xD8, 0xD3, 0xC0),
    ("dimbone",   0x8A, 0x85, 0x77),
    ("red",       0xA6, 0x26, 0x39),
    ("brightred", 0xD1, 0x49, 0x5B),
    ("leather",   0x8B, 0x5E, 0x34),
    ("wood",      0x5D, 0x40, 0x20),
    ("steel",     0x9A, 0xA0, 0xA8),
    ("steeldk",   0x5C, 0x64, 0x70),
    ("white",     0xF5, 0xF1, 0xE6),
    ("green",     0x4A, 0x7C, 0x59),
    ("bruise",    0x50, 0x3A, 0x6B),
]


def chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))


def main() -> int:
    src, dst = sys.argv[1], sys.argv[2]
    with open(src) as fh:
        spec = json.load(fh)
    grid = spec.get("grid")
    if not isinstance(grid, list) or not (16 <= len(grid) <= 256):
        sys.exit("grid must be a 16..256-row list")
    n = len(grid)
    raw = bytearray()
    for y, row in enumerate(grid):
        if not isinstance(row, list) or len(row) != n:
            sys.exit(f"row {y} must have {n} cells")
        raw.append(0)  # PNG filter: none
        for x, cell in enumerate(row):
            if not isinstance(cell, int) or not 0 <= cell < len(PALETTE):
                sys.exit(f"cell ({x},{y})={cell!r} not an int in 0..{len(PALETTE)-1}")
            r, g, b = PALETTE[cell][1], PALETTE[cell][2], PALETTE[cell][3]
            a = 0 if cell == 0 else 255
            raw += bytes((r, g, b, a))
    ihdr = struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)  # 8-bit RGBA
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")
    with open(dst, "wb") as fh:
        fh.write(png)
    print(f"ok {dst} ({len(png)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
