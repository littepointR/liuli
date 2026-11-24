# Waveshare ESP32-S3-Touch-AMOLED-1.91

板上逻辑坐标为竖屏 240 × 536。RM67162 使用 SPI2 四线数据、40 MHz，GPIO 47 为时钟，GPIO 6 为片选，D0 至 D3 对应 GPIO 18、7、48、5，GPIO 17 为复位。FT3168 使用 I²C 地址 0x38，SDA 40，SCL 39。BSP 将输入报告中的触点 ID 转为框架的 down、move、up 和 cancel。触摸峰值从实板报告读取。[微雪硬件资料](https://docs.waveshare.com/ESP32-S3-AMOLED-1.91)

## 构建

准备 ESP-IDF 5.5 工具链、其 Python 环境及包含 rust-src 的 esp Rust 工具链。进入已导出环境的 ESP-IDF 终端。在项目根目录运行：

~~~
python boards/waveshare-s3-amoled/build.py --profile basic
python boards/waveshare-s3-amoled/build.py --profile enhanced
python boards/waveshare-s3-amoled/build.py --profile benchmark
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

sleep-benchmark 配置另行启用 MCU 轻睡测试。轻睡会挂起 USB Serial/JTAG。基准配置在定时唤醒并重新绘制后将结果存入 RTC 内存，通过软件复位重新输出。若主机未重新枚举，重连后核对端口。[乐鑫 USB Serial/JTAG 休眠说明](https://docs.espressif.com/projects/esp-idf/en/v5.5.1/esp32s3/api-guides/usb-serial-jtag-console.html)
