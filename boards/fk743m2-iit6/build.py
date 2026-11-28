"""Build an FK743M2-IIT6 Widget Demo from the checkout or the extracted offline SDK."""
import argparse
import os
from pathlib import Path
import shutil
import struct
import sys
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
    parser.add_argument("--diagnostics", action="store_true", help="enable the SWD input and screenshot mailbox")
    parser.add_argument("--touch-irq", action="store_true", help="wait for the GT911 INT edge on PG3")
    parser.add_argument("--touch-poll-ms", type=int, default=4)
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
    if not 1 <= args.touch_poll_ms <= 1000:
        parser.error('--touch-poll-ms must be 1..1000')
    features = [args.backend]
    if args.touch_irq:
        features.append('touch-irq')
    if args.diagnostics:
        features.append("diagnostics")
    if args.profile == "enhanced":
        features.append("enhanced")
    if args.showcase:
        features.append("showcase")
    cmd.extend(["--no-default-features", "--features", ",".join(features)])
    env = dict(os.environ, FK743_TOUCH_POLL_MS=str(args.touch_poll_ms))
    env.setdefault("LIULI_METRICS", "1" if args.diagnostics else "0")
    env.setdefault("LIULI_DMA", "0")
    root = BOARD.parents[1]
    planner = [sys.executable, str(root / "tools/build-ui.py"), "--plan-only",
               "--app", str(root / "apps/liuli-widget-demo/Cargo.toml"),
               "--plan-arg", "showcase" if args.showcase else "studio",
               "--plan-arg", "800", "--plan-arg", "480"]
    if args.profile == "enhanced":
        planner.extend(["--app-features", "enhanced"])
    if args.offline:
        planner.append("--offline")
    _, nodes, depth, slots = subprocess.check_output(planner, text=True).split()
    env.update(LIULI_PLANNED="1", LIULI_MAX_NODES=nodes, LIULI_MAX_DEPTH=depth, LIULI_TEXT_SLOTS_LIMIT=slots)
    if args.showcase:
        # Paint/formatting budgets are independent of the retained tree.
        for name, budget in [("MAX_COMMANDS",512),("MAX_DAMAGE",16),("MAX_TEXT_BYTES",2048),
                             ("MAX_ANIMATIONS",12),("MAX_SPARKLINES",4)]:
            env.setdefault("LIULI_" + name, str(budget))
    print(f"UI storage: {nodes} nodes, {depth} levels, {slots} text slots", flush=True)
    subprocess.run(cmd, cwd=BOARD, env=env, check=True)
    elf = build / TARGET / "release/liuli-fk743m2"
    image = binary(elf.read_bytes())
    shutil.copyfile(elf, out / "widget-demo.elf")
    (out / "widget-demo.bin").write_bytes(image)
    print(f"{out / 'widget-demo.bin'} ({len(image)} bytes at {FLASH:#010x})")


if __name__ == "__main__":
    main()
