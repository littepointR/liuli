"""Build an offline SDK from Cargo publication archives and verify its clean-room template."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]

def cargo(*args, cwd=ROOT, env=None):
    subprocess.run(["cargo", *args], cwd=cwd, env=env, check=True)

def unpack_crate(archive, destination, expected=None):
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if expected is not None and digest != expected:
        raise ValueError(f"checksum mismatch for {archive.name}")
    with tarfile.open(archive) as source:
        source.extractall(destination, filter="data")
    directory = destination / archive.stem
    files = {p.relative_to(directory).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
             for p in directory.rglob("*") if p.is_file()}
    (directory / ".cargo-checksum.json").write_text(json.dumps({"files": files, "package": digest}), encoding="utf8")

def bundle_xtensa_inputs(vendor, fetch):
    registry = Path(os.environ.get("CARGO_HOME", Path.home()/".cargo"))/"registry/cache"
    cache = ROOT/"target/sdk-sysroot-cache"
    cache.mkdir(exist_ok=True, parents=True)
    for pkg in json.loads((ROOT/"checks/xtensa-sysroot.json").read_text())["packages"]:
        name, version = pkg["name"], pkg["version"]
        directory = f"{name}-{version}"
        if (vendor/directory).is_dir():
            continue
        archive = next(registry.glob("*/"+directory+".crate"), cache/(directory+".crate"))
        if not archive.exists():
            if not fetch:
                raise RuntimeError(f"missing {archive.name}; prepare the SDK with --fetch-toolchain-inputs")
            url = f"https://static.crates.io/crates/{name}/{directory}.crate"
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read()
            if hashlib.sha256(data).hexdigest() != pkg["checksum"]:
                raise ValueError(f"checksum mismatch for {directory}")
            archive.write_bytes(data)
        unpack_crate(archive,vendor,pkg["checksum"])

def package(output, fetch=False):
    output.mkdir(exist_ok=True, parents=True)
    cargo("package", "--workspace", "--no-verify", "--allow-dirty", "--locked", "--offline")
    with tempfile.TemporaryDirectory(prefix="liuli-sdk-") as scratch:
        stage = Path(scratch) / "liuli-sdk"
        stage.mkdir()
        vendor = stage / "vendor"
        cargo("vendor", "--locked", "--offline", "--versioned-dirs", str(vendor))
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        for pkg in (p for p in lock["package"] if "source" not in p):
            name = f"{pkg['name']}-{pkg['version']}"
            archive = ROOT / "target/package" / (name + ".crate")
            unpack_crate(archive, vendor)
        bundle_xtensa_inputs(vendor,fetch)
        config = stage / ".cargo"
        config.mkdir()
        (config / "config.toml").write_text('[source.crates-io]\nreplace-with="sdk"\n[source.sdk]\ndirectory="vendor"\n')
        app = stage / "device-ui"
        shutil.copytree(ROOT / "templates/device-ui", app)
        shutil.copytree(ROOT / "apps/liuli-alarms/assets", app / "assets")
        shutil.copytree(ROOT / "docs", stage / "docs")
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        shutil.copyfile(ROOT / "README.md", stage / "README.md")
        shutil.copyfile(ROOT / "checks/package-firmware.py", stage / "package-firmware.py")
        board = stage / "boards/waveshare-s3-amoled"
        shutil.copytree(ROOT / "boards/waveshare-s3-amoled", board, ignore=shutil.ignore_patterns("target", "build", "sdkconfig"))
        shutil.copytree(ROOT / "crates/liuli-enhanced/tests/fonts", board / "bench-fonts")
        manifest = board / "rust/Cargo.toml"
        text = manifest.read_text()
        for crate, path in (("liuli", "../../../crates/liuli"), ("liuli-alarms", "../../../apps/liuli-alarms")):
            text = text.replace(f'path="{path}"', f'version="={VERSION}"')
        manifest.write_text(text)
        # The BSP lock in the checkout describes workspace path dependencies.
        (board / "rust/Cargo.lock").unlink()
        # The starter links the same application crate used by its desktop preview.
        starter_board = app / "board"
        shutil.copytree(board, starter_board, ignore=shutil.ignore_patterns("bench-fonts"))
        starter_tool = starter_board / "build.py"
        starter_tool.write_text(starter_tool.read_text().replace('("basic", "enhanced", "benchmark", "sleep-benchmark")','("basic", "enhanced")'))
        starter_manifest = starter_board / "rust/Cargo.toml"
        starter_manifest.write_text(starter_manifest.read_text().replace(f'liuli-alarms={{version="={VERSION}"}}', 'device-ui={path="../.."}'))
        (starter_board / "rust/src/application.rs").write_text(
            "pub use device_ui::Device as BoardApp;\npub fn model(_: u16) -> bool { false }\n"
        )
        product = stage / "liuli-alarms"
        shutil.copytree(vendor / f"liuli-alarms-{VERSION}", product)
        widgets = stage / "liuli-widgets"
        shutil.copytree(vendor / f"liuli-widgets-{VERSION}", widgets)
        for consumer in (product, widgets):
            with (consumer/"Cargo.toml").open("a") as manifest:
                manifest.write("\n[workspace]\n")
        # Exercise the packaged product and template without the workspace, registry cache or network.
        env = dict(os.environ, CARGO_HOME=str(Path(scratch)/"cargo-home"), CARGO_NET_OFFLINE="true",
                   CARGO_TARGET_DIR=str(Path(scratch)/"build"))
        cargo("run", "--offline", "--example", "preview", "--", "--headless", cwd=app, env=env)
        cargo("test", "--offline", "--tests", "--release", "--all-features", cwd=product, env=env)
        cargo("run", "--offline", "--example", "editor", "--features", "enhanced", "--",
              str(board/"bench-fonts/Arabic.ttf"), str(board/"bench-fonts/Devanagari.ttf"), "--headless", cwd=product, env=env)
        cargo("generate-lockfile", "--offline", cwd=starter_board/"rust", env=env)
        cargo("run", "--offline", "--example", "catalog", "--", str(app/"assets/body.lf4"), "--headless", cwd=widgets, env=env)
        for name, binaries in (("liuli-fontpack",["--bin","liuli-fontpack"]),("liuli-sim",["--bins"])):
            cargo("install","--offline","--root",str(stage/"tools"),"--path",str(vendor/f"{name}-{VERSION}"),
                  *binaries,cwd=stage,env=env)
        (stage/"tools/.crates2.json").unlink(missing_ok=True)
        (stage/"tools/.crates.toml").unlink(missing_ok=True)
        exe_suffix = ".exe" if os.name == "nt" else ""
        subprocess.run([str(stage/"tools"/"bin"/("liuli-fontpack"+exe_suffix)),
                        "verify","--project",str(app/"assets/fonts.toml")],cwd=stage,env=env,check=True)
        packaged_sim = vendor / f"liuli-sim-{VERSION}"
        replay_output = Path(scratch) / "migrated-app-screenshots"
        for scenario in ("settings", "terminal"):
            subprocess.run([str(stage/"tools/bin"/("liuli-sim"+exe_suffix)), "replay",
                            str(packaged_sim/f"tests/fixtures/{scenario}.events"), "--format", "ppm",
                            "--out", str(replay_output)], cwd=stage, env=env, check=True)
        expected = sorted((packaged_sim/"tests/golden").glob("*.ppm"))
        if not expected or any((replay_output/p.name).read_bytes()!=p.read_bytes() for p in expected):
            raise ValueError("packaged v0.2 sample diverged from the reference screenshots")
        digits = Path(scratch)/"numeric-font"
        digits.mkdir()
        shutil.copyfile(app/"assets/example.ttf",digits/"source.ttf")
        (digits/"digits.txt").write_text("0123456789",encoding="utf8")
        (digits/"fonts.toml").write_text(
            'budget=4096\n[[font]]\nsource="source.ttf"\ncharset="digits.txt"\n'
            'output="digits.lf4"\npx=19\nfallback="0"\n',encoding="utf8")
        fontpack = str(stage/"tools/bin"/("liuli-fontpack"+exe_suffix))
        for op in ("pack","verify"):
            subprocess.run([fontpack,op,"--project","fonts.toml"],cwd=digits,env=env,check=True)
        (digits/"digits.txt").write_text("0"+chr(0x10ffff),encoding="utf8")
        missing = subprocess.run([fontpack,"pack","--project","fonts.toml"],cwd=digits,env=env,capture_output=True)
        if missing.returncode==0 or b"U+10FFFF" not in missing.stderr:
            raise ValueError("missing-glyph diagnostic for numeric project")
        cargo("test", "--offline", cwd=widgets, env=env)
        cargo("generate-lockfile", "--offline", cwd=board / "rust", env=env)
        cargo("doc", "--offline", "--no-deps", "--features", "enhanced", "-p", "liuli-alarms",
              "-p", "liuli","-p","liuli-core","-p","liuli-text","-p","liuli-render",
              "-p","liuli-enhanced","-p","liuli-widgets",cwd=product,env=env)
        cargo("doc", "--offline", "--no-deps", "--lib", cwd=vendor/f"liuli-sim-{VERSION}", env=env)
        shutil.copytree(Path(env["CARGO_TARGET_DIR"]) / "doc", stage / "api")
        filename = f"liuli-sdk-{VERSION}-{platform.system().lower()}-{platform.machine().lower()}.zip"
        temporary = Path(scratch)/filename
        with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as bundle:
            for entry in sorted(stage.rglob("*")):
                if entry.is_file():
                    bundle.write(entry, entry.relative_to(stage))
        shutil.copyfile(temporary,output/filename)
        print(output/filename)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=ROOT/"target"/"dist")
    parser.add_argument("--fetch-toolchain-inputs",action="store_true",help="download any missing pinned Rust sysroot sources")
    args=parser.parse_args()
    package(args.out.resolve(), args.fetch_toolchain_inputs)
