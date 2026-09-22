"""Generate HushType's app and tray icons as PNG (no third-party deps).

Design: a rounded square with a vertical gradient and five white
"voice" bars. Antialiasing is analytic (signed-distance coverage).

    python scripts/gen-icons.py
Writes apps/desktop/src-tauri/icons/app-icon.png (1024 px, feed to `tauri icon`)
and apps/desktop/src-tauri/icons/tray-{idle,recording,processing}.png (32 px).
"""
import math
import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(__file__), "..", "apps", "desktop", "src-tauri", "icons")


def png(path, w, h, rgba):
    raw = b"".join(b"\x00" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


def sd_round_rect(px, py, cx, cy, hw, hh, r):
    """Signed distance to a rounded rectangle centred at (cx, cy)."""
    qx = abs(px - cx) - (hw - r)
    qy = abs(py - cy) - (hh - r)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    inside = min(max(qx, qy), 0.0)
    return outside + inside - r


def render(size, top, bottom, margin=0.0, radius=0.23):
    s = float(size)
    m = s * margin
    half = (s - 2 * m) / 2
    r = half * 2 * radius
    heights = [0.30, 0.56, 0.80, 0.56, 0.30]
    bar_w = (s - 2 * m) * 0.085
    gap = (s - 2 * m) * 0.145
    bars = []
    for i, hgt in enumerate(heights):
        cx = s / 2 + (i - 2) * gap
        bars.append((cx, hgt * (s - 2 * m) / 2))
    out = bytearray(size * size * 4)
    for y in range(size):
        py = y + 0.5
        t = py / s
        cr = top[0] + (bottom[0] - top[0]) * t
        cg = top[1] + (bottom[1] - top[1]) * t
        cb = top[2] + (bottom[2] - top[2]) * t
        for x in range(size):
            px = x + 0.5
            d = sd_round_rect(px, py, s / 2, s / 2, half, half, r)
            a = min(max(0.5 - d, 0.0), 1.0)
            if a <= 0.0:
                continue
            # White bars (capsules) on top.
            w = 0.0
            for (bx, bh) in bars:
                dd = sd_round_rect(px, py, bx, s / 2, bar_w / 2, bh / 2 + bar_w / 2, bar_w / 2)
                w = max(w, min(max(0.5 - dd, 0.0), 1.0))
            rr = cr + (255 - cr) * w
            gg = cg + (255 - cg) * w
            bb = cb + (255 - cb) * w
            i = (y * size + x) * 4
            out[i:i + 4] = bytes((int(rr), int(gg), int(bb), int(a * 255)))
    return out


def main():
    os.makedirs(ROOT, exist_ok=True)
    indigo = ((99, 102, 241), (139, 92, 246))
    png(os.path.join(ROOT, "app-icon.png"), 1024, 1024, render(1024, *indigo, margin=0.06))
    for name, colors in {
        "tray-idle": indigo,
        "tray-recording": ((248, 80, 80), (220, 38, 38)),
        "tray-processing": ((251, 191, 36), (234, 138, 12)),
    }.items():
        png(os.path.join(ROOT, name + ".png"), 32, 32, render(32, *colors, margin=0.0, radius=0.28))
        print("wrote", name)


if __name__ == "__main__":
    main()
