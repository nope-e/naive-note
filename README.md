# Naive Note

Windows 本地 PDF 手写笔记应用。已接入 MuPDF 连续纵向多页阅读、页码定位和可见页高清分块，以及第 3 步的模拟压感笔迹保存验证：可预览一条变宽笔迹，将标准 Ink 批注、自定义外观和 CBOR＋Zstd 编辑附件导出为独立测试 PDF，再回读校验。2026-11-07 用户已确认多页阅读总体视觉无问题；此前模拟压感导出样本也已获查看确认。DPI、性能、具体交互，以及 Edge、Acrobat 与打印的逐项结果仍待记录。真实笔输入和正式 Ctrl+S 保存尚未实现。

## 构建与运行

需要 Windows x64、Rust 1.95 或更新版本的 MSVC 工具链、Visual Studio 2022 C++ Build Tools（v143 和 Windows SDK）、LLVM/libclang，以及支持 OpenGL 3.3 的显卡驱动。

MuPDF 的绑定生成器需要桌面 LLVM 的 `libclang.dll`。[.cargo/config.toml](.cargo/config.toml) 已用 `force = true` 将 `LIBCLANG_PATH` 固定到本机桌面 LLVM 的 **bin 目录**；换机器时需调整该路径。这样即使终端继承了 ESP/Xtensa 工具链的同名环境变量，Cargo 子进程也使用桌面 LLVM。仅设置普通字符串配置不会覆盖已存在的环境变量，详见 [Cargo env 配置](https://doc.rust-lang.org/cargo/reference/config.html#env)。

本机对应目录为：

```powershell
D:\clang+llvm-x86_64-pc-windows-msvc\bin
```

`mupdf-sys` 通过 MSBuild 编译自带的 MuPDF 源码，不要求 GNU、MSYS2 或单独下载 MuPDF DLL。首次原生构建需要几分钟。项目对 `mupdf-sys` 设置 `opt-level = 3`、`debug = false`，使开发与测试构建也使用与 Skia 预编译包一致的 release CRT，避免 `LNK2038`。

[.cargo/config.toml](.cargo/config.toml) 通过 MSVC 的 `_CL_` 参数启用 MuPDF 上游的 `FZ_HIDE_INTERNAL_JPEG` 命名空间，避免其 libjpeg 与 Skia 的同名符号冲突（`LNK2005`）。如果本机已自定义 `_CL_`，需在原有参数后补上 `/DFZ_HIDE_INTERNAL_JPEG`；Cargo 默认不会覆盖已有环境值。

```powershell
cargo run --locked
```

未打开文档时显示网格测试纸面。通过 **Open PDF / Ctrl+O** 选择文件，或将 PDF 拖入窗口；加载成功后从第一页开始连续纵向显示文档，页码随滚动更新，以画布中心最近的页面作为当前页。取消选择保留当前页面，打开失败显示错误并保留当前已加载页面，悬停状态栏可查看完整消息。也可传入启动路径：

```powershell
cargo run --locked -- 'D:\documents\中文示例.pdf'
```

首次加载适合页面；拖动或手动缩放后，调整窗口保持画布中心对应的纸面位置。所有绘制（含输入、系统重绘和 F12）统一限制为最高 **60 FPS**，保留 vsync；空闲时按需重绘。按 **F12** 显示或隐藏渲染、坐标和帧率信息。F12 栏显示时以最高 60 FPS 连续采样，`FPS` 每半秒更新一次，按实际成功提交的帧间隔计算；首次采样显示 `--`，`/ 60 max` 表示上限。隐藏后恢复按需重绘，最小化或遮挡时暂停绘制并重置采样。

| 操作 | 行为 |
| --- | --- |
| `Open PDF` / Ctrl＋O / 拖入文件 | 后台读取各页几何，从第一页开始连续阅读 |
| `Prev` / `Next` / PageUp / PageDown | 定位上一页 / 下一页 |
| 工具栏 `Page` 数字 / Home / End | 点击数字输入页码并回车 / 定位首页 / 末页 |
| `Close PDF` / Ctrl＋W | 关闭文档、取消待处理结果，返回测试纸面 |
| `Pressure demo` | 在第一页添加或移除一条模拟压感叠加笔迹 |
| `Export test PDF` | 将该笔迹保存到项目 `.scratch/pdf-save-probe/<UUID>/pressure.pdf`，校验后自动打开副本 |
| 滚轮 / Shift＋滚轮 | 纵向 / 横向滚动 |
| 左键或中键拖动中央画布 | 平移纸面 |
| Ctrl＋滚轮 | 以鼠标位置为锚点缩放，范围 10%—800% |
| `Fit page` / Ctrl＋0 | 适合当前页，随窗口尺寸调整 |
| `Fit width` / Ctrl＋2 | 按当前页宽度适合画布，随窗口尺寸调整 |
| `100%` / Ctrl＋1 | 100% 缩放，保持画布中心锚点 |
| `-` / `+` | 以画布中心为锚点缩放 |
| `Rotate 90 deg` | 所有页面顺时针旋转视图 90°，重新适合当前页，不改写 PDF |

100% 时，1 纸面 point（1/72 英寸）对应 96/72 Windows 逻辑像素，再乘 DPI 比例得到物理像素。状态栏显示纸面坐标和原始 PDF 用户空间坐标；MuPDF 的实际页面变换用于读取有效 CropBox、旋转和 UserUnit。

当前支持**连续纵向多页阅读**，使用每页实际 CropBox、旋转和 UserUnit，居中排列，页面之间留 24 pt 间隔。只有可见页才生成底图或高清块；尚未完成的页面先显示白纸，已加载页在补图期间保留底图。页码输入或翻页时，适合模式会重新适合目标页，手动缩放模式保留倍率并定位目标页顶部。页码文本编辑期间不触发 PageUp/PageDown/Home/End 导航。纸面和 PDF 坐标均对应鼠标所在页，页间隙不会显示页面坐标。

单文件上限 256 MiB；每页先按最高 72 DPI、最长边不超过 1024 px 生成低分辨率底图。放大后按 `缩放 × 96/72 × 屏幕 DPI 比例` 计算物理像素密度，向上选择相邻倍率约为 √2 的档位，只补渲可见的 512×512 px 分块，每侧增加 2 px 采样重叠。需求变化后等待 80 ms 启动后台补图；每项结果完成即可返回，旧任务在页面或分块之间取消，结果校验文档与请求编号，缓存键含页号。当前页优先补图，所有可见页共用最多 96 个高清块的需求预算，其余内容保留底图。

整页底图使用 64 MiB LRU，CPU 分块使用 128 MiB LRU，Skia GPU 资源缓存为 128 MiB；引擎只保留最近使用的 4 页显示列表。这些限制不是进程总内存上限，PDF 快照、页面元数据、显示列表、可见图像和临时光栅化缓冲另计。打开时先在后台读取全部页面几何，大文档的初次打开耗时与页数有关。密码保护 PDF 暂不支持，扫描原图的分辨率仍限制细节；已有批注随底图显示。真实手写、批注编辑和正式保存入口尚未接入；Pressure demo 和测试导出仍针对第一页。正在执行的单次 MuPDF 调用不能中途抢占，退出会等待 worker 收尾。

```powershell
cargo build --locked --target x86_64-pc-windows-msvc
cargo check --offline --locked --all-targets
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked
```

Skia 固定为 `0.157.0`，关闭默认 features，开启 `binary-cache + gl + pdf`。上游没有发布该版本的 Windows `ganesh-gl` 单独预编译包，增加 `pdf` 后才能匹配已发布产物。`pdf` 是 Skia 的 PDF 生成后端；已有 PDF 的读取改用 `mupdf 0.8.0`（内置 MuPDF 1.27.2），批注外观与附件的测试导出已接入，正式保存仍待按 [项目计划](PLAN.md) 第 8 步实现。

MuPDF 仅启用 `base14-fonts + system-fonts`，保留 PDF 支持及 Windows 字体回退，关闭绑定的默认附加功能。MuPDF 和 Rust 绑定的许可及来源见 [第三方依赖声明](THIRD_PARTY_NOTICES.md)。

首次构建需要可访问 GitHub 的 `curl.exe` 下载 Skia 预编译包。升级 Skia 或修改 features 时，须重新核对[官方预编译包清单](https://github.com/rust-skia/skia-binaries/releases)。如果没有匹配产物或下载失败，上游脚本会回退到需要 LLVM 等工具的源码编译。依赖和预编译包准备好之后，可以使用 `--offline --locked` 验证本机构建缓存。

## PDF 保存接口样本

以下命令只读写文件和校验 PDF 结构，不创建窗口：

```powershell
cargo run --offline --locked -- --pdf-save-probe
cargo run --offline --locked -- --pdf-save-probe 'D:\documents\示例.pdf'
cargo run --offline --locked -- --pdf-inspect '.scratch\pdf-save-probe\<UUID>\pressure.pdf'
```

不指定源文件时生成一张 A4 空白 `source.pdf`；指定源文件时使用其内存副本。每次使用独立目录，输出 `pressure.pdf` 和 `verification.txt`，保留源文件。`--pdf-inspect` 检查版本、文档和笔迹 UUID、中心线、外观及指纹；缺失或失配时返回失败，普通界面仍可显示 PDF 并在底栏提示忽略编辑数据。GUI 的 `Export test PDF` 使用同一保存接口，导出期间可以关闭或打开其他文档；过时结果不会重新打开旧页，已开始的导出可能仍留下独立副本。

样本采用 97 个模拟采样，压力从轻到重再减轻，最大笔宽为 8 pt；Skia 叠加和 PDF 外观使用同一份序列化轮廓。正式手写、Ctrl+S、另存为、未保存提示及事务替换源文件属于后续阶段。完整操作、结构检查和人工结果表见 [PDF 保存验收](docs/pdf-save-validation.zh-CN.md)。

## 模块

| 路径 | 职责 |
| --- | --- |
| `src/main.rs` | 日志与程序入口 |
| `src/app/` | 窗口生命周期、事件路由、按需重绘和 UI 布局 |
| `src/app/frame_scheduler.rs` | 60 FPS 上限、合并重绘期限；纯时间计算，无窗口操作 |
| `src/app/canvas.rs` | 组织画布布局、路由文档与视图操作、组装 `CanvasScene` |
| `src/app/canvas/document.rs` | PDF 打开/关闭、worker 结果与状态消息、当前文档和预览生命周期 |
| `src/app/canvas/view.rs` | 页面视图状态、适合页面、缩放/平移/旋转、布局重定位及指针坐标换算 |
| `src/app/canvas/toolbar.rs` | 工具栏、快捷键、渲染信息；区分文档操作与视图操作 |
| `src/app/canvas/status_bar.rs` | 页面、缩放和指针坐标的只读状态显示 |
| `src/app/canvas/area.rs` | 中央区域布局、输入归属、拖动和滚轮手势；返回交互结果 |
| `src/app/canvas/pdf_preview.rs` | 高清补图防抖、请求生命周期及过时结果过滤 |
| `src/document/` | 页面几何、连续页面布局、可见页查找、CropBox / 旋转 / UserUnit 与坐标转换 |
| `src/document/stroke.rs` / `src/ink/` | 最小笔迹模型、原始采样、参数快照、可序列化轮廓和模拟压力样本 |
| `src/platform/` | 原生窗口、GL 上下文、窗口 surface、vsync 与交换缓冲 |
| `src/render/` | egui 输入和绘制、共享 GL 状态切换、Skia 帧缓冲包装与资源释放 |
| `src/pdf/` | MuPDF worker、内存会话快照、各页几何、有界显示列表及 RGBA8 预览/分块 |
| `src/pdf/tiles.rs` | 可见分块与 DPI 档位计算、128 MiB CPU 像素 LRU |
| `src/pdf/save.rs` / `src/pdf/edit_data.rs` | Ink 和自定义 AP、CBOR＋Zstd 附件、保存后回读及指纹核验 |
| `src/pdf/probe.rs` | 无窗口生成与检查保存样本，输出限于项目 `.scratch` |

每帧先计算 egui 布局，明确中央画布的逻辑区域、物理区域和 DPI 比例，再由 Skia 绘制工作区和页面，flush 后叠加 egui，最后交换缓冲。MuPDF 对象始终留在一个后台线程，只传拥有所有权的页面信息和像素；请求编号过滤切换或关闭后的旧结果。Skia 缓存当前可见页图像，同时裁剪到中央画布和纸张边界；egui 控件负责输入归属。重绘请求统一合并，并通过 `WaitUntil` 等待帧期限，避免在 UI 线程 sleep 或忙等；最小化与零尺寸窗口暂停绘制。收到 `RedrawRequested` 不再自动请求下一帧，普通窗口位置移动也不触发内容重绘，避免 Windows 原生拖窗循环被连续绘制和 vsync 阻塞（对应 [winit #4708](https://github.com/rust-windowing/winit/issues/4708) 的机制）。缩放、DPI 变化和恢复事件重新触发绘制；连续尺寸事件合并到下一帧，只有实际绘制尺寸变化才重新包装窗口帧缓冲为 Skia surface。拖窗流畅度仍待用户实机验收。

首次显示前直接提交一帧，避免隐藏窗口等待 `RedrawRequested` 而无法出现。退出时依次释放渲染器、GL 上下文和原生窗口。

## 当前验证范围

无窗口测试覆盖坐标数学，以及 MuPDF 的继承页面属性、有效 CropBox、旋转、UserUnit、位图尺寸和行布局、中文路径、文件句柄释放和错误处理；高清部分覆盖 800% 与 100/150/200% DPI 的可见分块选择、视图旋转、预算降档、LRU 淘汰、分块重叠与尺寸、任务替换及文档切换失效。保存部分覆盖笔迹和附件回读、原有内容和附件保留、名称树、重复导出、损坏和版本不支持的附件、外部修改失配及加密/签名拒绝。这些检查不判断渲染外观。多页部分增加混合尺寸与旋转布局、跨页定位与坐标、缩放锚点、逐页结果返回、跨页缓存隔离、底图 LRU 和过时结果过滤的无窗口检查。所有视觉、窗口和实际交互验收按 [AGENTS.md](AGENTS.md) 由用户执行；步骤与预期结果见 [多页阅读手动验收](docs/multipage-validation.zh-CN.md)、[画布手动验收](docs/canvas-validation.zh-CN.md)、[MuPDF 手动验收](docs/mupdf-validation.zh-CN.md) 和 [PDF 保存验收](docs/pdf-save-validation.zh-CN.md)。
