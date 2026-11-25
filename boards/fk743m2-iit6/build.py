"""Build an FK743M2-IIT6 Widget Demo from the checkout or the extracted offline SDK."""
import argparse
from pathlib import Path
import shutil
import struct
import subprocess

BOARD = Path(__file__).resolve().parent
TARGET = "thumbv7em-none-eabihf"
FLASH = 0x08000000
FLASH_SIZE = 2 * 1024 * 1024


def binary(elf):
    if elf[:7] != b"\x7fELF\x01\x01\x01" or struct.unpack_from("<H", elf, 18)[0] != 40:
        raise ValueError("expected little-endian ELF32 for ARM")
    phoff = struct.unpack_from("<I", elf, 28)[0]
    step, count = struct.unpack_from("<HH", elf, 42)
    segments = []
    for i in range(count):
        kind, offset, _, address, size, _, _, _ = struct.unpack_from("<8I", elf, phoff + i * step)
        if kind != 1 or size == 0:
            continue
        if not FLASH <= address < address + size <= FLASH + FLASH_SIZE or offset + size > len(elf):
            raise ValueError("loadable data outside internal flash or truncated ELF")
        segments.append((address - FLASH, elf[offset:offset + size]))
    image = bytearray()
    for offset, data in sorted(segments):
        if offset < len(image):
            raise ValueError("overlapping load segments")
        image.extend(b"\xff" * (offset - len(image)))
        image.extend(data)
    stack, reset = struct.unpack_from("<II", image)
    if stack != 0x20020000 or reset & 1 != 1 or not FLASH <= reset - 1 < FLASH + len(image):
        raise ValueError("invalid reset vectors")
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["basic", "enhanced"], default="enhanced")
    parser.add_argument("--backend", choices=["st-hal", "embassy"], default="st-hal")
    ui = parser.add_mutually_exclusive_group()
    ui.add_argument("--showcase", dest="showcase", action="store_true", default=True)
    ui.add_argument("--studio", dest="showcase", action="store_false", help="build the Spool recorder example")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    build = out / "cargo"
    cmd = ["cargo", "build", "--locked", "--release", "--target", TARGET, "--target-dir", str(build)]
    if args.offline:
        cmd.append("--offline")
    features = [args.backend]
    if args.profile == "enhanced":
        features.append("enhanced")
    if args.showcase:
        features.append("showcase")
    cmd.extend(["--no-default-features", "--features", ",".join(features)])
    subprocess.run(cmd, cwd=BOARD, check=True)
    elf = build / TARGET / "release/liuli-fk743m2"
    image = binary(elf.read_bytes())
    shutil.copyfile(elf, out / "widget-demo.elf")
    (out / "widget-demo.bin").write_bytes(image)
    print(f"{out / 'widget-demo.bin'} ({len(image)} bytes at {FLASH:#010x})")


if __name__ == "__main__":
    main()
