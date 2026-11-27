"""Identify, install and pixel-check the FK743M2 through SEGGER J-Link Commander."""
import argparse
import itertools
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import time

from build import FLASH, FLASH_SIZE

REPORT = 0x24000000
FRONTS = {0xc0000000, 0xc00c0000}
FRAME_BYTES = 800 * 480 * 2


class Commander:
    def __init__(self, exe, speed, temp):
        self.exe, self.speed, self.temp = exe, speed, temp
        self.serial = None

    def commands(self, commands, connect=True, transfer_bytes=0, speed=None):
        speed = self.speed if speed is None else speed
        script = self.temp / "commands.jlink"
        script.write_text("\n".join([*commands, "exit", ""]), encoding="ascii")
        cmd = [self.exe, "-NoGui", "1", "-ExitOnError", "1"]
        if connect:
            cmd += ["-USB", self.serial, "-device", "STM32H743II", "-if", "SWD",
                    "-speed", str(speed), "-autoconnect", "1"]
        cmd += ["-CommanderScript", script.name]
        # SWD framing, turnaround and flash programming add to the payload wire time.
        timeout = 120 + 64 * transfer_bytes / (speed * 1000)
        res = subprocess.run(cmd, cwd=self.temp, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             timeout=timeout, text=True, errors="replace")
        if res.returncode or re.search(r"\*\*\*\*\* Error|^Error:|FAILED|Cannot connect|Verification failed|Could not read memory",res.stdout,re.I|re.M):
            raise RuntimeError(res.stdout[-2400:])
        return res.stdout

    def identify(self, requested):
        for attempt in range(6):
            listing = self.commands(["ShowEmuList USB"], connect=False)
            probes = set(re.findall(r"(?:S/N|Serial number)\s*[:=]\s*(\d+)", listing, re.I))
            if (requested in probes if requested else bool(probes)) or attempt == 5:
                break
            time.sleep(1)
        if requested:
            if requested not in probes:
                raise RuntimeError("requested J-Link USB serial was not enumerated")
            self.serial = requested
        elif len(probes) == 1:
            self.serial, = probes
        elif not probes:
            raise RuntimeError("no SEGGER USB probe enumerated (ShowEmuList USB)")
        else:
            raise RuntimeError("multiple probes present; specify --serial")
        log = self.commands(["mem32 0x5C001000,1", "mem16 0x1FF1E880,1", "mem32 0x1FF1E800,3"],
                            speed=min(self.speed, 100))
        device = words(log, 0x5C001000, 1)[0]
        capacity = words(log, 0x1FF1E880, 1)[0]
        uid = words(log, 0x1FF1E800, 3)
        if device & 0xfff != 0x450 or capacity != 2048 or all(v in (0,0xffffffff) for v in uid):
            raise RuntimeError("unexpected MCU, flash size or UID")
        print(f"STM32H74x/75x ID={device:#010x}, {capacity} KiB internal flash, J-Link S/N {self.serial}")
        return uid

    def boot_report(self):
        # Keep the connection open while the program starts. Some probes reset
        # the MCU when reconnecting after a flash operation.
        marker = f"mem32 {REPORT:#x},10"
        log = self.commands(["r", "g", "sleep 3000", marker, "sleep 2000", marker],
                            speed=min(self.speed, 100))
        samples = log.split("J-Link>" + marker)[1:]
        if len(samples) != 2:
            raise RuntimeError("missing boot readback")
        state = report_words(samples[-1])
        if state[:2] != [0x4c554c49, 4] or not state[2] or state[3] not in FRONTS:
            raise RuntimeError(f"boot failed, stage {state[1]:#x}, CFSR {state[15]:#x}")
        return state


def words(log, addr, count=None):
    match = re.search(rf"^\s*(?:0x)?{addr:08x}\s*=\s*((?:[0-9a-f]{{4,8}}\b[ \t]*)+)",log,re.M|re.I)
    if not match:
        raise RuntimeError(f"missing readback for {addr:#x}")
    values = [int(v,16) for v in match[1].split()]
    if count and len(values)!=count:
        raise RuntimeError("short read from target")
    return values


def report_words(log):
    return [word for offset in range(0, 64, 16) for word in words(log, REPORT + offset, 4)]


def frame_runs(pixels):
    # Comparing adjacent native-endian words also works on big-endian hosts.
    total = 0
    for _, group in itertools.groupby(memoryview(pixels).cast("H")):
        length = sum(1 for _ in group)
        total += (length + 65534) // 65535
    return total * 4


def expand_snapshot(data):
    result = bytearray()
    for value, count in struct.iter_unpack("<HH", data):
        if not count or len(result) + 2 * count > FRAME_BYTES:
            raise ValueError("invalid run in SDRAM snapshot")
        result.extend(struct.pack("<H", value) * count)
    if len(result) != FRAME_BYTES:
        raise ValueError("truncated SDRAM snapshot")
    return result


def check_report(chunk, index, page, name):
    state = report_words(chunk)
    front = words(chunk, 0x500010ac, 1)[0]
    if (state[:2] != [0x4c554c49, 4] or not state[2] or state[13] != index
            or state[8] != page or state[3] != front or front not in FRONTS
            or not words(chunk, 0x50001018, 1)[0] & 1 or not words(chunk, 0x50001084, 1)[0] & 1):
        raise RuntimeError(f"{name}: bad board report {state}, LTDC front {front:#x}")
    return state


def compare_pixels(name, actual, expected):
    if actual != expected:
        pixel = next((i // 2 for i, (a, b) in enumerate(zip(actual, expected)) if a != b), len(actual) // 2)
        raise ValueError(f"{name}: pixel mismatch at ({pixel % 800},{pixel // 800})")
    print(f"{name} matches {len(expected)} bytes")


def compare_snapshots(c, checkpoints, lengths):
    # Acquire each image from the physical scanout after its tap has committed.
    # Only the transport is compressed. The final comparison covers every pixel.
    commands = ["r", "g", "sleep 3000"]
    marker = "mem32 0x24000000,10"
    for i, ((x, y, _, _), size) in enumerate(zip(checkpoints, lengths)):
        if i:
            commands.extend([f"w4 0x24000030,{(y << 16) | x:#x}", "w4 0x2400002c,5",
                             f"w4 0x24000028,{2*i:#x}", "sleep 1000"])
        commands.extend(["w4 0x2400002c,6", f"w4 0x24000028,{2*i+1:#x}", "sleep 320", marker,
                         "mem32 0x50001018,1", "mem32 0x50001084,1", "mem32 0x500010ac,1",
                         f"savebin snapshot-{i}.bin,0xc0180000,{size:#x}"])
    log = c.commands(commands, transfer_bytes=sum(lengths))
    reports = log.split("J-Link>" + marker)[1:]
    if len(reports) != len(checkpoints):
        raise RuntimeError("incomplete snapshot reports")
    for i, ((_, _, page, expected), size, chunk) in enumerate(zip(checkpoints, lengths, reports)):
        state = check_report(chunk, i*2+1, page, expected.name)
        if state[14] != size:
            raise ValueError(f"{expected.name}: board encoded {state[14]} bytes, expected {size}")
        data = (c.temp / f"snapshot-{i}.bin").read_bytes()
        if len(data) != size:
            raise RuntimeError(f"{expected.name}: short J-Link read")
        compare_pixels(expected.name, expand_snapshot(data), expected.read_bytes())
    print(f"Goodix ID {struct.pack('<I', state[6])!r}, I2C errors {state[7]}, LTDC ISR {state[9]:#x}")


def compare(c, directory, raw=False, probe_uid=None):
    checkpoints = []
    for index, line in enumerate((directory / "frames.txt").read_text().splitlines()):
        sx, sy, page, name = line.split()
        x, y = int(sx), int(sy)
        expected = directory / name
        if expected.stat().st_size != FRAME_BYTES:
            raise ValueError(f"{name}: expected a complete RGB565 framebuffer")
        if (index == 0 and (x, y) != (-1, -1)) or (index and not (0 <= x < 800 and 0 <= y < 480)):
            raise ValueError(f"{name}: invalid reference tap")
        checkpoints.append((x, y, int(page), expected))
    if not checkpoints:
        raise ValueError("empty reference run")

    if not raw:
        lengths = [frame_runs(expected.read_bytes()) for _, _, _, expected in checkpoints]
        if max(lengths) <= 512 * 1024:
            if probe_uid is not None:
                from probe import capture
                return capture(c, probe_uid, checkpoints, lengths)
            return compare_snapshots(c, checkpoints, lengths)
    # Reopening the SWD connection between a press and its acknowledgement can
    # reset a running target. Keep each replay on one Commander connection.
    # First discover the scanout addresses. On replay, confirm the address at
    # each checkpoint again before accepting its captured pixels.
    fronts = []
    marker = "mem32 0x24000000,10"
    for capture in (False, True):
        commands = ["r", "g", "sleep 3000"]
        for index, (x, y, _, _) in enumerate(checkpoints):
            if index:
                commands.extend([f"w4 0x24000030,{(y << 16) | x:#x}", "w4 0x2400002c,5",
                                 f"w4 0x24000028,{index:#x}", "sleep 900"])
            commands.extend([marker, "mem32 0x50001018,1", "mem32 0x50001084,1", "mem32 0x500010ac,1"])
            if capture:
                commands.append(f"savebin capture-{index}.bin,{fronts[index]:#x},{FRAME_BYTES:#x}")
        log = c.commands(commands, transfer_bytes=len(checkpoints) * FRAME_BYTES if capture else 0)
        reports = log.split("J-Link>" + marker)[1:]
        if len(reports) != len(checkpoints):
            raise RuntimeError("incomplete checkpoint reports")
        for index, (chunk, (_, _, page, expected)) in enumerate(zip(reports, checkpoints)):
            state = check_report(chunk, index, page, expected.name)
            front = state[3]
            if not capture:
                fronts.append(front)
                continue
            if fronts[index] != front:
                raise RuntimeError(f"{expected.name}: scanout address changed between replays")
            actual = (c.temp / f"capture-{index}.bin").read_bytes()
            compare_pixels(expected.name, actual, expected.read_bytes())
        if not capture:
            print(f"{len(fronts)} page and LTDC checkpoints passed; reading scanout", flush=True)
    print(f"Goodix ID {struct.pack('<I', state[6])!r}, I2C errors {state[7]}, LTDC ISR {state[9]:#x}")


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--jlink",default=shutil.which("JLinkExe") or shutil.which("JLink") or "JLink.exe")
    p.add_argument("--serial",help="SEGGER USB serial (autoselects a single connected probe)")
    p.add_argument("--speed",type=int,default=100,help="SWD speed in kHz")
    p.add_argument("--flash",type=Path,help="write and verify the Widget Demo internal-flash binary")
    p.add_argument("--ram-writer", action="store_true", help="use the bundled ST HAL flash writer via pylink-square")
    p.add_argument("--compare",type=Path,help="compare scanout with --panel-frames directory from a fresh boot")
    p.add_argument("--raw-scanout", action="store_true", help="read uncompressed framebuffer memory during --compare")
    p.add_argument("--probe-api", action="store_true", help="poll frame acknowledgements and double-read snapshot blocks with pylink-square")
    p.add_argument("--depth-scroll", nargs=2, type=Path, metavar=("TRACE", "DEMO"),
                   help="replay the enhanced H743 depth-scroll fixture and compare with the desktop executable")
    p.add_argument("--motion-gallery", type=Path, metavar="SCANOUT",
                   help="exercise the H743 showcase gallery against the desktop h743-motion.trace scanout")
    p.add_argument("--themed-motion", nargs=2, type=Path, metavar=("TRACE","SCANOUT"), help="replay the Motion style fixture and compare its full content pane")
    p.add_argument("--widget-samples", nargs=2, type=Path, metavar=("TRACE", "SCANOUT"),
                   help="replay the widget catalog trace and compare its content pane")
    p.add_argument("--startup-focus", type=Path, metavar="DESKTOP_DEMO",
                   help="compare the cold-start, navigation and touch header against the desktop demo")
    p.add_argument("--strip-fling", nargs=2, type=Path, metavar=("TRACE", "SCANOUT"),
                   help="replay h743-strip-fling.trace, sample the moving card after release and compare the settled pane")
    args=p.parse_args()
    if args.speed <= 0:
        p.error("--speed must be a positive SWD frequency in kHz")
    if args.ram_writer and not args.flash:
        p.error("--ram-writer requires --flash")
    with tempfile.TemporaryDirectory(prefix="liuli-fk743-") as scratch:
        c=Commander(args.jlink,args.speed,Path(scratch))
        uid = c.identify(args.serial)
        if args.flash:
            image=args.flash.read_bytes()
            if not 8<=len(image)<=FLASH_SIZE:
                raise ValueError("invalid internal flash image length")
            stack,pc=struct.unpack_from("<2I",image)
            if stack!=0x20020000 or not pc&1 or not FLASH<=pc-1<FLASH+len(image):
                raise ValueError("invalid Widget Demo reset vector")
            (c.temp/"widget.bin").write_bytes(image)
            if args.ram_writer:
                from ram_writer import install
                install(c, image, uid)
            else:
                c.commands(["r","h","loadbin widget.bin,0x08000000","verifybin widget.bin,0x08000000"],
                           transfer_bytes=2*len(image))
            v=c.boot_report()
            print(f"boot verified, front={v[3]:#x}, first/last update={v[4]} pixels, touch ID={v[6]:#x}")
        if args.compare:
            compare(c,args.compare,args.raw_scanout,uid if args.probe_api else None)
        if args.depth_scroll:
            from probe import depth_scroll
            depth_scroll(c, uid, *args.depth_scroll)
        if args.motion_gallery:
            from probe import motion_gallery
            motion_gallery(c, uid, args.motion_gallery)
        if args.themed_motion:
            from probe import showcase_trace
            showcase_trace(c, uid, *args.themed_motion, 4, "Motion")
        if args.widget_samples:
            from probe import showcase_trace
            showcase_trace(c, uid, *args.widget_samples, 5, "Widget samples")
        if args.startup_focus:
            from probe import startup_focus
            startup_focus(c, uid, args.startup_focus)
        if args.strip_fling:
            from probe import strip_fling
            strip_fling(c, uid, *args.strip_fling)


if __name__=="__main__":
    main()
