"""The painted panel floating in the void beyond the final sign: "THANK YOU FOR PLAYING!", a lit
brazier (the checkpoints' fire) under an arc of the bosses' medallions (the final door's), then
"BE EXCELLENT TO EACH OTHER". Pixel art, the game's font. Pure Python (no bpy): arena.py turns it
into a texture, `python3 tools/blender/finale_art.py out.png` previews it."""

import math
import random

W, H = 168, 106

# Glyphs of tools/pixel_font.py (7 rows; proportional widths).
FONT = {
    "A": (".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"),
    "B": ("####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."),
    "C": (".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."),
    "E": ("#####", "#....", "#....", "####.", "#....", "#....", "#####"),
    "F": ("#####", "#....", "#....", "####.", "#....", "#....", "#...."),
    "G": (".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####"),
    "H": ("#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"),
    "I": ("###", ".#.", ".#.", ".#.", ".#.", ".#.", "###"),
    "K": ("#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"),
    "L": ("#....", "#....", "#....", "#....", "#....", "#....", "#####"),
    "N": ("#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"),
    "O": (".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."),
    "P": ("####.", "#...#", "#...#", "####.", "#....", "#....", "#...."),
    "R": ("####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"),
    "T": ("#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."),
    "U": ("#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."),
    "X": ("#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"),
    "Y": ("#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."),
    "!": ("#", "#", "#", "#", "#", ".", "#"),
    " ": ("...",) * 7,
}

SKY = (0.06, 0.03, 0.14)
GOLD = (0.98, 0.78, 0.36)
TEXT = (0.86, 0.86, 0.84)
SHADOW = (0.0, 0.0, 0.0)
STONE = (0.42, 0.4, 0.44)
STONE_D = (0.26, 0.24, 0.3)
EMBER = (0.75, 0.22, 0.06)
FLAME = (0.98, 0.5, 0.12)
FLAME_HI = (1.0, 0.86, 0.4)
# The bosses' colours (assets/config/bosses.ron), in order; arena.py passes the real ones.
DEFAULT_COLORS = [(1.0, 0.5, 0.2), (1.0, 0.3, 0.1), (0.8, 0.1, 0.1), (0.35, 1.0, 0.3),
                  (0.68, 0.36, 1.0), (0.4, 0.85, 1.0), (0.9, 0.9, 0.85)]


def image(colors=None):
    """Rows from top to bottom, each a list of (r, g, b) in 0..1 (sRGB)."""
    colors = colors or DEFAULT_COLORS
    px = [[SKY] * W for _ in range(H)]

    def put(x, y, c):
        if 0 <= x < W and 0 <= y < H:
            px[y][x] = c

    def rect(x0, y0, x1, y1, c):
        for y in range(y0, y1):
            for x in range(x0, x1):
                put(x, y, c)

    def disc(cx, cy, r, c, ry=None):
        ry = ry or r
        for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
            for x in range(int(cx - r) - 1, int(cx + r) + 2):
                if ((x - cx) / r) ** 2 + ((y - cy) / ry) ** 2 <= 1.0:
                    put(x, y, c)

    def text(line, y, color, scale=2):
        w = sum(len(FONT[ch][0]) + 1 for ch in line) * scale - scale
        x = (W - w) // 2
        for ch in line:
            for r, row in enumerate(FONT[ch]):
                for c, bit in enumerate(row):
                    if bit == "#":
                        gx, gy = x + c * scale, y + r * scale
                        rect(gx + 1, gy + 1, gx + scale + 1, gy + scale + 1, SHADOW)
                        rect(gx, gy, gx + scale, gy + scale, color)
            x += (len(FONT[ch][0]) + 1) * scale

    # Stars, away from the words and the emblem.
    rng = random.Random(1989)
    for _ in range(110):
        x, y = rng.randrange(W), rng.randrange(H)
        if (2 <= y < 36 or 72 <= y < 106) or (abs(x - W / 2) < 34 and 36 <= y < 72):
            continue
        b = 0.4 + rng.random() * 0.6
        put(x, y, (b, b, b * 0.95 + 0.05))

    text("THANK YOU", 3, GOLD)
    text("FOR PLAYING!", 19, GOLD)

    # --- The emblem: a lit brazier under the arc of the seven medallions.
    cx, base = W // 2, 70
    for k, col in enumerate(colors):
        a = math.radians(-150 + 120 * k / max(1, len(colors) - 1))  # from the left, over the top
        mx, my = cx + 26 * math.cos(a), 61 + 21 * math.sin(a)
        disc(mx, my, 3.4, STONE_D)
        disc(mx, my, 2.4, col)
        put(round(mx) - 1, round(my) - 1, tuple(min(1.0, 0.4 + v) for v in col))  # glint
    # Pedestal and bowl.
    rect(cx - 6, base - 2, cx + 7, base, STONE_D)
    rect(cx - 3, base - 9, cx + 4, base - 2, STONE)
    rect(cx - 3, base - 9, cx - 1, base - 2, STONE_D)
    rect(cx - 11, base - 13, cx + 12, base - 9, STONE)
    rect(cx - 9, base - 9, cx + 10, base - 8, STONE_D)
    rect(cx - 10, base - 14, cx + 11, base - 13, EMBER)
    # The flame: three tongues, a bright heart.
    for dx, h, r in ((-5, 8, 3.0), (5, 7, 2.8), (0, 12, 4.0)):
        for k in range(h):
            t = k / h
            disc(cx + dx + math.sin(t * 5 + dx) * 0.8, base - 14 - k, r * (1 - t) + 0.5, FLAME, (r * (1 - t) + 0.5) * 0.8)
    for k in range(7):
        t = k / 7
        disc(cx + math.sin(t * 4) * 0.6, base - 15 - k, 2.2 * (1 - t) + 0.4, FLAME_HI)
    for x, y in ((cx - 8, base - 26), (cx + 7, base - 29)):  # sparks
        put(x, y, FLAME_HI)

    text("BE EXCELLENT", 74, TEXT)
    text("TO EACH OTHER", 90, TEXT)
    return px


if __name__ == "__main__":
    import struct
    import sys
    import zlib

    k = 4
    px = image()
    raw = b"".join(b"\0" + bytes(int(min(max(v, 0), 1) * 255) for c in row for c in [c] * k for v in c)
                   for row in px for _ in range(k))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W * k, H * k, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
    open(sys.argv[1], "wb").write(png)
