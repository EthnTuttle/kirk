#!/usr/bin/env python3
"""Bake-off fair-play quantizer: any input PNG -> 64x64 palette-indexed PNG.

Pure stdlib. Downscales by block average, composites alpha onto transparency,
then maps every pixel to the nearest bake-off palette entry (RGB distance).
Usage: quant.py <in.png> <out.png> [--size 64]
"""
import json
import struct
import sys
import zlib

PALETTE = [
    None,
    (0x0B, 0x0E, 0x1B), (0x23, 0x29, 0x46), (0xC9, 0xA2, 0x27), (0xF4, 0xD3, 0x5E),
    (0xD8, 0xD3, 0xC0), (0x8A, 0x85, 0x77), (0xA6, 0x26, 0x39), (0xD1, 0x49, 0x5B),
    (0x8B, 0x5E, 0x34), (0x5D, 0x40, 0x20), (0x9A, 0xA0, 0xA8), (0x5C, 0x64, 0x70),
    (0xF5, 0xF1, 0xE6), (0x4A, 0x7C, 0x59), (0x50, 0x3A, 0x6B),
]


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a png"
    pos, idat, w, h, ctype = 8, b"", 0, 0, None
    plte = trns = None
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos : pos + 4])
        tag, payload = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + ln]
        if tag == b"IHDR":
            w, h, depth, ctype, comp, filt, inter = struct.unpack(">IIBBBBB", payload)
            assert depth == 8 and inter == 0 and ctype in (2, 3, 6), f"unsupported depth/ctype/interlace: {depth}/{ctype}/{inter}"
        elif tag == b"PLTE":
            plte = payload
        elif tag == b"tRNS":
            trns = payload
        elif tag == b"IDAT":
            idat += payload
        pos += 12 + ln
    ch = {6: 4, 2: 3, 3: 1}[ctype]
    stride = w * ch
    raw, rows, prev, p = zlib.decompress(idat), [], bytearray(stride), 0
    for _ in range(h):
        filt = raw[p]
        line = bytearray(raw[p + 1 : p + 1 + stride])
        p += 1 + stride
        bpp = ch
        if filt == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b, c = prev[i], prev[i - bpp] if i >= bpp else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        rgba = bytearray()
        for x in range(w):
            if ctype == 6:
                rgba += line[x * 4 : x * 4 + 4]
            elif ctype == 2:
                rgba += line[x * 3 : x * 3 + 3] + b"\xff"
            else:
                idx = line[x]
                r, g, b = plte[idx * 3 : idx * 3 + 3]
                a = trns[idx] if trns and idx < len(trns) else 255
                rgba += bytes((r, g, b, a))
        rows.append(rgba)
        prev = line
    return w, h, rows


def write_png(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(tag, payload):
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", zlib.crc32(tag + payload))

    open(path, "wb").write(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def nearest(r, g, b):
    best, bd = 0, 1 << 60
    for i, entry in enumerate(PALETTE):
        if entry is None:
            continue
        d = (r - entry[0]) ** 2 + (g - entry[1]) ** 2 + (b - entry[2]) ** 2
        if d < bd:
            best, bd = i, d
    return best


def main():
    src, dst = sys.argv[1], sys.argv[2]
    size = 64
    if "--size" in sys.argv:
        size = int(sys.argv[sys.argv.index("--size") + 1])
    w, h, rows = read_png(src)
    out = []
    palette = [e for e in PALETTE]
    for oy in range(size):
        line = bytearray()
        y0, y1 = oy * h // size, (oy + 1) * h // size
        for ox in range(size):
            x0, x1 = ox * w // size, (ox + 1) * w // size
            sr = sg = sb = sa = n = 0
            for yy in range(y0, y1):
                row = rows[yy]
                for xx in range(x0, x1):
                    r, g, b, a = row[xx * 4 : xx * 4 + 4]
                    sr, sg, sb, sa = sr + r, sg + g, sb + b, sa + a
                    n += 1
            if sa // n < 24:  # ~fully translucent -> transparent cell
                line += b"\x00\x00\x00\x00"
            else:
                for base in PALETTE[nearest(sr // n, sg // n, sb // n)] :
                    line.append(base)
                line.append(255)
        out.append(line)
    write_png(dst, size, size, out)
    print(f"ok {dst} ({size}x{size}, palette-quantized)")


if __name__ == "__main__":
    main()
