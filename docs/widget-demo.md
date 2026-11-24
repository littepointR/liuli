# Widget Demo 全控件演示

解压对应平台的 SDK 后运行 `tools/bin/liuli-widget-demo`，Windows 使用 `tools/bin/liuli-widget-demo.exe`。打开终端打印的网址，浏览器中显示与参考板同尺寸的 240 × 536 页面。程序自带所需字体。可点击、触摸或通过预览器的导航键操作。首页的 Theme 切换浅色和深色，各页的 Back 返回目录。

<img src="hardware/widget-home.png" width="200" alt="Widget Demo 首页"> <img src="hardware/widget-data.png" width="200" alt="进度条、曲线和自绘表"> <img src="hardware/widget-text.png" width="200" alt="多语言编辑框和板上组合输入按钮">

| 页面 | 可以直接尝试的操作 |
|---|---|
| Buttons & sliders | 按按钮计数、用开关禁用或启用按钮、拖动滑块。标签显示点击次数和当前百分比。 |
| Meters & charts | 点击 Add sample，观察进度条、32 点环形缓冲曲线和自绘数值表同步更新。 |
| Tabs & selectors | Mode 页选择互斥项，Value 页按 5 的步长调整数值。切回后保留选项。 |
| Dialogs & toasts | 打开模态确认框，尝试确认、取消和返回。确认后提示条显示两秒，也可单独触发。 |
| Virtual list | 在 1,000 条记录间滚动并点选条目。选中项显示勾号，活动节点随视口有界。 |
| Icon set | 逐项查看全部 11 个命名图标及其按钮状态，点选后显示名称。 |
| Image transforms | 旋转、缩放绘制的 RGB565 指针图像，用滑块调节 16 档透明度，观察裁剪窗口。 |
| Text field | 显示中文、拉丁字母、阿拉伯文和天城文。桌面可使用输入法；实板可用 Compose、Commit、Cancel preedit 和 Backspace 操作同一个单行编辑器。 |

完整演示采用增强配置。基础配置可从源码运行前七页。页面布局使用根节点、行、列、层叠、纵向滚动和模态容器。Shift + 点击桌面预览可查看节点和命中目标；导出的输入轨迹可直接交给同一程序。

~~~sh
tools/bin/liuli-widget-demo --headless
tools/bin/liuli-widget-demo --screenshots widget-shots
tools/bin/liuli-widget-demo --replay recorded.trace
~~~

无头模式逐页发送指针和按键输入，核对开关禁用、选择、模态返回、提示消失、列表滚动、旋转和文本组合，并将每一次实际呈现的帧与全屏参考绘制比对。它还单独核对首页按钮和紧邻下一行的列表项焦点框，四条边上的每个像素都应保留焦点色。使用 `--port 0` 可为交互模式分配空闲本机端口。

日常开发可运行 `python tools/check-ui.py`，同时执行控件目录和告警产品的无头回归。[离线运行和编写夹具](sdk.md#无人值守的-ui-回归)

从仓库源码启动完整演示：

~~~sh
cargo run -p liuli-widget-demo --features desktop,enhanced
~~~

参考板固件包内的 `widget-demo.bin` 与日常告警程序共用分区表和 LF4 字库。已安装本框架的板子可单独替换应用分区。空白板的完整写入步骤见板级指南。使用 SDK 从源码重建演示固件：

~~~sh
python boards/waveshare-s3-amoled/build.py --profile widget-demo --offline --out target/widget-demo
~~~
