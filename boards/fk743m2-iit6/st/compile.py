"""Compile the pinned STM32Cube and CMSIS subset for either H743 runtime."""
import os
from pathlib import Path
import subprocess
import sys
import zipfile

st = Path(__file__).resolve().parent
out = Path(sys.argv[1])
source = out / 'st-drivers'
with zipfile.ZipFile(st / 'stm32h743-drivers.zip') as archive:
    archive.extractall(source)
compiler = os.environ.get('FK743_CLANG', 'clang')
args = [compiler, '--target=arm-none-eabihf', '-mcpu=cortex-m7', '-mfpu=fpv5-d16',
        '-mfloat-abi=hard', '-mthumb', '-std=c11', '-O2', '-ffreestanding',
        '-ffunction-sections', '-fdata-sections', '-fno-unwind-tables',
        '-DSTM32H743xx', '-DUSE_HAL_DRIVER', '-DUSE_PWR_LDO_SUPPLY',
        '-I', str(st), '-I', str(st / 'freestanding'), '-I', str(source / 'inc')]
if 'CARGO_FEATURE_DIAGNOSTICS' in os.environ:
    args.append('-DLIULI_DIAGNOSTICS')
if 'CARGO_FEATURE_TOUCH_IRQ' in os.environ:
    args.append('-DLIULI_TOUCH_IRQ')
writer = '--writer' in sys.argv
flash_sources = [source / 'src' / name for name in ('stm32h7xx_hal_flash.c', 'stm32h7xx_hal_flash_ex.c')]
sources = [st / 'flash_writer.c', *flash_sources] if writer else [
    *sorted(set((source / 'src').glob('*.c')) - set(flash_sources)), st / 'board.c']
objects = []
for file in sources:
    obj = out / (file.stem + '.o')
    subprocess.run([*args, '-c', str(file), '-o', str(obj)], check=True)
    objects.append(str(obj))
    if not writer:
        print(f'cargo:rustc-link-arg={obj}')
if writer:
    # The RAM image contains its own vectors; both the host and the target validate its bounds.
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    host = next(line[6:] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                if line.startswith("host: "))
    linker = sysroot / "lib/rustlib" / host / "bin" / ("rust-lld.exe" if os.name == "nt" else "rust-lld")
    subprocess.run([str(linker), "-flavor", "gnu", "--gc-sections", "--oformat=binary",
                    "-T", str(st / "writer.x"), *objects, "-o", str(out / "flash-writer.bin")], check=True)
