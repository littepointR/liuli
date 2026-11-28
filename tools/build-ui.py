"""Build a product with storage sized by its own component declaration."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=Path("Cargo.toml"),
                        help="application manifest containing the capacity example")
    parser.add_argument("--firmware", type=Path, help="target manifest (defaults to --app)")
    parser.add_argument("--plan-example", help="host example publishing the scene plan")
    parser.add_argument("--plan-arg", action="append", default=[], help="argument to the application capacity example")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--locked", action="store_true")
    parser.add_argument("--action", choices=("build","run","test"), default="build", help="target Cargo action")
    parser.add_argument("--plan-only", action="store_true", help="emit the evaluated plan for a board toolchain")
    parser.add_argument("--app-features", help="application features for the host declaration")
    parser.add_argument("build_args", nargs=argparse.REMAINDER, help="target cargo build arguments after --")
    args = parser.parse_args()
    app = args.app.resolve()
    firmware = (args.firmware or app).resolve()
    common = [flag for flag, enabled in (("--offline", args.offline), ("--locked", args.locked)) if enabled]
    host = next(line.split()[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                if line.startswith("host:"))
    features = ["--features", args.app_features] if args.app_features else []
    target_args = args.build_args
    if target_args[:1] == ["--"]:
        target_args = target_args[1:]
    host_features = list(features)
    if firmware == app:
        cargo_flags = iter(target_args)
        for flag in cargo_flags:
            if flag == "--": break
            if flag in ("--features", "-F"):
                host_features.extend([flag, next(cargo_flags)])
            elif flag in ("--all-features","--no-default-features") or flag.startswith(("--features=", "-F")):
                host_features.append(flag)
    # Cargo uses the example name as the executable filename. Independent app plans
    # must not overwrite another running capacity.exe on Windows.
    with app.open("rb") as manifest:
        metadata = tomllib.load(manifest)["package"].get("metadata", {}).get("liuli", {})
    example = args.plan_example or metadata.get("capacity-example", "capacity")
    cache = Path(os.environ.get("CARGO_TARGET_DIR", app.parent / "target"))
    identity = hashlib.sha256(str((str(app), example, host_features)).encode()).hexdigest()[:12]
    plan_dir = cache.resolve() / "scene-plans" / identity
    probe = ["cargo", "run", "--quiet", "--target-dir", str(plan_dir), "--target", host, "--manifest-path", str(app),
             "--example", example, "--features", ("scene-planner" if args.plan_example else "liuli/scene-planner"), *host_features, *common, "--", *args.plan_arg]
    host_env = os.environ.copy()
    host_env["LIULI_PRINT_CAPACITY"] = "1"
    for name in ("PLANNED", "MAX_NODES", "MAX_DEPTH", "TEXT_SLOTS_LIMIT"):
        host_env.pop("LIULI_" + name, None)
    # Scratch space for analysing any supported tree, independent of the target.
    host_env.update(LIULI_PLANNED="1", LIULI_MAX_NODES="250", LIULI_MAX_DEPTH="250", LIULI_TEXT_SLOTS_LIMIT="250")
    output = subprocess.check_output(probe, cwd=app.parent, env=host_env, text=True).strip().split()
    if len(output) != 4 or output[0] != "liuli-capacity-v1":
        parser.error("capacity example must print liuli-capacity-v1 NODES DEPTH TEXT_SLOTS")
    nodes, depth, text_slots = map(int, output[1:])
    if not (1 <= depth <= nodes <= 250 and 0 <= text_slots <= nodes):
        parser.error(f"tree requires {nodes} nodes and {depth} levels (supported 1..250 nodes)")
    if args.plan_only:
        print(*output)
        return
    env = dict(os.environ, LIULI_MAX_NODES=str(nodes), LIULI_MAX_DEPTH=str(depth),
               LIULI_TEXT_SLOTS_LIMIT=str(text_slots), LIULI_PLANNED='1')
    print(f"liuli capacity: {nodes} nodes, {depth} levels, {text_slots} text slots", flush=True)
    subprocess.run(["cargo", args.action, "--manifest-path", str(firmware), *common,
                    *(features if firmware == app else []), *target_args],
                   cwd=Path.cwd() if args.action == "run" else firmware.parent, env=env, check=True)


if __name__ == "__main__":
    main()
