"""Build the Waveshare firmware using an exported ESP-IDF environment and the esp Rust toolchain."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

BOARD = Path(__file__).resolve().parent
TARGET = "xtensa-esp32s3-none-elf"
PLANNER = BOARD.parents[1] / "tools/build-ui.py"
CAPACITY_APP = BOARD.parents[1] / "apps/liuli-alarms/Cargo.toml"

def main():
    parser = argparse.ArgumentParser()
    profiles = ("basic", "enhanced", "benchmark", "sleep-benchmark", "widget-demo")
    parser.add_argument("--profile", choices=profiles, default="basic")
    parser.add_argument("--out", type=Path, default=BOARD.parents[1] / "target" / "s3")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--rust-only", action="store_true")
    parser.add_argument("--effects-benchmark",action="store_true",help="exercise the widget demo with a fixed clock and report on-board timings")
    parser.add_argument("--trace-input", action="store_true", help="stream accepted inputs and clock advances over the debug serial port")
    parser.add_argument("--diagnostics", action="store_true", help="collect frame, bus, and heap metrics")
    parser.add_argument("--touch-irq", type=int, metavar="GPIO", help="wired FT3168 interrupt input")
    parser.add_argument("--touch-poll-ms", type=int, default=10)
    args = parser.parse_args()
    if not 1 <= args.touch_poll_ms <= 1000 or args.touch_irq is not None and not 0 <= args.touch_irq <= 48:
        parser.error("touch poll interval must be 1..1000 ms and GPIO must be 0..48")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    components = "core" if args.profile == "basic" else "core,alloc"
    command = ["cargo", "+esp", "build", "-Zbuild-std=" + components, "--locked",
               "--target", TARGET, "--release", "--ignore-rust-version", "--target-dir", str(out / "rust")]
    features = [] if args.profile == "basic" else [args.profile]
    diagnostics = args.diagnostics or args.effects_benchmark or args.profile in ("benchmark", "sleep-benchmark")
    if args.diagnostics:
        features.append("diagnostics")
    if args.effects_benchmark:
        if args.profile != "widget-demo":
            parser.error("--effects-benchmark requires --profile widget-demo")
        features.append("effects-benchmark")
    if args.trace_input:
        features.append("input-trace")
    if features:
        command.extend(["--features", ",".join(features)])
    if args.offline:
        command.append("--offline")
    env = os.environ.copy()
    env.setdefault('LIULI_METRICS', '1' if diagnostics else '0')
    env.setdefault('LIULI_DMA', '0')
    app = CAPACITY_APP
    planner_args = []
    if args.profile == "widget-demo":
        app = BOARD.parents[1] / "apps/liuli-widget-demo/Cargo.toml"
        view = "studio" if args.effects_benchmark else "showcase"
        planner_args = ["--app-features", "enhanced", "--plan-arg", view,
                        "--plan-arg", "240", "--plan-arg", "536"]
    plan = subprocess.check_output([sys.executable, str(PLANNER), "--app", str(app),
                                    "--plan-only", *planner_args,
                                    *(["--offline"] if args.offline else [])], text=True)
    _, nodes, depth, slots = plan.split()
    env.update(LIULI_PLANNED="1", LIULI_MAX_NODES=nodes, LIULI_MAX_DEPTH=depth, LIULI_TEXT_SLOTS_LIMIT=slots)
    print(f"UI storage: {nodes} nodes, {depth} levels, {slots} text slots", flush=True)
    subprocess.run(command, cwd=BOARD/"rust", env=env, check=True)
    if args.rust_only:
        return
    idf = Path(os.environ["IDF_PATH"]) / "tools/idf.py"
    build = out / "idf"
    library = out / "rust" / TARGET / "release/libliuli_s3_runtime.a"
    subprocess.run([sys.executable, str(idf), "-C", str(BOARD), "-B", str(build),
                    "-DSDKCONFIG=" + (build/"sdkconfig").as_posix(),
                    "-DLIULI_DIAGNOSTICS=" + ("ON" if diagnostics else "OFF"),
                    f"-DLIULI_TOUCH_IRQ={args.touch_irq if args.touch_irq is not None else -1}",
                    f"-DLIULI_TOUCH_POLL_MS={args.touch_poll_ms}",
                    "-DLIULI_RUNTIME=" + library.as_posix(), "build"], check=True)

if __name__ == "__main__":
    main()
