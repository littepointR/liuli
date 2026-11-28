"""Compare linked Thumb images of the same public, single-label application."""
import os
import json
from pathlib import Path
import shutil
import struct
import subprocess

HERE = Path(__file__).resolve().parent
TARGET = "thumbv7em-none-eabihf"


def elf_report(path):
    data = path.read_bytes()
    ph, sh = struct.unpack_from("<II", data, 28)
    ps, pn, ss, sn, names = struct.unpack_from("<5H", data, 42)
    segments = [struct.unpack_from("<8I", data, ph + n*ps) for n in range(pn)]
    sections = [struct.unpack_from("<10I", data, sh + n*ss) for n in range(sn)]
    off, length = sections[names][4:6]
    strings = data[off:off+length]
    report = {"flash_span": max(s[3]+s[4] for s in segments
                               if s[0] == 1 and s[4] and 0x08000000 <= s[3] < 0x10000000) - 0x08000000}
    for name_off, _, _, _, off, length, *_ in sections:
        name = strings[name_off:strings.index(0, name_off)].decode()
        if name in {".text", ".rodata", ".data", ".bss"}:
            report[name] = length
            if name == ".data":
                report["nonzero_data_bytes"] = sum(bool(x) for x in data[off:off+length])
    return report



OUTPUT = HERE.parents[1]/"target"/"cost-qualification"
TRIM = {
    "MAX_NODES": 2, "MAX_DEPTH": 2, "TEXT_SLOTS_LIMIT": 1, "MAX_COMMANDS": 8,
    "MAX_DAMAGE": 1, "MAX_ANIMATIONS": 0, "MAX_EFFECTS": 0, "MAX_SPARKLINES": 0,
    "MAX_PART_STYLES": 0, "MAX_LAYOUT_EXTRAS": 0, "MAX_TEXT_BYTES": 0, "FONT_SLOTS": 1,
    "MASKS": 0, "ICONS": 0, "SHADOWS": 0, "TAB_INDICATOR": 0, "METRICS": 0, "DMA": 0,
    "POINTER": 0, "REPEAT": 0, "SCROLL": 0,
}


def build(name, features="", settings=None):
    env = {k: v for k,v in os.environ.items() if not k.startswith("LIULI_")}
    # Deliberately fix the reference arena for cost comparisons; production
    # firmware obtains these values from its application declaration.
    env.update(LIULI_PLANNED="1", LIULI_MAX_NODES="64", LIULI_MAX_DEPTH="12", LIULI_TEXT_SLOTS_LIMIT="64")
    env.update({"LIULI_"+k: str(v) for k,v in (settings or {}).items()})
    command = ["cargo", "build", "--offline", "--locked", "--release", "--target", TARGET,
               "--target-dir", str(OUTPUT), "--bin", "cost-probe"]
    if features:
        command += ["--features", features]
    subprocess.run(command, cwd=HERE, env=env, check=True)
    elf = OUTPUT/(name+".elf")
    shutil.copyfile(OUTPUT/TARGET/"release"/"cost-probe", elf)
    return elf_report(elf)


def check_trimmed_symbols(elf):
    nm = shutil.which("llvm-nm")
    if nm is None:
        sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
        host = next(line.split()[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                    if line.startswith("host:"))
        nm = sysroot / "lib/rustlib" / host / "bin" / ("llvm-nm.exe" if os.name == "nt" else "llvm-nm")
    symbols = subprocess.check_output([nm, "--demangle", str(elf)], text=True)
    for renderer in ("draw_shadow", "draw_sparkline", "draw_icon", "paint_mask_tile", "paint_sliding_indicator"):
        assert "::" + renderer not in symbols, f"disabled renderer remains in the linked image: {renderer}"


if __name__ == "__main__":
    report = {name: build(name, features) for name, features in [
        ("base", ""), ("images", "heap,owned"), ("layers", "heap,layers"), ("enhanced", "heap,enhanced")]}
    report["trimmed"] = build("trimmed", settings=TRIM)
    print(json.dumps(report, indent=2))
    (OUTPUT/"sections.json").write_text(json.dumps(report, indent=2))
    assert all(result[".data"] == 0 for result in report.values()), "static UI leaked into flash load image"
    assert report["trimmed"][".bss"] < report["base"][".bss"] // 4
    assert report["trimmed"]["flash_span"] < report["base"]["flash_span"]
    check_trimmed_symbols(OUTPUT/"trimmed.elf")
