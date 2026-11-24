"""Draw Spool's original cassette artwork into the SDK's L565 asset format."""
from math import cos, pi, sin
from pathlib import Path
from struct import pack

W, H = 240, 136
pixels = [0] * (W * H)


def ink(hex_color):
    r, g, b = bytes.fromhex(hex_color)
    return (r >> 3) << 11 | (g >> 2) << 5 | (b >> 3)


def box(x, y, w, h, color, radius=0):
    color = ink(color)
    for j in range(max(0, y), min(H, y + h)):
        for i in range(max(0, x), min(W, x + w)):
            dx = max(x + radius - i, 0, i - (x + w - 1 - radius))
            dy = max(y + radius - j, 0, j - (y + h - 1 - radius))
            if dx * dx + dy * dy <= radius * radius:
                pixels[j * W + i] = color


def disc(x, y, r, color):
    box(x - r, y - r, 2 * r + 1, 2 * r + 1, color, r)


def lettering(x, y, word, scale, color):
    glyphs = {
        "S": (15, 16, 16, 14, 1, 1, 30), "P": (30, 17, 17, 30, 16, 16, 16),
        "O": (14, 17, 17, 17, 17, 17, 14), "L": (16, 16, 16, 16, 16, 16, 31),
        "C": (15, 16, 16, 16, 16, 16, 15), "6": (14, 16, 16, 30, 17, 17, 14),
        "0": (14, 17, 19, 21, 25, 17, 14), "A": (14, 17, 17, 31, 17, 17, 17),
    }
    for letter in word:
        for j, row in enumerate(glyphs[letter]):
            for i in range(5):
                if row & (16 >> i):
                    box(x + i * scale, y + j * scale, scale, scale, color)
        x += 6 * scale


# Screen-printed shell, paper label, smoked tape window and exposed drive hubs.
box(0, 0, W, H, "457873")
box(17, 12, 210, 120, "345e59", 9)
box(14, 9, 210, 120, "203838", 8)
box(12, 6, 210, 120, "172b2e", 7)
box(14, 8, 206, 114, "384c4c", 6)
for y in range(11, 120, 3):
    box(18, y, 198, 1, "405452")
box(23, 14, 188, 82, "172c2e", 5)
box(25, 16, 184, 78, "f1dfac", 4)
lettering(34, 23, "SPOOL", 2, "213f40")
lettering(171, 26, "C60", 1, "213f40")
for y, h, color in [(42, 8, "f6af56"), (50, 9, "ed7953"), (59, 12, "c5524b")]:
    box(25, y, 184, h, color)
box(47, 44, 142, 42, "eee0b6", 17)
box(50, 47, 136, 36, "22373a", 16)
box(87, 54, 62, 21, "694735", 3)
box(93, 58, 50, 13, "aa8968", 2)
for x, r in [(70, 15), (165, 15)]:
    disc(x, 65, r, "b4bca9")
    disc(x, 65, r - 2, "f2e9cb")
    disc(x, 65, 7, "243d3d")
    for n in range(6):
        angle = n * pi / 3
        disc(round(x + 7 * cos(angle)), round(65 + 7 * sin(angle)), 2, "f2e9cb")
lettering(32, 81, "A", 1, "243d3d")
for y in range(95, 122):
    inset = (121 - y) // 4
    box(52 + inset, y, 130 - 2 * inset, 1, "293d3e")
for x, r in [(67, 5), (87, 3), (147, 3), (167, 5)]:
    disc(x, 114, r, "0d2224")
    disc(x, 113, r - 1, "516762")
for x in [19, 215]:
    for y in [12, 116]:
        disc(x, y, 3, "203337")
        disc(x, y, 2, "c5c9b1")
        box(x-1, y, 3, 1, "425453")

out = Path(__file__).resolve().parents[1] / "assets" / "cassette.l565"
out.write_bytes(pack("<4sHHH", b"L565", 1, W, H) + pack(f"<{W*H}H", *pixels))
