"""Run the same unattended UI regressions in a checkout or an extracted SDK."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def planned(app, action, features=None, example=None):
    command = [sys.executable, str(ROOT/"tools/build-ui.py"), "--app", str(app/"Cargo.toml"),
               "--offline", "--locked", "--action", action]
    if features:
        command += ["--app-features", features]
    if example:
        command += ["--plan-example", example]
    return [*command, "--", "--release"]


def check(screenshots):
    packaged = (ROOT / "vendor").is_dir() and (ROOT / "liuli-alarms").is_dir()
    apps = ROOT if packaged else ROOT / "apps"
    alarms = apps / "liuli-alarms"
    catalog = apps / "liuli-widget-demo"
    fixtures = alarms / "tests/fixtures"
    manifest = str(alarms / "Cargo.toml")
    if packaged:
        exe = ".exe" if os.name == "nt" else ""
        demo = [str(ROOT / "tools/bin" / ("liuli-widget-demo" + exe))]
    else:
        demo = [*planned(catalog, "run", "desktop,enhanced"), "--bin", "liuli-widget-demo", "--"]
    if screenshots:
        run(*demo, "--studio", "--screenshots", str(screenshots.resolve()))
    else:
        run(*demo, "--studio", "--headless")

    landscape = ["--screenshots", str(screenshots.resolve() / "fk743m2")] if screenshots else ["--headless"]
    run(*demo, "--studio", "--size", "800x480", *landscape)
    run(*demo, "--size", "240x536", "--headless")
    run(*demo, "--showcase", *landscape)
    for size, trace in (("800x480", "h743-widgets.trace"), ("240x536", "portrait-widgets.trace"),
                        ("800x480", "h743-strip-fling.trace"), ("800x480", "h743-composition.trace"),
                        ("800x480", "h743-focus-shapes.trace")):
        run(*demo, "--size", size, "--replay", str(catalog / "tests/fixtures" / trace))

    run("cargo", "test", "--offline", "--locked", "--release", "--manifest-path",
        str(catalog / "Cargo.toml"), "--features", "desktop", "--test", "composition", "--test", "geometry")
    run("cargo", "test", "--offline", "--locked", "--release", "--manifest-path",
        str(catalog / "Cargo.toml"), "--features", "desktop,enhanced", "--test", "effects",
        "--test", "showcase", "--test", "composition", "--test", "geometry")
    run("cargo", "test", "--offline", "--locked", "--release", "--manifest-path", manifest,
        "--all-features", "--tests")
    run(*planned(alarms, "run", example="diagnostics"), "--example", "diagnostics")
    editor = [*planned(alarms, "run", "enhanced", "editor"), "--example", "editor", "--"]
    run(*editor, str(catalog/"assets/Arabic.ttf"), str(catalog/"assets/Devanagari.ttf"), "--headless")
    run(*editor, "--replay", str(ROOT/"docs/hardware/rc1-browser-ime.trace"))
    with TemporaryDirectory() as directory:
        exported = Path(directory) / "from-device.trace"
        run(sys.executable, str(ROOT/"tools/export-touch-trace.py"), "--log",
            str(fixtures/"s3-long-press.serial.gz"), "--out", str(exported))
        if exported.read_text(encoding="ascii") != (fixtures/"s3-long-press.trace").read_text(encoding="ascii"):
            raise ValueError("exported board trace differs from the regression fixture")
        run(*planned(alarms, "run"), "--example", "preview", "--", str(alarms/"assets/body.lf4"), "--replay", str(exported))
    print("UI regressions passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screenshots", type=Path, help="save the rendered widget gallery")
    check(parser.parse_args().screenshots)
