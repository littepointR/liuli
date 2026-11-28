# FK743M2-IIT6 排针版与 RGB050M1

该板包运行与 S3 和桌面相同的 Widget Demo。适配对象是板载 25 MHz 晶振的 STM32H743IIT6、W9825G6KH SDRAM 和 5 寸 800 × 480 横屏。BSP 使用片内 Flash 启动，通过 FMC 访问 SDRAM，通过 LTDC 连续扫描 RGB565。板级时序按反客配套资料中 800 × 480 LTDC 例程配置，外设时序可对照 ST 参考手册。[STM32H743 RM0433](https://www.st.com/resource/en/reference_manual/dm00314099-stm32h742-stm32h743-753-and-stm32h750-value-line-advanced-arm-based-32-bit-mcus-stmicroelectronics.pdf)

## 从 SDK 构建和写入

安装 Python 3、支持 ARM 目标的 LLVM Clang、Rust 的 `thumbv7em-none-eabihf` 标准库和 SEGGER J-Link Commander。`FK743_CLANG` 可指定 Clang 的绝对路径。SDK 已包含固定版本的 STM32CubeH7、CMSIS 源码和 Embassy crates。以下命令从解压的 SDK 根目录或源码根目录运行。输出目录保留 ELF 和可直接写入的二进制。发布包的 `boards/fk743m2-iit6/firmware/` 还提供四个预构建镜像、RAM 写入器和 `SHA256SUMS`。

~~~sh
rustup target add thumbv7em-none-eabihf
python boards/fk743m2-iit6/build.py --backend st-hal --profile enhanced --offline --out target/fk743-st-enhanced
python boards/fk743m2-iit6/build.py --backend embassy --profile enhanced --offline --out target/fk743-embassy-enhanced
python boards/fk743m2-iit6/jlink.py
python boards/fk743m2-iit6/jlink.py --flash target/fk743-st-enhanced/widget-demo.bin
~~~

默认固件进入功能展台。桌面可生成回放参考图，J-Link 可注入同一组触摸并核对实板扫描缓冲区。[功能展台指南](widget-demo.md) 要运行下文的 Spool 录音台全屏检查点与磁带套滚动轨迹，请在构建对应固件时传入 `--studio`。

`--profile basic` 用相同入口构建零堆固件。默认后端 `st-hal` 由 HAL 配置 RCC、PWR、FMC、SDRAM、LTDC 和 MPU；LL 负责 GPIO 快速读写，SysTick 提供 HAL 超时。选 `embassy` 时，`embassy-stm32` 配置时钟，TIM5 提供单调时间，UI 在 Embassy 线程执行器上运行，按 4 ms 定时器让出执行权。两个后端共用 STM32Cube 显示、SDRAM 和触控引脚驱动，以及同一套应用与调试接口。PI11 和 PI8 分属不同的硬件 I²C 复用组合，因此 GT911 使用开漏 GPIO 实现 I²C。[STM32CubeH7 HAL](https://github.com/STMicroelectronics/stm32h7xx-hal-driver) · [Embassy STM32H743II](https://docs.embassy.dev/embassy-stm32/0.6.0/stm32h743ii/)

Windows 的 Commander 可执行文件是 `JLink.exe`，Linux 和 macOS 是 `JLinkExe`。可用 `--jlink "安装目录/JLink.exe"` 指定。工具自动选择唯一的 SEGGER USB 探针，有多个探针时用 `--serial` 选择。连接使用 SWD，默认 100 kHz（`--speed` 的单位是 kHz），可用 `--speed` 调整。板卡使用独立稳定供电，连接 SWCLK、SWDIO、GND 和 3.3 V VTref，复位线接 NRST。核心板 P1 的 1、2、3、7、8 脚分别对应上述 SWCLK、SWDIO、GND、NRST、VTref。

工具先读取芯片 ID、片内 Flash 容量和 UID，再从 `0x08000000` 写入并校验镜像。重启后通过 SWD 检查应用就绪标志和当前扫描缓冲区地址。LF4 字库编入固件。写入范围为片内应用覆盖的扇区；外部 W25Q64 的资源和用户设置继续保留。

如果 Commander 的内置 Flash 算法报告擦除失败，可用 SDK 自带的 STM32Cube HAL RAM 写入器。此模式使用 SEGGER 动态库和 Python 包 `pylink-square==2.1.0`。源码检出中使用 Clang 和 Rust 工具链自带的链接器构建 RAM 写入器；SDK 直接使用随包的写入器。主机在同一个连接中校验临时 RAM 的每一块，板端按 128 KiB 扇区擦写并核验 CRC32，最后再读整份映像核验一次。SDK 的 `wheels/` 目录提供对应桌面平台的 Python wheel，安装时按随包摘要进行校验。

~~~sh
python -m pip install --no-index --require-hashes --find-links boards/fk743m2-iit6/wheels -r boards/fk743m2-iit6/probe-requirements.txt
python boards/fk743m2-iit6/jlink.py --ram-writer --speed 12000 --flash target/fk743-embassy-enhanced/widget-demo.bin
~~~

本次 5 寸参考板在 100 kHz 下核对芯片身份，在 12 MHz 下完成分块传输与校验。`--speed` 控制传输频率，可按探针和线长调整。

## 显示、内存与输入

| 项目 | BSP 配置 |
|---|---|
| 时钟 | CPU 400 MHz，HCLK 200 MHz，SDRAM 80 MHz，像素时钟 33 MHz |
| 面板 | HSYNC / HBP / HFP 为 1 / 80 / 200，VSYNC / VBP / VFP 为 1 / 40 / 22，背光 PH6 |
| RGB565 扫描缓冲区 | SDRAM `0xC0000000` 和 `0xC00C0000`，每块 768,000 字节 |
| UI | 内部 AXI SRAM 存放 Engine 和 800 × 32 像素条带，调用栈位于 DTCM |
| 基础档 | 六个顶层页签，控件页含五组交互样例。UI 零堆，使用外部 SDRAM 作为显示内存 |
| 增强档 | 同一套页签，增加复杂文字、双触点变换、透明图层和毛玻璃。UI 优先从静态对象之后的空闲 AXI SRAM 分配，空间不足时使用 SDRAM 的后 30 MiB。每份命令列表的图层预算为 1 MiB |
| Goodix 触摸 | PI11 SCL、PI8 SDA、PG3 INT、PH4 RESET。探测 0x5D、0x14，读取产品 ID 和分辨率，按触点 ID 生成事件 |

LTDC 在垂直消隐时换到新帧，SDRAM 的 MPU 属性为普通、Write-Back 可缓存，驱动在 LTDC/DMA2D 与 CPU 交接处显式执行 D-Cache clean/invalidate 与数据同步屏障。换帧完成后，由硬件 Chrom-ART `DMA2D` 异步 2D 跨步搬移将已更新像素区域回填到备用缓冲区，与下一帧 CPU 绘制并行。扫描缓冲区由外设持续读取，UI 静止时无需提交新帧。触摸驱动区分暂无新报告、释放和 I²C 错误。未检测到控制器时保持显示并重试。

在此硬件与紧凑图层架构（`RGB565 + A4` 与原地盒式模糊）下，实板 `Flip card`（97,792 像素）呈现耗时：开启毛玻璃时 P50 为 30.57 ms、P95 为 48.45 ms（25.9 fps）；关闭毛玻璃时 P50 为 30.81 ms、P95 为 48.68 ms（26.7 fps），含 LTDC 单帧垂直消隐等待。

## 自动验收

桌面程序支持 `--size 800x480` 预览、无头回放和截图。在 SDK 中执行下列命令可先跑完整横屏回归，再生成实板对照输入和每个检查点的参考帧。Windows 可使用 `tools/bin/liuli-widget-demo.exe`。

~~~sh
tools/bin/liuli-widget-demo --studio --size 800x480 --headless
tools/bin/liuli-widget-demo --size 800x480 --panel-frames target/fk743-frames
python boards/fk743m2-iit6/jlink.py --compare target/fk743-frames
~~~

Spool 增强档共 31 个检查点，覆盖三个主区、上下级页面切换，以及返回录音台。比较工具通过 SWD 写入原始按下、松开及逻辑坐标，等待固件确认提交，核对页面，再对当前 LTDC 扫描缓冲区的 768,000 字节做逐字节比对。固件在收到快照命令时将当前扫描像素按游程无损编码到预留 SDRAM，主机还原完整帧并逐字节比较。`--probe-api` 使用 SEGGER 动态库轮询每次提交的确认序号，分块重复读回一份稳定的快照，适合诊断间歇性 SWD 传输错误。`--raw-scanout` 可直接导出未压缩像素。物理触摸坐标和面板实际出画可另行观察。SDK 默认桌面程序启用了增强特性，基础档的参考帧须用仅启用 `desktop` 的 Widget Demo 生成。

在解压后的 SDK 中，先用 `--studio` 构建用于此组检查点的固件。两个后端可共用同一档位的桌面参考帧。

~~~sh
python tools/build-ui.py --locked --offline --app liuli-widget-demo/Cargo.toml --app-features desktop --action run -- --release --bin liuli-widget-demo -- --size 800x480 --panel-frames target/fk743-frames-basic
python boards/fk743m2-iit6/build.py --offline --backend embassy --profile basic --studio --out target/fk743-studio-basic
python boards/fk743m2-iit6/jlink.py --speed 12000 --ram-writer --probe-api --flash target/fk743-studio-basic/widget-demo.bin --compare target/fk743-frames-basic
~~~

片内 SRAM `0x24000000` 起有 16 个小端 32 位诊断字。字 0 为签名 `0x4C554C49`，字 1 为启动阶段（4 表示首帧就绪），字 2 至 5 为帧数、扫描地址、最近一次提交像素数和呈现耗时（微秒），字 6 至 9 为触控产品 ID、I²C 报错数、当前页面和 LTDC 状态。字 10 至 13 为输入序列、动作、打包坐标和已提交序列。动作 6 导出当前帧；字 14 为 `0xC0180000` 处快照的字节数，超出 512 KiB 返回 `0xFFFFFFFF`。字 15 记录故障状态。异常时阶段为 `0xBAD00000 | 错误码`。这些字段与桌面参考帧共同定位启动、渲染和传输问题。

四种固件的首帧提交耗时、Flash CRC32 和逐页实板结果见配套记录。[实板验收报告](hardware/fk743-widget-demo.md)

### 磁带套图形区滚动回归

同一条 800 × 480 轨迹会按住屏幕，把顶部卡片完全滑出，再沿原路滑回。桌面回放逐帧比较增量与整屏绘制。实板工具按每次提交的确认序号注入轨迹坐标，在四个位置读回完整扫描缓冲区，与桌面输出逐字节比较。触摸由工具产生，板上的时钟用于帧耗时测量。

~~~sh
python tools/build-ui.py --app apps/liuli-widget-demo/Cargo.toml --app-features desktop,enhanced --action build --locked --offline -- --bin liuli-widget-demo --release
target/release/liuli-widget-demo --studio --size 800x480 --replay apps/liuli-widget-demo/tests/fixtures/h743-depth-scroll.trace
python boards/fk743m2-iit6/jlink.py --speed 12000 --depth-scroll apps/liuli-widget-demo/tests/fixtures/h743-depth-scroll.trace target/release/liuli-widget-demo
~~~

Windows 把桌面程序路径写为 `target/release/liuli-widget-demo.exe`。在含本次修复的离线 SDK 中，使用 `liuli-widget-demo/tests/fixtures/h743-depth-scroll.trace` 和 `tools/bin/` 下的桌面程序，板上运行同一构建的增强版 Widget Demo。工具在卡片接近顶部与完全隐藏时分别取中位数，两组相差超过 18 ms（约一个面板刷新周期）、检测到 LTDC 欠载或传输错误、扫描像素不匹配时均会报错。

## 生产与调试配置

`build.py --touch-irq` 在 PG3 启用 GT911 的 EXTI3。触点活动期间继续采样至抬起。默认以 4 ms 为间隔轮询，`--touch-poll-ms` 接受 1 到 1000 ms。两个后端都优先服务更早的 UI 截止时间。HAL 等待时使用 WFI，SysTick 维持毫秒时钟。Embassy 使用 TIM5 和任务唤醒。

普通烧录按写后校验结果启动程序。需要在烧录后读取诊断邮箱时，给 jlink.py 添加 `--diagnostics`，像素对照命令会自动核对邮箱。自行编译用于 SWD 自动输入、计数和截图的固件时添加 `--diagnostics`。SDK 内预编译的测试镜像已经启用。生产配置释放测试邮箱和截图保留区。板级 DMA2D 由显示驱动管理。


实板空闲验收可运行 `jlink.py --speed 12000 --touch-wakeup target/fk743-irq/widget-demo.elf`。工具要求已安装的镜像与 ELF 完整匹配，使用 pylink-square 和 llvm-nm，在 500 ms 静止期间监测触摸总线，再触发一次 EXTI3 软件中断，核对执行到达中断入口及 GT911 读事务并复位。该测试覆盖唤醒与分发；物理 INT 电气时序需单独测量。
