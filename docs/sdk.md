# 从空目录使用琉璃 SDK

解压对应操作系统的 SDK 到空目录。桌面开发需要 Rust 1.100 和系统链接器。SDK 内的 .cargo/config.toml 已指向随包供应的 Cargo 源。保留该目录和 vendor，然后在 SDK 根目录运行：

~~~
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview -- --headless
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview
~~~

浏览器打开终端打印的本机地址。第一条命令会用模板应用构建两帧，对照完整重绘。第二条命令会提供交互预览。在 device-ui/src/lib.rs 实现 App，Model 保存业务数据，update 返回 Change::View 时框架重建视图。桌面和设备使用相同的键、布局和逻辑坐标。在 api/liuli/index.html 和 api/liuli_widgets/index.html 查看 API。

## 无人值守的 UI 回归

安装 Python 3 和桌面 Rust 工具链后，在解压的 SDK 根目录执行以下命令。源码仓库也有相同入口，首次使用源码时先用 `cargo fetch --locked` 准备依赖。

~~~sh
python tools/check-ui.py
python tools/check-ui.py --screenshots shots
~~~

三平台打包 CI 会在空 Cargo 缓存下运行第一条命令。它直接驱动应用的输入、命中测试和单调时钟，比较增量渲染与完整重绘，同时检查八页控件、焦点框的四条边、告警编辑后返回的位置、输入法组合与故障恢复。失败会以非零状态退出，并输出失败的场景或断言。第二条命令还会保存控件画面。整个过程使用随包夹具，可以在没有浏览器、串口或开发板的构建机上执行。

为应用添加回归时，可以直接编写逻辑坐标和时间戳。例如下面的夹具在同一位置按住 900 ms。中间的 `tick` 在手指静止时推进时钟，供引擎触发长按。将轨迹交给 `replay_trace`，用应用模型断言目标条目、打开的页面或焦点，并启用逐帧像素对照。随包的 `liuli-alarms/tests/workflow.rs` 展示了如何在抬手前断言页面变化。

~~~text
liuli-trace 1
0 kind=down&v=0&x=92&y=389
800 tick
900 kind=up&v=0&x=92&y=389
~~~

发生实板特有的问题时，可自愿将该次输入从串口导出，作为另一条夹具。随包的实板记录可以离线反复重放。要在参考板上测量像素计算或 QSPI 传输时间，可直接启动 `benchmark.bin`，它会自动执行固定输入和像素对照。[参考板录制和基准](waveshare-s3.md)

## 控件与主题

安装包内的 liuli-widget-demo 是完整交互控件目录。运行 tools/bin/liuli-widget-demo（Windows 使用 .exe），打开终端打印的网址即可操作八个分类。示例源码位于包根目录的 liuli-widget-demo。板级固件使用同一份应用代码。[控件清单和运行方法](widget-demo.md)

用下面的命令启动控件目录，切换 Controls / Meter，操作选择器、数值编辑器和确认框。控件目录可独立从供应的控件包构建。

~~~
cargo run --offline --manifest-path liuli-widgets/Cargo.toml --example catalog -- device-ui/assets/body.lf4
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example preview -- liuli-alarms/assets/body.lf4
cargo test --offline --manifest-path liuli-alarms/Cargo.toml --release
~~~

| 控件 | 应用接入方式 |
|---|---|
| 按钮、开关、滑块、进度条、曲线 | 调用 Children 上的对应构建方法。通过 UiAction 处理激活、切换和数值变化。 |
| 虚拟列表 | virtual_list 接收非零行高、记录数、偏移和视口高度。为每个数据 ID 提供稳定 Key，在 App::on_scroll 中更新偏移。 |
| 数值编辑、选择器、页签 | number_editor 产生 SetValue。selector 和 tabs 使用调用方提供的选项键。应用保存选中项。 |
| 对话框、提示条 | confirm_dialog 使用模态焦点域和 Dismiss 动作。toast 使用无焦点标签，显示时长由应用的时钟控制。 |
| 自绘 | Children::custom 建立节点，App::measure_custom 给出内容尺寸，App::paint_custom 可调用 WidgetCanvas::fill 或 bitmap。绘制使用相对坐标，框架负责裁剪。 |

组件本地状态可放在模型的 KeyedState<T, N> 中，按页面和稳定控件键查找。超容量返回 StateCapacity，保留旧条目。由组件在退出页面或删除数据后调用 retain 释放对应条目。

复合组件可以是接收 Children、键和主题的普通 Rust 函数。liuli-widgets 中的 paint_meter、confirm_dialog 和 virtual_list 是可运行范例。为复合组件的每个子节点提供稳定键。将决定自绘内容的值保存在 NodeProps 中，框架依据提交前后的属性差异决定损伤区域。

从 Theme::DARK 或 LIGHT 开始，把主题传给子组件。Theme::with(Style { accent: Some(color), ..Default::default() }) 生成局部覆盖。更深的组件继续使用该值作为父主题。颜色变化影响绘制，字体、间距和尺寸影响布局。字体槽可以通过 FontSet::new 在构建应用时单独指定。

把 Tween::at(initial) 存入组件状态，用 animate_to(now, target, duration_ms, easing) 控制位置、缩放或透明度。App::next_deadline 返回 tween.next_deadline 的结果，App::on_time 读取 tween.value(now) 更新绘制属性。重新设置目标会从当前取值续接，cancel(now) 可在页面退出时冻结动画。

## 字库与图片

device-ui/assets/fonts.toml 中的 source、charset 和 output 都相对配置文件。每个 [[font]] 提供源字体、Unicode 字符集文件、字号和一个源字体具备的回退字符。budget 限制所有槽位生成的总字节数。把纯数字项目的 fallback 设置为项目中已有的字符即可；配置文件支持多项和自定义像素字号。

~~~
tools/bin/liuli-fontpack pack --project device-ui/assets/fonts.toml
tools/bin/liuli-fontpack verify --project device-ui/assets/fonts.toml
tools/bin/liuli-image logo.ppm device-ui/assets/logo.l565
~~~

图片工具接收二进制 RGB8 PPM（P6）。在应用里用 Bitmap::from_asset(include_bytes!("../assets/logo.l565")) 建立静态图片，在 paint_custom 里向 canvas.bitmap 传入目标矩形、旋转角和 0 到 15 的透明度。一个整圈是 256 个角度单位。样例使用的字体子集随附 OFL 许可证；替换字体时保留新字体的再分发许可。

基础档可用 Lf4Font::from_bytes 和 FontSet::single 直接读取 LF4 v1。增强参考板把索引映射到地址空间，传给 Engine::present_sync_with_pixels 的 GlyphPixels 仅在字形与条带相交时读取位图。StoredFont、ReadAt、Cached 分离索引验证、按偏移读、缓存预算和调用方提供的字形缓冲区。未找到的字符、格式错误和底层读取错误分别返回。两种字体槽使用不同资源时，读取实现需按 FontId 分派。

## 增强档文本框

用下面的命令打开单行编辑器。输入控件取得框架焦点后，预览器会显示“输入到当前文本框”。在此处使用系统输入法、粘贴文本或用左右方向键和退格键编辑。用 --headless 可重放组合更新、提交和取消，逐帧检查增量绘制与全屏参考图像。

~~~
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example editor --features enhanced -- boards/waveshare-s3-amoled/bench-fonts/Arabic.ttf boards/waveshare-s3-amoled/bench-fonts/Devanagari.ttf
~~~

示例把 TextField 放在模型中，用 accepts_text 和 on_text 接收焦点路由的 TextInput。预编辑与已提交字符串分开保存，按字素进行删除，按视觉顺序移动光标。paint_model 提交不可变的 RGB565 文本图，保留到该帧被替换。单行控件在创建时指定最大 UTF-8 字节数，容量或栅格化失败时保留上次成功的文字和图像。

## 录制、排查与回归

在浏览器里 Shift + 点击节点可查看键、祖先、声明尺寸、测量结果、裁剪与最终颜色，同时可对照实际输入目标。Theme::control 记录主题、声明位置以及每个局部覆盖的来源，检查器展示当前已提交视图的记录。帧诊断记录触发输入和命中路径、实际变色像素、传输字节、内嵌字形请求次数和字节数、跨条带重复请求及容量峰值。状态面板显示焦点、捕获者、节点数、命令数、文本池字节数和本帧传输像素。Damage 和 Bands 标注重绘与发送区域。点击“导出操作回放”，将文件放入应用的测试夹具目录。以下命令在从初始模型启动的模板上重放，包括计时器推进及触点 ID：

~~~
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview -- --replay recorded.trace
~~~

对告警应用，先给 preview 传入字库路径，再传 --replay 与文件路径。回放逐帧比较增量渲染与全屏重绘；用模型、焦点或截图断言额外验证业务结果。每次预览保留最近 16 帧的 PNG，完整事件记录最多 8 MiB，达到上限会明确报错。

在 SDK 根目录运行以下诊断和编辑器回放。两条命令都使用随包资源。

~~~sh
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example diagnostics
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example editor --features enhanced -- --replay docs/hardware/rc1-browser-ime.trace
~~~

诊断示例在同一个 96 × 48 场景内重现三类故障。点 (8, 8) 应当选中按钮 11，命中报告给出挡住按钮的可点击标签 12。将覆盖层的 clickable 设为 false 即可让点击落到按钮。第二帧只修改根节点的 theme_id，报告显示像素变化为零，仍传输了 9,216 B；移除没有视觉变化的版本递增可避免这次整屏发送。最后一次更新耗尽节点容量，错误列出页面 7、键 160 和 Nodes。例子断言上一帧仍然可用，再通过 AppSession::recover 缩小活动窗口并提交视图。

## 从 v0.2 迁移与恢复

两个原有应用、LF4 v1 和旧 .events 回放保留原入口。新应用使用 AppSession::<YourApp>::new 设置模型、FontSet 和显示尺寸。设备使用 Engine::new_in_place_with_fonts，在单调毫秒时钟下依次调用 try_advance、try_dispatch 和 present。

try_message、try_dispatch 和 try_advance 提供事务结果。Busy 会交还尚待处理的消息或输入，在 DMA 完成后重试。WentBackwards 要求宿主修正时间。Build 表示提交失败，活动树和之前完成的帧仍可使用；模型可能已更新。读取 build_failure，按其中的页面、可用的控件键和资源类别修正模型，再调用 commit_view。重复业务消息可能重复执行它的副作用。AppSession::recover 可在桌面执行同样的修复和像素校验。

导航示例在 liuli-alarms/src/lib.rs。Navigator 以 const 容量保存携带参数的页面枚举。确认框关闭后才处理页面返回。离开列表前保存条目键、偏移和焦点，返回时通过 page_restore 提供这些值。运行 workflow 测试可检查整条路径。


## 版本与兼容性

1.0.0 沿用 RC1 已冻结的应用、组件与资源接口。应用、控件、模拟器、资源工具和参考板使用同一个 SDK 版本。升级时先保留原 LF4 和应用回放，再在新 SDK 的供应目录中构建并核对它们。安装包自带 liuli-sim 命令，可在空目录内重放两个 v0.2 应用。打包验收会把 12 幅输出和原始 PPM 逐字节比较。

~~~
tools/bin/liuli-sim replay vendor/liuli-sim-1.0.0/tests/fixtures/settings.events --out shots/settings --format ppm
tools/bin/liuli-sim replay vendor/liuli-sim-1.0.0/tests/fixtures/terminal.events --out shots/terminal --format ppm
~~~

| 接口或数据 | 1.0 兼容约定 |
|---|---|
| 应用 SDK 与正式控件 API | 1.x 中保持已有公开调用和事务行为。破坏已有调用的变更随新主版本发布，并给出迁移示例。 |
| 字体与图片 | LF4 v1、L565 v1 按各自文件头选取解码器。格式变更引入新的版本头。保留旧字库与像素对照。 |
| 操作回放 | 旧 .events 使用原入口。新预览事件在首行声明 liuli-trace 1，时间单位为毫秒。未知版本会返回错误。 |
| 平台后端 | 基础档保持有界存储和零堆。增强特性单独启用。每个支持平台用相同的逻辑坐标与裁剪规则验证。 |
