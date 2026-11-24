"""Build the Waveshare firmware using an exported ESP-IDF environment and the esp Rust toolchain."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

BOARD = Path(__file__).resolve().parent
TARGET = "xtensa-esp32s3-none-elf"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=("basic", "enhanced", "benchmark", "sleep-benchmark"), default="basic")
    parser.add_argument("--out", type=Path, default=BOARD.parents[1] / "target" / "s3")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--rust-only", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    components = "core" if args.profile == "basic" else "core,alloc"
    command = ["cargo", "+esp", "build", "-Zbuild-std=" + components, "--locked",
               "--target", TARGET, "--release", "--ignore-rust-version", "--target-dir", str(out / "rust")]
    if args.profile != "basic":
        command.extend(["--features", args.profile])
    if args.offline:
        command.append("--offline")
    subprocess.run(command, cwd=BOARD/"rust", check=True)
    if args.rust_only:
        return
    idf = Path(os.environ["IDF_PATH"]) / "tools/idf.py"
    build = out / "idf"
    library = out / "rust" / TARGET / "release/libliuli_s3_runtime.a"
    subprocess.run([sys.executable, str(idf), "-C", str(BOARD), "-B", str(build),
                    "-DSDKCONFIG=" + (build/"sdkconfig").as_posix(),
                    "-DLIULI_RUNTIME=" + library.as_posix(), "build"], check=True)

if __name__ == "__main__":
    main()
