# 第 3 步：PDF 保存关键接口验证

日期：2026-11-06。此阶段验证模拟压力数据的 PDF 表达和回读；真实笔输入、正式保存事务在后续步骤实现。

## 生成和检查样本

在项目目录执行：

```powershell
cargo run --offline --locked -- --pdf-save-probe
```

这条命令不创建窗口，打印独立输出路径 `.scratch/pdf-save-probe/<UUID>/pressure.pdf`。同目录包含 A4 空白 `source.pdf` 和 `verification.txt`。指定源文件时从其内存副本导出，源文件不会被改写：

```powershell
cargo run --offline --locked -- --pdf-save-probe 'D:\documents\中文示例.pdf'
cargo run --offline --locked -- --pdf-inspect '.scratch\pdf-save-probe\<UUID>\pressure.pdf'
```

`--pdf-inspect` 成功时输出页数、文档 UUID、附件版本和笔迹数量；缺失、损坏、版本不支持或指纹失配时以失败退出。它不渲染页面，不判断显示效果。

## 已接入的接口

| 内容 | 保存形式和检查 |
| --- | --- |
| 标准笔迹 | `/Subtype /Ink`；`/InkList` 保存转换到 PDF 用户空间的中心线；`/NM` 保存稳定笔迹 UUID；`/F` 含打印标志 |
| 压感外观 | `/AP /N` 指向 Form XObject；`BBox` 与批注 `Rect` 相同，`Matrix` 为单位矩阵；填充同一份最终变宽轮廓 |
| 原始与处理数据 | 原始时间、可选压力、倾角、旋转、Down/Move/Up，处理后点、样式、算法版本、坐标单位及最终轮廓 |
| 编辑附件 | `naive-note.<文档 UUID>.cbor.zst`，内层 CBOR 格式版本 1，外层 Zstd；每条记录有笔迹 UUID、页码和批注 SHA256 指纹 |
| 回读 | 检查页数、所有页的几何、新批注数量、中心线坐标、打印标志、外观命令及附件逐字段一致性；文件同步后再从磁盘读取检查 |
| 安全输出 | 使用新文件创建模式，每次独立目录；不会覆盖源文件或已有输出；尚未实现正式原子替换和外部修改冲突处理 |
| 重新打开 | 标准 PDF 外观成为 MuPDF 底图；本次模拟叠加清除，避免显示两遍；底栏提示 `Edit data v1: 1 verified stroke(s)` |
| 编辑记录失效 | 缺失时只显示标准 PDF；损坏、版本不支持或指纹失配时提示 `Edit data ignored: ...` 并保留实际 PDF 内容；当前测试导出保守拒绝覆盖失效编辑数据 |

MuPDF Rust 0.8.0 未暴露通用自定义外观 setter，本实现使用其安全 `PdfObject` 和 `add_stream` 接口写入 Form XObject。先完成语义 setter 并调用 `annotation.update()`，再安装自定义 AP；随后主动调用 `page.update()`。测试在保存、重开、再次 update 和重新序列化之后核对 AP 未被替换。相关接口见 [PdfDocument](https://docs.rs/mupdf/0.8.0/mupdf/pdf/struct.PdfDocument.html) 和 [PdfAnnotation](https://docs.rs/mupdf/0.8.0/mupdf/pdf/struct.PdfAnnotation.html)。

该版本绑定的附件 helper 仅读取平坦名称树，因此写入前通过 MuPDF 展开原有 `/Kids`，保留其他附件，再按名称排序写入。回读不修改源名称树。

应用数据的压缩输入、CBOR 解压输出上限各为 8 MiB，Zstd 最大窗口为 8 MiB；这是测试附件层面的限制，不是 PDF 引擎解码与整个进程的内存上限。单个源 PDF 仍限制为 256 MiB；会话额外保留用于导出的不可变源字节快照。

## 无窗口验证

```powershell
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked
cargo build --offline --locked --target x86_64-pc-windows-msvc
```

保存测试不创建窗口、不导出渲染图、不读取像素，也不启动阅读器；结构测试覆盖：

- 0/90/180/270 度旋转、偏移 CropBox、非 1 UserUnit、两页文档和中文输出路径。
- 原有页面内容流、批注、链接、书签和附件保留；`/Kids` 名称树保留和名称排序。
- 同一不可变会话重复导出无重复批注；保存结果再次追加一笔时保留文档 UUID 和原记录。
- 改动 `/NM`、`InkList`、`Rect`、打印标志或 AP 后编辑记录失效；缺失、损坏、未来版本附件仍允许解析 PDF。
- 截断 Zstd、CBOR 尾随数据、超过解压上限的输入被拒绝。
- 加密/签名文档和非法笔迹拒绝导出；源文件和已有输出保持原字节，导出不锁源文件。
- worker 导出之后原会话仍能响应读取；关闭文档会取消尚未开始的排队导出。

## 用户手动步骤和预期结果

按 [AGENTS.md](../AGENTS.md)，全部窗口、显示、阅读器和打印检查由用户执行。

1. 用本应用打开生成的 `source.pdf`，点击 `Pressure demo`。预期只出现一条蓝色波浪笔迹，两端细、中间粗；按钮再次点击可移除。
2. 在 100% 和高倍率下缩放、平移，旋转视图四次。预期轮廓跟随纸面，四次旋转后位置还原。
3. 点击 `Export test PDF`。预期显示保存状态，随后自动打开独立的 `pressure.pdf`，笔迹位置、颜色和粗细与叠加预览一致，底栏显示编辑附件验证成功；源文件保持原样。
4. 关闭后重开该输出。预期只显示原来的一条笔迹，无重复叠加，底栏仍显示 1 条验证通过的编辑记录。
5. 在 Edge 和 Acrobat 各自打开同一个 `pressure.pdf`，分别检查 100%、400% 和打印预览或实际打印。预期蓝色变宽笔迹完整，两端圆滑，没有变成等宽线、缺段或被裁切；记录阅读器版本。
6. 在支持附件面板的阅读器中检查 `naive-note.<UUID>.cbor.zst` 存在；若经外部保存，再用 `--pdf-inspect` 检查数据是否仍完整。附件保留与视觉显示分别记录。
7. 导出时立刻关闭或切换文档。预期旧导出结果不重新打开旧页；已开始的导出可以留下独立测试副本。

## 验收记录

| 项目 | 当前证据 / 结果 |
| --- | --- |
| 结构保存和回读 | 无窗口样本已生成并通过回读；见生成目录内 `verification.txt` |
| 自动检查 | 41 项无窗口测试通过；格式、全目标 Clippy 零警告和 MSVC 构建通过。显式目标首次构建时 Skia 原生构建脚本下载了匹配预编译包，不能将该次构建称为全程断网重建 |
| 用户查看结果 | 2026-11-06 用户反馈“我看过了 没问题”；反馈未细分阅读器版本及打印项目 |
| Edge 显示与打印 | 待逐项记录 |
| Acrobat 显示与打印 | 待逐项记录 |
| 外部保存后的附件保留 | 待用户验收 |
| 窗口及各 DPI 交互 | 待按步骤逐项记录，不能由编译和结构检查推断 |

P0 的保存接口关卡已实现并具备结构证据；是否全部通过仍取决于前序窗口和阅读验收，以及上述跨阅读器与打印记录。
