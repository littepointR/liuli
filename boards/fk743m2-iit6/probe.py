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
    from pylink.errors import JLinkReadException
    result = bytearray()
    for offset in range(0, size, 1024):
        count = min(1024, size-offset) // 4
        addr = 0xc0180000 + offset
        for attempt in range(4):
            try:
                data = link.memory_read32(addr, count)
            except JLinkReadException:
                if attempt == 3:
                    raise
                time.sleep(.025 * (attempt + 1))
                continue
            if len(data) == count and link.memory_read32(addr, count) == data:
                result.extend(struct.pack(f"<{count}I", *data))
                break
        else:
            raise RuntimeError(f"unstable snapshot data at {addr:#x}")
    return bytes(result)


def depth_scroll(commander, uid, trace, demo):
    """Check the fixed 800x480 out-and-back drag against the desktop renderer."""
    import statistics
    import subprocess
    from urllib.parse import parse_qsl
    from jlink import FRONTS, compare_pixels, expand_snapshot, frame_runs

    lines = trace.read_text().splitlines()
    if lines[:1] != ["liuli-trace 1"]:
        raise ValueError("expected the h743-depth-scroll.trace fixture")
    events, references = [], {}
    for i, line in enumerate(lines[1:], 1):
        _, query = line.split(" ", 1)
        fields = dict(parse_qsl(query, strict_parsing=True))
        kind, x, y = fields["kind"], int(fields["x"]), int(fields["y"])
        events.append((kind, x, y))
        if kind == "move" and y in (72, 360, 432):
            prefix = commander.temp / "scroll.trace"
            frame = commander.temp / "scroll.rgb565"
            prefix.write_text("\n".join(lines[:i+1]) + "\n", encoding="ascii")
            subprocess.run([str(demo.resolve()), "--studio", "--size", "800x480", "--replay", str(prefix),
                            "--scanout", str(frame)], check=True)
            references[i] = frame.read_bytes()
    visible, hidden = [], []
    link = new_link(commander)
    try:
        attach(commander, link, uid)
        link.reset(halt=True)
        link.memory_write32(0x24000000, [0])
        link.restart()
        last_frame = wait_for_ack(link, 0)[2]
        # Clear sticky FIFO and transfer errors once. Leave the reload flag to the BSP.
        link.memory_write32(0x5000103c, [6])
        for i, (kind, x, y) in enumerate(events, 1):
            action = {"down": 1, "up": 2, "move": 3}[kind]
            state = issue(link, 2*i, action, x | y << 16)
            if kind == "move":
                if state[8] != 9 or state[2] <= last_frame:
                    raise RuntimeError(f"move {i} at y={y} did not submit a Depth frame")
                if y >= 384:
                    visible.append(state[5])
                elif y <= 240:
                    hidden.append(state[5])
            last_frame = state[2]
            if i in references:
                snapshot = issue(link, 2*i+1, 6)
                expected = references[i]
                front = link.memory_read32(0x500010ac, 1)[0]
                if front not in FRONTS or front != snapshot[3] or snapshot[14] != frame_runs(expected):
                    raise ValueError(f"unexpected scanout at event {i}, y={y}")
                compare_pixels(f"event {i}, y={y}", expand_snapshot(read_snapshot(link, snapshot[14])), expected)
        flags = link.memory_read32(0x50001038, 1)[0]
        if issue(link, 2*len(events)+2, 5, 757 | 30 << 16)[8] != 0:
            raise RuntimeError("failed to return to the catalog")
        if len(references) != 4 or not visible or not hidden:
            raise ValueError("expected the complete H743 depth-scroll fixture")
        top, away = statistics.median(visible), statistics.median(hidden)
        print(f"near-top median {top:g} us; hidden median {away:g} us; LTDC ISR {flags:#x}")
        # 1081 x 543 total clocks at 33 MHz is one 17.8 ms panel refresh.
        if flags & 6 or top - away > 18000:
            raise RuntimeError("LTDC scanout error or top graphic costs more than one additional refresh")
    finally:
        link.close()


def showcase_trace(commander, uid, trace, reference, page, label, observe=None):
    """Replay raw touch through the board mailbox and compare the complete settled content pane."""
    from urllib.parse import parse_qsl
    from jlink import expand_snapshot, compare_pixels
    rows = trace.read_text(encoding="ascii").splitlines()
    if rows[:1] != ["liuli-trace 1"]:
        raise ValueError("expected a liuli-trace 1 input")
    expected = reference.read_bytes()
    if len(expected) != 800*480*2:
        raise ValueError("expected an 800x480 RGB565 frame")
    link = new_link(commander)
    try:
        attach(commander, link, uid)
        link.reset(halt=True)
        link.memory_write32(0x24000000, [0])
        link.restart()
        wait_for_ack(link, 0)
        started = time.monotonic()
        for seq, line in enumerate(rows[1:], 1):
            millis, query = line.split(" ", 1)
            time.sleep(max(0, started + int(millis)/1000 - time.monotonic()))
            kind = "tick"
            if query != "tick":
                event = dict(parse_qsl(query, strict_parsing=True))
                kind = event["kind"]
                action = {"down":1, "up":2, "move":3, "cancel":4}[kind]
                issue(link, seq, action, int(event["x"]) | int(event["y"]) << 16)
            if observe is not None:
                observe(link, kind)
        snap = issue(link, len(rows)+1, 6)
        if snap[8] != page or snap[9] & 6 or not 0 < snap[14] <= 512*1024:
            raise RuntimeError(f"invalid {label} page or scanout: {snap}")
        frame = expand_snapshot(read_snapshot(link, snap[14]))
        # Only the footer contains board-clock telemetry. Include the complete tab bar and title.
        pane = slice(0,430*800*2)
        compare_pixels(label, frame[pane], expected[pane])
        print(f"{label} matches the desktop replay (344000 RGB565 pixels)")
        link.reset(halt=False)
    finally:
        link.close()


def strip_fling(commander, uid, trace, reference):
    """Track the leading edge of the purple card on the published layout-page trajectory."""
    from jlink import FRONTS
    dragging = False
    released = False
    edges = []

    def observe(link, kind):
        nonlocal dragging, released
        if kind == "move":
            dragging = True
        elif dragging and kind == "up":
            released = True
        if released:
            front, = link.memory_read32(0x2400000c, 1)
            if front not in FRONTS:
                raise RuntimeError(f"invalid scanout address {front:#x}")
            row = link.memory_read16(front + 2*(274*800 + 70), 660)
            color = (116 >> 3) << 11 | (76 >> 2) << 5 | (176 >> 3)
            edges.append(70 + row.index(color))

    showcase_trace(commander, uid, trace, reference, 3, "Layout fling", observe)
    if len(set(edges)) < 3 or edges != sorted(edges, reverse=True):
        raise RuntimeError(f"card did not continue scrolling left after release: {edges}")
    print(f"Card edge after release (pixels): {edges}")


def motion_gallery(commander, uid, reference):
    from jlink import expand_snapshot

    from urllib.parse import parse_qs
    root = Path(__file__).resolve().parents[2]
    apps = root / "apps" if (root / "apps").is_dir() else root
    trace = apps / "liuli-widget-demo/tests/fixtures/h743-motion.trace"
    taps = []
    for line in trace.read_text(encoding="ascii").splitlines()[1:]:
        fields = parse_qs(line.split(" ", 1)[1])
        if fields.get("kind") == ["down"]:
            taps.append(int(fields["x"][0]) | int(fields["y"][0]) << 16)
    select_layout, select_flip, replay = taps

    def stage(frame):
        if len(frame) != 800 * 480 * 2:
            raise ValueError("expected an 800x480 RGB565 scanout")
        return frame[:430 * 800 * 2]

    expected = stage(reference.read_bytes())
    link = new_link(commander)
    try:
        attach(commander, link, uid)
        link.reset(halt=True)
        link.memory_write32(0x24000000, [0])
        link.restart()
        wait_for_ack(link, 0)

        def snapshot(seq):
            state = issue(link, seq, 6)
            if state[8] != 3 or not 0 < state[14] <= 512*1024 or state[9] & 6:
                raise RuntimeError(f"unexpected showcase page, snapshot size or LTDC status: {state}")
            return stage(expand_snapshot(read_snapshot(link, state[14])))

        issue(link, 1, 5, select_layout)
        time.sleep(.35)
        issue(link, 2, 5, select_flip)
        time.sleep(.85)
        if snapshot(3) != expected:
            raise ValueError("settled Flip preset differs from the desktop trace")
        issue(link, 4, 5, replay)
        time.sleep(.12)
        moving = snapshot(5)
        time.sleep(.85)
        settled = snapshot(6)
        changed = sum(moving[i:i+2] != settled[i:i+2] for i in range(0, len(moving), 2))
        if not changed or settled != expected:
            raise ValueError(f"Flip replay: {changed} moving pixels, endpoint matches={settled == expected}")
        print(f"Flip replay: {changed} moving pixels; all {len(expected)//2} settled stage pixels match desktop")
    finally:
        link.close()


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
