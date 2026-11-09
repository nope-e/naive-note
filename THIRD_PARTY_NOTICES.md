# 第三方依赖说明

本次 PDF 引擎由原计划中的 PDFium 改为 MuPDF。

| 依赖 | 固定版本 / 来源 | 许可 |
| --- | --- | --- |
| `mupdf` Rust 绑定 | [mupdf-rs 0.8.0](https://github.com/messense/mupdf-rs)，Cargo.lock 中锁定 crates.io 包和 SHA256 | AGPL-3.0 |
| `mupdf-sys` | 0.8.0，内置 `fitz/version.h` 标记的 MuPDF 1.27.2 | AGPL-3.0；MuPDF 源文件另有上游许可条款 |
| MuPDF | [Artifex / MuPDF](https://mupdf.com/)，由上述 sys crate 源码编译链接 | 上游提供 GNU AGPL 和商业许可选项，见[官方许可说明](https://mupdf.readthedocs.io/en/latest/license.html) |
| `rfd` 原生文件对话框 | [rfd 0.17.2](https://github.com/PolyMeilex/rfd) | [MIT](licenses/rfd-MIT.txt) |
| `serde` 序列化 | [serde 1.0.229](https://github.com/serde-rs/serde) | MIT OR Apache-2.0 |
| `ciborium` CBOR | [ciborium 0.2.2](https://github.com/enarx/ciborium) | Apache-2.0 |
| `zstd` 编辑附件压缩 | [zstd-rs 0.14.3](https://github.com/gyscos/zstd-rs) | BSD-3-Clause；内置 Zstandard 另有上游许可 |
| `uuid` 文档与笔迹标识 | [uuid 1.29.0](https://github.com/uuid-rs/uuid) | MIT OR Apache-2.0 |
| `sha2` 批注指纹 | [sha2 0.11.1](https://github.com/RustCrypto/hashes) | MIT OR Apache-2.0 |
| `tablet-input-windows` 原生笔输入 | [crates.io 1.0.0](https://crates.io/crates/tablet-input-windows/1.0.0)，Cargo.lock 锁定包校验和 | [MIT](licenses/tablet-input-windows-MIT.txt) |
| `windows` Win32 绑定 | [windows-rs 0.62.2](https://github.com/microsoft/windows-rs) | MIT OR Apache-2.0 |

AGPL 全文见 [licenses/MuPDF-AGPL-3.0.txt](licenses/MuPDF-AGPL-3.0.txt)。接入 MuPDF 没有替项目自动选择或更改根许可证；发布版本需要落实适用许可、完整第三方声明及对应源码的提供方式。MuPDF 包中的第三方库另有各自许可，制作发行包时需从锁定版本的 `mupdf-sys/mupdf` 源码一并汇总。

当前构建关闭 `mupdf` 默认 features，仅启用 `base14-fonts` 和 `system-fonts`，不启用 JavaScript、OCR 或额外文档格式的绑定功能。MuPDF 通过 MSBuild 编译静态库并链接到 EXE，不需要运行时查找 MuPDF DLL。MSVC 运行库和 Windows 系统字体仍需在发行环境验收。

笔输入直接使用 crates.io 发布包 `tablet-input-windows = "=1.0.0"`，不使用本地 path、Git 或 vendor 依赖。`windows` 与该库统一使用 0.62.2 的 HWND/MSG 类型。
