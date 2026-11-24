"""Bundle S3 firmware built from an extracted SDK, including partition offsets and notices."""
import argparse
import hashlib
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZipFile, ZIP_DEFLATED

PROFILES = ("basic", "enhanced", "starter", "benchmark")
GUIDE = """# Waveshare ESP32-S3-Touch-AMOLED-1.91

enhanced.bin 是日常告警固件，basic.bin 为零堆 UI 的基础档，starter.bin 显示 SDK 模板页面，benchmark.bin 会主动输出采样。官方包内的 widget-demo.bin 展示全部控件，可刷到同一应用地址并用触屏逐页操作。manifest.json 记录了文件长度、SHA-256 和写入位置。烧录前用 esptool flash_id 和 USB 设备身份核对型号、16 MiB 容量及当前端口。

使用 ESP-IDF 5.5 的 Python 环境。对于已经具有本分区表和字体的参考板，恢复日常固件只需要以下命令。PORT 替换为设备管理器或 serial.tools.list_ports 确认的端口。

input-trace.bin 为可选的告警触摸录制镜像。使用 SDK 内的 tools/export-touch-trace.py 导出串口记录，详见 docs/waveshare-s3.md。换回日常或全控件镜像会关闭录制。

~~~
python -m esptool --chip esp32s3 -p PORT --after watchdog_reset write_flash 0x10000 enhanced.bin
~~~

空白板完整安装会写入启动程序、分区表、应用和字体。先保存需要保留的原始固件。

~~~
python -m esptool --chip esp32s3 -p PORT --after watchdog_reset write_flash --flash_mode dio --flash_size 16MB --flash_freq 80m 0x0 bootloader.bin 0x8000 partition-table.bin 0x10000 enhanced.bin 0x310000 body.lf4
~~~

按住 BOOT，轻按并释放 RESET，再松开 BOOT 可进入 ROM 下载模式。启动串口为 115200，首帧日志包含 pixels=128640。各版本的实板测量和覆盖范围见 SDK 内的 docs/qualification.md。
"""


def read_json(path):
    return json.loads(path.read_bytes())


def notices(directory):
    for path in directory.rglob("*"):
        if path.name.lower().startswith(("license", "licence", "copying", "notice", "copyright")):
            if path.is_file() and path.suffix.lower() not in (".rs", ".py", ".c", ".h", ".cmake"):
                yield path


def package(sdk, builds, output):
    reference = builds["enhanced"]
    metadata = read_json(reference / "project_description.json")
    version = metadata["project_version"]
    flash = read_json(reference / "flasher_args.json")
    files = {int(offset, 16): name for offset, name in flash["flash_files"].items()}
    if set(files) != {0, 0x8000, 0x10000}:
        raise ValueError("unexpected S3 boot or application partition offset")
    entries = {
        "bootloader.bin": reference / files[0],
        "partition-table.bin": reference / files[0x8000],
        "body.lf4": sdk / "liuli-alarms/assets/body.lf4",
        "partitions.csv": sdk / "boards/waveshare-s3-amoled/partitions.csv",
        "licenses/liuli.txt": sdk / "LICENSE",
        "licenses/chinese-font.txt": sdk / "liuli-alarms/assets/FONT-LICENSE.txt",
        "licenses/complex-fonts.txt": sdk / "boards/waveshare-s3-amoled/bench-fonts/LICENSE.txt",
    }
    for name, directory in builds.items():
        project = read_json(directory / "project_description.json")
        options = read_json(directory / "flasher_args.json")
        if (project["project_version"], project["target"]) != (version, "esp32s3"):
            raise ValueError(f"firmware version or chip mismatch in {name}")
        if options["flash_settings"] != flash["flash_settings"] or options["flash_files"] != flash["flash_files"]:
            raise ValueError(f"flash layout mismatch in {name}")
        if (directory / files[0x8000]).read_bytes() != entries["partition-table.bin"].read_bytes():
            raise ValueError(f"partition table mismatch in {name}")
        entries[f"{name}.bin"] = directory / files[0x10000]
        entries[f"config/{name}.sdkconfig"] = directory / "sdkconfig"

    idf = Path(metadata["idf_path"])
    entries["licenses/esp-idf/LICENSE"] = idf / "LICENSE"
    for component in map(Path, metadata["build_component_paths"]):
        if component.is_relative_to(idf):
            for path in notices(component):
                entries["licenses/esp-idf/" + path.relative_to(idf).as_posix()] = path
    vendor = sdk / "vendor"
    for path in notices(vendor):
        entries["licenses/rust/" + path.relative_to(vendor).as_posix()] = path

    records = {name: {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
               for name, path in entries.items() if not name.startswith("licenses/")}
    manifest = {
        "version": version, "board": "Waveshare ESP32-S3-Touch-AMOLED-1.91", "flash_bytes": 16*1024*1024,
        "offsets": {"bootloader.bin":"0x0", "partition-table.bin":"0x8000",
                    "selected_application":"0x10000", "body.lf4":"0x310000"},
        "files": records,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(dir=output.parent) as scratch:
        candidate = Path(scratch) / output.name
        with ZipFile(candidate, "w", ZIP_DEFLATED) as bundle:
            for name, path in sorted(entries.items()):
                bundle.write(path, name)
            bundle.writestr("README.md", GUIDE)
            bundle.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
        with ZipFile(candidate) as bundle:
            if bundle.testzip() is not None:
                raise ValueError("firmware archive failed CRC verification")
            for name, record in records.items():
                if hashlib.sha256(bundle.read(name)).hexdigest() != record["sha256"]:
                    raise ValueError(f"checksum mismatch in {name}")
        candidate.replace(output)
    print(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True, help="extracted SDK root")
    for name in PROFILES:
        parser.add_argument("--" + name, type=Path, required=True, help=f"{name} ESP-IDF output directory")
    parser.add_argument("--widget-demo", type=Path, help="include the interactive widget catalog firmware")
    parser.add_argument("--input-trace", type=Path, help="include the alarm touch recording firmware")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    builds = {p: getattr(args, p).resolve() for p in PROFILES}
    if args.widget_demo:
        builds["widget-demo"] = args.widget_demo.resolve()
    if args.input_trace:
        builds["input-trace"] = args.input_trace.resolve()
    package(args.sdk.resolve(), builds, args.out.resolve())
