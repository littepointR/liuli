"""Program the FK743 internal application sectors with the bundled STM32Cube HAL writer."""
from pathlib import Path
import struct
import subprocess
import sys
import time
import zlib

from probe import attach, new_link

CODE = 0x24000000
PAYLOAD = 0x24010000
CONTROL = 0x2407F000
SECTOR = 128 * 1024
MAGIC = 0x464B3734


def transfer(link, address, data):
    from pylink.errors import JLinkException
    for offset in range(0, len(data), 512):
        block = data[offset:offset + 512]
        values = list(struct.unpack(f"<{len(block)//4}I", block))
        for attempt in range(4):
            try:
                if attempt and link.memory_read32(address + offset, len(values)) == values:
                    break
                link.memory_write32(address + offset, values)
                if link.memory_read32(address + offset, len(values)) == values:
                    break
            except JLinkException:
                if attempt == 3:
                    raise
            time.sleep(.025)
        else:
            raise RuntimeError(f"RAM readback mismatch at {address+offset:#x}")


def request(link, registers, helper, command, offset, data):
    stack, entry = struct.unpack_from("<II", helper)
    if not (stack == 0x20020000 and entry & 1 and CODE <= entry-1 < CODE+len(helper) <= PAYLOAD):
        raise ValueError("invalid RAM writer vectors")
    link.reset(halt=True)
    transfer(link, CODE, helper)
    if command == 1:
        # Internal flash words are 32 bytes. Never touch bytes in the next sector.
        transfer(link, PAYLOAD, data + b"\xff" * (-len(data) % 32))
    header = [MAGIC, command, offset, len(data), zlib.crc32(data)]
    transfer(link, CONTROL, struct.pack("<8I", *header, 0, 0, 0))
    transfer(link, 0xe000ed08, struct.pack("<I", CODE))
    link.register_write(registers["MSP"], stack)
    link.register_write(registers["R15 (PC)"], entry-1)
    if link.register_read(registers["R15 (PC)"]) != entry-1:
        raise RuntimeError("RAM writer program counter readback mismatch")
    link.restart()
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        time.sleep(.1)
        state = link.memory_read32(CONTROL, 8)
        if state[:5] != header or state[5] not in (2, 3) or link.memory_read32(CONTROL, 8) != state:
            continue
        if command == 2 and state[5] == 3 and state[7] == 7:
            return False
        if state[5:] != [2, header[4], 0]:
            raise RuntimeError(f"RAM writer failed at {offset:#x}, error {state[7]:#x}, CRC32 {state[6]:08x}")
        return True
    raise RuntimeError(f"no confirmed RAM writer result at {offset:#x}")


def install(commander, image, uid):
    try:
        import pylink
    except ImportError as error:
        raise RuntimeError("install pylink-square 2.1.0 to use --ram-writer") from error
    board = Path(__file__).resolve().parent
    binary = board / "firmware/flash-writer.bin"
    if not binary.is_file():
        subprocess.run([sys.executable, str(board / "st/compile.py"), str(commander.temp), "--writer"], check=True)
        binary = commander.temp / "flash-writer.bin"
    helper = binary.read_bytes()
    helper += b"\x00" * (-len(helper) % 4)
    link = new_link(commander)

    def connect(speed):
        attach(commander, link, uid, speed)
        link.reset(halt=True)
        return {link.register_name(i).upper(): i for i in link.register_list()}

    try:
        registers = connect(commander.speed)
        for offset in range(0, len(image), SECTOR):
            data = image[offset:offset+SECTOR]
            for attempt in range(3):
                try:
                    if not request(link, registers, helper, 2, offset, data):
                        request(link, registers, helper, 1, offset, data)
                    break
                except (pylink.errors.JLinkException, RuntimeError) as error:
                    if attempt == 2:
                        raise
                    print(f"Rechecking sector {offset//SECTOR} after {error}", flush=True)
                    link.close()
                    time.sleep(3)
                    registers = connect(min(commander.speed, 100))
            print(f"Sector {offset//SECTOR} verified, {len(data)} bytes", flush=True)
        if not request(link, registers, helper, 2, 0, image):
            raise RuntimeError("complete internal-flash CRC mismatch")
        print(f"Flash verified: {len(image)} bytes, CRC32 {zlib.crc32(image):08x}", flush=True)
        link.reset(halt=False)
    finally:
        link.close()
