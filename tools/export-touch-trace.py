"""Extract one complete boot's Liuli trace from a serial log or a live port."""
import argparse
import gzip
import os
from pathlib import Path
import re
from tempfile import NamedTemporaryFile
import time


def export(lines, output):
    expected = 0
    last_ms = 0
    contacts = 0
    pattern = re.compile(r"(\d+) (tick|kind=(down|move|up)&v=(\d+)&x=-?\d+&y=-?\d+|kind=cancel&v=(\d+))")
    with NamedTemporaryFile("w", dir=output.parent, encoding="ascii", newline="\n",
                            delete=False) as stream:
        temporary = Path(stream.name)
        try:
            for line in lines:
                # miniterm on Windows may add a second CR before the newline.
                if not line.startswith(b"LIULI_TRACE "):
                    continue
                if not line.endswith(b"\n"):
                    raise ValueError("truncated trace record")
                _, seq, body = line.rstrip(b"\r\n").decode("ascii").split(" ", 2)
                if int(seq) != expected:
                    raise ValueError(f"missing record or new boot: expected {expected}, got {seq}")
                if expected == 0:
                    if body != "liuli-trace 1":
                        raise ValueError("missing boot header; start capture before reset")
                else:
                    match = pattern.fullmatch(body)
                    if not match or int(match[1]) < last_ms:
                        raise ValueError(f"invalid timestamp or pointer record at {seq}")
                    last_ms = int(match[1])
                    if match[2] != "tick":
                        if int(match[4] or match[5]) > 255:
                            raise ValueError(f"invalid pointer id at {seq}")
                        contacts += 1
                stream.write(body + "\n")
                expected += 1
            if not contacts:
                raise ValueError("no touch events; capture from boot and interact with the screen")
            stream.close()
            os.replace(temporary, output)
        finally:
            if not stream.closed:
                stream.close()
            temporary.unlink(missing_ok=True)
    return expected - 1, contacts


def serial_lines(port, seconds):
    import serial
    # No control-line reset on open. Start the program, then reset the board.
    device = serial.Serial(port=None, baudrate=115200, timeout=0.2)
    device.port = port
    device.dtr = False
    device.rts = False
    with device:
        print("Listening. Reset the board now and perform the gesture.", flush=True)
        end = time.monotonic() + seconds
        pending = bytearray()
        while time.monotonic() < end:
            pending.extend(device.read(device.in_waiting or 1))
            while (newline := pending.find(b"\n")) >= 0:
                yield bytes(pending[:newline + 1])
                del pending[:newline + 1]
        # The local timer can expire in the middle of a line. Finish that line.
        if pending:
            yield bytes(pending) + device.readline()


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    source = cli.add_mutually_exclusive_group(required=True)
    source.add_argument("--log", type=Path)
    source.add_argument("--port", help="115200 baud, requires pyserial")
    cli.add_argument("--seconds", type=int, default=30)
    cli.add_argument("--out", type=Path, required=True)
    args = cli.parse_args()
    if args.seconds < 1:
        cli.error("--seconds must be positive")
    try:
        if args.log:
            open_log = gzip.open if args.log.suffix == ".gz" else open
            with open_log(args.log, "rb") as lines:
                count, contacts = export(lines, args.out)
        else:
            count, contacts = export(serial_lines(args.port, args.seconds), args.out)
    except (OSError, ValueError, ImportError) as error:
        cli.exit(1, f"{error}\n")
    print(f"{count} records, {contacts} pointer events -> {args.out}")
