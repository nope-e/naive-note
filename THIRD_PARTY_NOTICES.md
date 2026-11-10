# 第三方依赖说明

本项目原创代码采用 [Apache License 2.0](LICENSE)，许可范围见 [NOTICE](NOTICE)。第三方代码与资源保留各自的许可证和版权声明。

当前 PDF 引擎使用 MuPDF。未取得覆盖 MuPDF 及其 Rust 绑定等相关组件的另行授权时，包含这些依赖的完整应用须按 AGPLv3 的条件分发；原创代码在组合中继续保留 Apache-2.0 许可。

| 依赖 | 固定版本 / 来源 | 许可 |
| --- | --- | --- |
| `mupdf` Rust 绑定 | [mupdf-rs 0.8.0](https://github.com/messense/mupdf-rs)，Cargo.lock 中锁定 crates.io 包和 SHA256 | AGPL-3.0 |
| `mupdf-sys` | 0.8.0，内置 `fitz/version.h` 标记的 MuPDF 1.27.2 | AGPL-3.0；MuPDF 源文件另有上游许可条款 |
| MuPDF | [Artifex / MuPDF](https://mupdf.com/)，由上述 sys crate 源码编译链接 | 上游提供 GNU AGPL 和商业许可选项，见[官方许可说明](https://mupdf.readthedocs.io/en/latest/license.html) |
| `rfd` 原生文件对话框 | [rfd 0.17.2](https://github.com/PolyMeilex/rfd) | [MIT](licenses/rfd-MIT.txt) |
| `serde` 序列化 | [serde 1.0.229](https://github.com/serde-rs/serde) | MIT OR Apache-2.0 |
| `serde_json` 全局设置 | [serde_json 1.0.154](https://github.com/serde-rs/json) | MIT OR Apache-2.0 |
| `ciborium` CBOR | [ciborium 0.2.2](https://github.com/enarx/ciborium) | Apache-2.0 |
| `zstd` 编辑附件压缩 | [zstd-rs 0.14.3](https://github.com/gyscos/zstd-rs) | BSD-3-Clause；内置 Zstandard 另有上游许可 |
| `uuid` 文档与笔迹标识 | [uuid 1.29.0](https://github.com/uuid-rs/uuid) | MIT OR Apache-2.0 |
| `sha2` 批注指纹 | [sha2 0.11.1](https://github.com/RustCrypto/hashes) | MIT OR Apache-2.0 |
| `tablet-input-windows` 原生笔输入 | [crates.io 1.0.0](https://crates.io/crates/tablet-input-windows/1.0.0)，Cargo.lock 锁定包校验和 | [MIT](licenses/tablet-input-windows-MIT.txt) |
| `ink-stroke-modeler-rs` 笔迹建模 | [crates.io 0.1.0](https://crates.io/crates/ink-stroke-modeler-rs/0.1.0)，Cargo.lock 锁定包校验和 | [MIT](licenses/ink-stroke-modeler-rs-MIT.txt) OR [Apache-2.0](licenses/ink-stroke-modeler-rs-Apache-2.0.txt) |
| `windows` Win32 绑定 | [windows-rs 0.62.2](https://github.com/microsoft/windows-rs) | MIT OR Apache-2.0 |
| Lucide 工具栏图标 | [固定版本及来源](assets/icons/lucide/README.md) | [ISC；Feather 衍生图标采用 MIT](assets/icons/lucide/LICENSE) |

AGPL 全文见 [licenses/MuPDF-AGPL-3.0.txt](licenses/MuPDF-AGPL-3.0.txt)。发布包含 MuPDF 的应用时，需要按 AGPLv3 条款向接收者提供完整对应源码（包括构建所需的源码与脚本），并保留 Apache-2.0 及各第三方组件所需的许可证、版权和归属声明。MuPDF 包中的第三方库另有各自许可，制作发行包时需从锁定版本的 `mupdf-sys/mupdf` 源码一并汇总。

当前构建关闭 `mupdf` 默认 features，仅启用 `base14-fonts` 和 `system-fonts`，不启用 JavaScript、OCR 或额外文档格式的绑定功能。MuPDF 通过 MSBuild 编译静态库并链接到 EXE，不需要运行时查找 MuPDF DLL。MSVC 运行库和 Windows 系统字体仍需在发行环境验收。

UI 字体回退在运行时从 Windows 字体目录读取 `msyh.ttc`（中文）与 `seguisym.ttf`（Segoe UI Symbol，用于 egui 内置取色器的剪贴板字符等符号），同时追加到比例和等宽字体族；仓库不附带这两个系统字体文件。字体文件读取失败时记录警告，实际文字与图标显示仍需在目标环境手动验收。

笔输入直接使用 crates.io 发布包 `tablet-input-windows = "=1.0.0"`，不使用本地 path、Git 或 vendor 依赖。`windows` 与该库统一使用 0.62.2 的 HWND/MSG 类型。
