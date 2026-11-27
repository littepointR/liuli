# Waveshare ESP32-S3-Touch-AMOLED-1.91

板上逻辑坐标为竖屏 240 × 536。RM67162 使用 SPI2 四线数据、40 MHz，GPIO 47 为时钟，GPIO 6 为片选，D0 至 D3 对应 GPIO 18、7、48、5，GPIO 17 为复位。FT3168 使用 I²C 地址 0x38，SDA 40，SCL 39。BSP 将输入报告中的触点 ID 转为框架的 down、move、up 和 cancel。触摸峰值从实板报告读取。[微雪硬件资料](https://docs.waveshare.com/ESP32-S3-AMOLED-1.91)

## 构建

RC1 使用 ESP-IDF 5.5.2、其 Python 环境及带 rust-src 的 Xtensa Rust 1.97.0.0。进入已导出环境的 ESP-IDF 终端，在项目根目录运行以下命令。

~~~
python boards/waveshare-s3-amoled/build.py --profile basic
python boards/waveshare-s3-amoled/build.py --profile enhanced
python boards/waveshare-s3-amoled/build.py --profile benchmark
python boards/waveshare-s3-amoled/build.py --profile widget-demo
~~~

可用 --out 为各档指定独立目录，--rust-only 只构建 Xtensa 静态库。脚本用 ESP Rust 分支编译 MCU，并显式允许已实测的分支低于桌面 workspace 的 rust-version。驱动和 ESP-IDF 负责 QSPI/DMA 完成。当前提交路径同步等待每次传输。基本配置在内部 RAM 持有两块 240 × 8 RGB565 缓冲区，增强配置另用 PSRAM 存放字体缓存。基准配置额外包含复杂文字测试字体和固定的输入序列。

## 接入自有应用

解压 SDK 后，以下命令直接构建 device-ui/src/lib.rs 对应的基础档固件。桌面预览和这条命令共用应用代码。烧录时使用 device-ui/assets/body.lf4。

~~~
python device-ui/board/build.py --offline --profile basic --out target/starter
~~~

板级循环位于 rust/src/lib.rs，绑定应用的代码位于 rust/src/application.rs。将 rust/Cargo.toml 的应用依赖指向 SDK 内的 device-ui 或新的应用 crate，再从 application.rs 导出 BoardApp 并实现 model(height)。模板的 BoardApp 是 device_ui::Device，初始模型是 false。自有应用使用 basic 或 enhanced 构建，benchmark 绑定官方告警样例的消息。SDK 的 app 和 BSP 都应以 SDK 根目录的供应源解析依赖。

## 分区与恢复

USB Serial/JTAG 的 Windows 端口号可能改变。先用以下命令枚举并核对 Espressif USB 身份、芯片和 Flash 容量。首次写入会替换现有程序；需要保留时，先保存完整的 16 MiB 镜像并核对文件长度和散列。

~~~
python -m serial.tools.list_ports -v
python -m esptool --chip esp32s3 -p PORT flash_id
python -m esptool --chip esp32s3 -p PORT read_flash 0 0x1000000 original.bin
~~~

在 target/s3/idf 中使用生成的 flasher_args.json 复核地址。板载 ROM 下载模式可通过按住 BOOT、轻按 RESET、再释放 BOOT 进入。用自己的端口替换 PORT，最后的字体路径指向本次应用配置生成的 LF4：

~~~
python -m esptool --chip esp32s3 -p PORT write_flash 0x0 bootloader/bootloader.bin 0x8000 partition_table/partition-table.bin 0x10000 liuli_amoled.bin 0x310000 PATH_TO_BODY.lf4
~~~

只更新应用时可只写 0x10000。更新字库只写 0x310000，分区容量为 1 MiB；应用分区容量为 3 MiB。以 115200 波特率读取运行日志。在同一块未启用加密或安全启动的参考板上，完整备份可用 esptool write_flash 0 original.bin 恢复。本机此次按用户选择跳过原固件备份。

sleep-benchmark 配置另行启用 MCU 轻睡测试。轻睡会挂起 USB Serial/JTAG。轻睡配置在入睡前将该 ELF 的标识和测试状态保存到已有 NVS 分区的 liuli_qa 命名空间。只有验证定时器唤醒、重新绘制并完成传输后，才写入完成状态和耗时。下一次正常启动会读出结果；若只读到入睡前标记，会报告未完成并进入普通告警界面。同一个镜像在断电后仍保留该次记录。回到日常使用时刷入 enhanced.bin。[乐鑫 USB Serial/JTAG 休眠说明](https://docs.espressif.com/projects/esp-idf/en/v5.5.1/esp32s3/api-guides/usb-serial-jtag-console.html)

## 将实板触摸录成桌面回归

从离线 SDK 根目录构建带串口记录的告警固件。基础和增强配置都可单独启用 `--trace-input`。普通构建通过 Cargo 特性移除记录器；调试配置使用固定大小的时间起点和序号，直接写串口。串口会增加耗时，性能基准使用普通固件。

~~~sh
python boards/waveshare-s3-amoled/build.py --profile enhanced --trace-input --offline --out target/trace-input
python tools/export-touch-trace.py --port PORT --seconds 45 --out touch.trace
~~~

将 debug 应用镜像写入应用分区，在第二条命令显示 Listening 后轻按 RESET，再执行触摸操作。若已有同一次启动的完整串口日志，也可离线导出。导出器检查启动头、连续序号、单调时间和触点数据。原始事件使用设备实际提交给 Engine 的逻辑坐标，时间为相对 Engine 创建时刻的毫秒。每次成功推进引擎时钟都记录 tick，手指停在原处时也能重现长按。cancel 携带触点 ID；up 携带最后一个有效逻辑坐标。桌面回放按事件渲染并对照全屏参考，板端在一次轮询结束后提交显示。

~~~sh
python tools/export-touch-trace.py --log serial.log --out touch.trace
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example preview -- liuli-alarms/assets/body.lf4 --replay touch.trace
~~~

随包的 `liuli-alarms/tests/fixtures/touch-workflow.trace` 全程通过原始指针坐标操作列表、详情、编辑器和模态框。运行 `cargo test --offline --manifest-path liuli-alarms/Cargo.toml --test workflow` 可断言第 12 条记录从 50 变为 75，返回后恢复偏移 320 和焦点，下一条记录仍为 50。

固件包中的 input-trace.bin 使用增强告警配置。下面两条命令直接从实板串口封包重建回放并在桌面运行。原固件录制时两次上滑累计偏移 415 px，触点 0 在 (92, 389) 停住 900 ms。启用抬手惯性的引擎回放相同输入时，含滑行的偏移为 438 px，仍在收到 up 前打开第 25 条告警。

~~~sh
python tools/export-touch-trace.py --log liuli-alarms/tests/fixtures/s3-long-press.serial.gz --out hardware.trace
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example preview -- liuli-alarms/assets/body.lf4 --replay hardware.trace
~~~

## 生成固件分发包

在离线 SDK 根目录分别构建 basic、enhanced、benchmark 和 device-ui 模板，将各自的 IDF 输出目录传给随包脚本。脚本核对目标芯片、版本、分区表及各文件校验和，把日常固件、基准固件、字体、配置和许可证一起打包。解压后的 README.md 给出空白板安装和仅恢复日常程序的命令。

~~~
python package-firmware.py --sdk . --basic target/basic/idf --enhanced target/enhanced/idf --benchmark target/benchmark/idf --starter target/starter/idf --widget-demo target/widget-demo/idf --input-trace target/trace-input/idf --out liuli-waveshare-s3.zip
~~~
