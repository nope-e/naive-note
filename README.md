# Naive Note

Windows 本地 PDF 手写笔记应用。已接入 MuPDF 第一页只读预览：后台读取 PDF 内存快照并渲染，Skia 在现有画布显示，支持缩放、平移、视图旋转及可见区域按需高清重渲。窗口、显示和实际交互待用户手动验收；书写和保存尚未实现。

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

未打开文档时显示网格测试纸面。通过 **Open PDF / Ctrl+O** 选择文件，或将 PDF 拖入窗口；加载成功后显示第一页、总页数和只读预览状态。取消选择保留当前页面，打开失败显示错误并保留当前已加载页面，悬停状态栏可查看完整消息。也可传入启动路径：

```powershell
cargo run --locked -- 'D:\documents\中文示例.pdf'
```

首次加载适合页面；拖动或手动缩放后，调整窗口保持画布中心对应的纸面位置。所有绘制（含输入、系统重绘和 F12）统一限制为最高 **60 FPS**，保留 vsync；空闲时按需重绘。按 **F12** 显示或隐藏渲染、坐标和帧率信息。F12 栏显示时以最高 60 FPS 连续采样，`FPS` 每半秒更新一次，按实际成功提交的帧间隔计算；首次采样显示 `--`，`/ 60 max` 表示上限。隐藏后恢复按需重绘，最小化或遮挡时暂停绘制并重置采样。

| 操作 | 行为 |
| --- | --- |
| `Open PDF` / Ctrl＋O / 拖入文件 | 后台打开 PDF，显示第一页 |
| `Close PDF` / Ctrl＋W | 关闭文档、取消待处理结果，返回测试纸面 |
| 滚轮 / Shift＋滚轮 | 纵向 / 横向滚动 |
| 左键或中键拖动中央画布 | 平移纸面 |
| Ctrl＋滚轮 | 以鼠标位置为锚点缩放，范围 10%—800% |
| `Fit page` / Ctrl＋0 | 适合页面，随窗口尺寸调整 |
| `100%` / Ctrl＋1 | 100% 缩放，保持画布中心锚点 |
| `-` / `+` | 以画布中心为锚点缩放 |
| `Rotate 90 deg` | 顺时针旋转视图 90°，重新适合页面，不改写 PDF |

100% 时，1 纸面 point（1/72 英寸）对应 96/72 Windows 逻辑像素，再乘 DPI 比例得到物理像素。状态栏显示纸面坐标和原始 PDF 用户空间坐标；MuPDF 的实际页面变换用于读取有效 CropBox、旋转和 UserUnit。

当前范围为**第一页只读预览**：单文件上限 256 MiB；先生成最高 144 DPI、最长边不超过 2048 px 的整页底图。放大后按 `缩放 × 96/72 × 屏幕 DPI 比例` 计算物理像素密度，向上选择相邻倍率约为 √2 的渲染档位，只补渲可见的 512×512 px 分块，每侧增加 2 px 采样重叠。800% 时不再仅拉伸整页底图；原始扫描图片自身的分辨率仍限制细节。

所需分块集合变化后等待 120 ms 再启动后台补图，期间显示底图或当前档位已有分块；平移复用缓存，缩放和 DPI 变化重新计算需求。CPU 分块像素 LRU 与 Skia GPU 资源缓存分别设为 128 MiB，Skia 与 CPU 缓存共享像素而不重复复制；这不是整个进程的内存上限，PDF 快照、MuPDF 显示列表、底图和临时光栅化缓冲另计。单次可见集合最多 96 块，超大可视区会降低档位以适应预算。旧渲染在分块之间取消，结果同时校验文档与请求编号。密码保护文档暂不支持。PDF 已有批注随底图显示，尚无翻页、连续多页、手写、批注编辑或保存入口。正在执行的单次 MuPDF 调用尚不能中途抢占，退出会等待 worker 收尾。

```powershell
cargo build --locked --target x86_64-pc-windows-msvc
cargo check --offline --locked --all-targets
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked
```

Skia 固定为 `0.157.0`，关闭默认 features，开启 `binary-cache + gl + pdf`。上游没有发布该版本的 Windows `ganesh-gl` 单独预编译包，增加 `pdf` 后才能匹配已发布产物。`pdf` 是 Skia 的 PDF 生成后端；已有 PDF 的读取改用 `mupdf 0.8.0`（内置 MuPDF 1.27.2），批注保存仍待按 [项目计划](PLAN.md) 实现。

MuPDF 仅启用 `base14-fonts + system-fonts`，保留 PDF 支持及 Windows 字体回退，关闭绑定的默认附加功能。MuPDF 和 Rust 绑定的许可及来源见 [第三方依赖声明](THIRD_PARTY_NOTICES.md)。

首次构建需要可访问 GitHub 的 `curl.exe` 下载 Skia 预编译包。升级 Skia 或修改 features 时，须重新核对[官方预编译包清单](https://github.com/rust-skia/skia-binaries/releases)。如果没有匹配产物或下载失败，上游脚本会回退到需要 LLVM 等工具的源码编译。依赖和预编译包准备好之后，可以使用 `--offline --locked` 验证本机构建缓存。

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
| `src/document/` | 页面几何、CropBox / 旋转 / UserUnit、纸面与视图坐标转换 |
| `src/platform/` | 原生窗口、GL 上下文、窗口 surface、vsync 与交换缓冲 |
| `src/render/` | egui 输入和绘制、共享 GL 状态切换、Skia 帧缓冲包装与资源释放 |
| `src/pdf/` | MuPDF worker、内存会话快照、第一页几何、显示列表及 RGBA8 预览/分块 |
| `src/pdf/tiles.rs` | 可见分块与 DPI 档位计算、128 MiB CPU 像素 LRU |

每帧先计算 egui 布局，明确中央画布的逻辑区域、物理区域和 DPI 比例，再由 Skia 绘制工作区和页面，flush 后叠加 egui，最后交换缓冲。MuPDF 对象始终留在一个后台线程，只传拥有所有权的页面信息和像素；请求编号过滤切换或关闭后的旧结果。Skia 缓存当前页图像，同时裁剪到中央画布和纸张边界；egui 控件负责输入归属。重绘请求统一合并，并通过 `WaitUntil` 等待帧期限，避免在 UI 线程 sleep 或忙等；最小化与零尺寸窗口暂停绘制。收到 `RedrawRequested` 不再自动请求下一帧，普通窗口位置移动也不触发内容重绘，避免 Windows 原生拖窗循环被连续绘制和 vsync 阻塞（对应 [winit #4708](https://github.com/rust-windowing/winit/issues/4708) 的机制）。缩放、DPI 变化和恢复事件重新触发绘制；连续尺寸事件合并到下一帧，只有实际绘制尺寸变化才重新包装窗口帧缓冲为 Skia surface。拖窗流畅度仍待用户实机验收。

首次显示前直接提交一帧，避免隐藏窗口等待 `RedrawRequested` 而无法出现。退出时依次释放渲染器、GL 上下文和原生窗口。

## 当前验证范围

无窗口测试覆盖坐标数学，以及 MuPDF 的继承页面属性、有效 CropBox、旋转、UserUnit、位图尺寸和行布局、中文路径、文件句柄释放和错误处理；高清部分覆盖 800% 与 100/150/200% DPI 的可见分块选择、视图旋转、预算降档、LRU 淘汰、分块重叠与尺寸、任务替换及文档切换失效。这些检查不判断渲染外观。所有视觉、窗口和实际交互验收按 [AGENTS.md](AGENTS.md) 由用户执行；步骤与预期结果见 [画布手动验收](docs/canvas-validation.zh-CN.md) 和 [MuPDF 手动验收](docs/mupdf-validation.zh-CN.md)。
