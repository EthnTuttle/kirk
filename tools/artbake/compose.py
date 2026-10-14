#!/usr/bin/env python3
"""Compose a vertical contact sheet of every <dir>/*/sprite.png, 4x nearest,
plus INDEX.txt mapping row number -> model. Stdlib only."""
import os
import struct
import sys
import zlib

SCALE, GAP = 4, 8
BG = (0x0B, 0x0E, 0x1B, 255)


def read_png_rgba(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat, w, h = 8, b"", 0, 0
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos : pos + 4])
        tag, payload = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + ln]
        if tag == b"IHDR":
            w, h, depth, ctype, *_ = struct.unpack(">IIBBBBB", payload)
            assert depth == 8 and ctype == 6, path
        elif tag == b"IDAT":
            idat += payload
        pos += 12 + ln
    raw = zlib.decompress(idat)
    stride, rows, prev = w * 4, [], bytearray(w * 4)
    p = 0
    for _y in range(h):
        filt = raw[p]
        line = bytearray(raw[p + 1 : p + 1 + stride])
        p += 1 + stride
        if filt == 1:
            for i in range(4, stride):
                line[i] = (line[i] + line[i - 4]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                a = line[i - 4] if i >= 4 else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                a = line[i - 4] if i >= 4 else 0
                b = prev[i]
                c = prev[i - 4] if i >= 4 else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        rows.append(line)
        prev = line
    return w, h, rows


def write_png_rgba(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(tag, payload):
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", zlib.crc32(tag + payload))

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(
        b"IDAT", zlib.compress(raw, 9)
    ) + chunk(b"IEND", b"")
    open(path, "wb").write(png)


def main():
    root = sys.argv[1]
    entries = []
    for slug in sorted(os.listdir(root)):
        p = os.path.join(root, slug, "sprite.png")
        if os.path.isfile(p):
            w, h, rows = read_png_rgba(p)
            entries.append((slug, w, h, rows))
    if not entries:
        sys.exit("no sprites found")
    cw = max(w for _, w, _, _ in entries) * SCALE
    out_rows, index = [], []
    y = 0
    for i, (slug, w, h, rows) in enumerate(entries):
        index.append(f"row {i+1} (y={y}): {slug}")
        big = []
        for line in rows:  # nearest-neighbor scale
            scaled = bytearray()
            for x in range(w):
                scaled += line[x * 4 : x * 4 + 4] * SCALE
            for _ in range(SCALE):
                big.append(bytearray(scaled))
        pad = bytearray(BG * cw)
        for r in big:
            if len(r) < len(pad):
                r += pad[len(r) :]
        out_rows.extend(big)
        y += h * SCALE
        if i + 1 < len(entries):
            out_rows.extend([bytearray(BG * cw)] * GAP)
            y += GAP
    write_png_rgba(os.path.join(root, "sheet.png"), cw, y, out_rows)
    with open(os.path.join(root, "INDEX.txt"), "w") as fh:
        fh.write("\n".join(index) + "\n")
    print(f"sheet: {cw}x{y}, {len(entries)} sprites -> {root}/sheet.png (+ INDEX.txt)")


if __name__ == "__main__":
    main()
