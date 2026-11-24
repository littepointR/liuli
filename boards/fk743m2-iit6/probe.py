"""SEGGER DLL access for paced SWD reads and the optional internal flash writer."""
import ctypes
from pathlib import Path
import platform
import struct
import time


def new_link(commander):
    try:
        import pylink
    except ImportError as error:
        raise RuntimeError("this transport requires pylink-square 2.1.0") from error
    name = {"Windows": "JLink_x64.dll" if ctypes.sizeof(ctypes.c_void_p) == 8 else "JLinkARM.dll",
            "Linux": "libjlinkarm.so", "Darwin": "libjlinkarm.dylib"}[platform.system()]
    path = Path(commander.exe).resolve().with_name(name)
    dll = pylink.library.Library(dllpath=str(path)) if path.is_file() else pylink.library.Library()
    return pylink.JLink(lib=dll)


def attach(commander, link, uid, speed=None):
    import pylink
    for attempt in range(6):
        try:
            link.open(serial_no=int(commander.serial))
            link.set_tif(pylink.enums.JLinkInterfaces.SWD)
            link.connect("STM32H743II", speed=commander.speed if speed is None else speed)
            if (link.memory_read32(0x1ff1e800, 3) == uid and link.memory_read32(0x5c001000, 1)[0] & 0xfff == 0x450
                    and link.memory_read16(0x1ff1e880, 1) == [2048]):
                return
            raise ValueError("target identity changed on the persistent SWD connection")
        except (ValueError, pylink.errors.JLinkException):
            link.close()
            if attempt == 5:
                raise
            time.sleep(1)


def wait_for_ack(link, seq):
    deadline = time.monotonic() + 8
    state = []
    while time.monotonic() < deadline:
        state = link.memory_read32(0x24000000, 16)
        if len(state) == 16 and state[0] == 0x4c554c49:
            if state[1] & 0xffff0000 == 0xbad00000:
                raise RuntimeError(f"board fault {state[1]:#x}, CFSR {state[15]:#x}")
            if state[1] == 4 and state[2] and state[13] == seq:
                return state
        time.sleep(.025)
    raise RuntimeError(f"no acknowledgement for sequence {seq}: {state}")


def issue(link, seq, action, point=None):
    # Publish data before the sequence. Reads and writes are ordered by the SWD connection.
    fields = [(0x2400002c, action)]
    if point is not None:
        fields.append((0x24000030, point))
    fields.append((0x24000028, seq))
    for addr, value in fields:
        for _ in range(4):
            link.memory_write32(addr, [value])
            if link.memory_read32(addr, 1) == [value]:
                break
        else:
            raise RuntimeError(f"mailbox readback mismatch for {addr:#x}")
    return wait_for_ack(link, seq)


def read_snapshot(link, size):
    result = bytearray()
    for offset in range(0, size, 1024):
        count = min(1024, size-offset) // 4
        addr = 0xc0180000 + offset
        for _ in range(4):
            data = link.memory_read32(addr, count)
            if len(data) == count and link.memory_read32(addr, count) == data:
                result.extend(struct.pack(f"<{count}I", *data))
                break
        else:
            raise RuntimeError(f"unstable snapshot data at {addr:#x}")
    return bytes(result)


def capture(commander, uid, checkpoints, lengths):
    from jlink import FRONTS, compare_pixels, expand_snapshot
    link = new_link(commander)
    reports = []
    try:
        attach(commander, link, uid)
        # A warm reset retains the NOLOAD report. Invalidate the signature while
        # halted so the host cannot publish input against the previous boot.
        link.reset(halt=True)
        link.memory_write32(0x24000000, [0])
        if link.memory_read32(0x24000000, 1) != [0]:
            raise RuntimeError("could not invalidate the previous boot report")
        link.restart()
        wait_for_ack(link, 0)
        for i, ((x, y, page, expected), size) in enumerate(zip(checkpoints, lengths)):
            if i:
                issue(link, 2*i, 5, (y << 16) | x)
            state = issue(link, 2*i+1, 6)
            gcr, layer, front = (link.memory_read32(addr, 1)[0] for addr in (0x50001018, 0x50001084, 0x500010ac))
            if state[8] != page or state[3] != front or front not in FRONTS or not (gcr & 1 and layer & 1):
                raise RuntimeError(f"{expected.name}: unexpected board state {state}, active front={front:#x}")
            if state[14] != size:
                raise ValueError(f"{expected.name}: expected {size} snapshot bytes, board reported {state[14]}")
            data = read_snapshot(link, size)
            (commander.temp / f"snapshot-{i}.bin").write_bytes(data)
            compare_pixels(expected.name, expand_snapshot(data), expected.read_bytes())
            reports.append(dict(state=state, front=front, gcr=gcr, layer=layer))
        print(f"Goodix ID {struct.pack('<I', state[6])!r}, I2C errors {state[7]}, LTDC ISR {state[9]:#x}")
        return reports
    finally:
        link.close()
