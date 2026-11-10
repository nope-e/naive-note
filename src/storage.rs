//! Full-copy saves: validate a synced sibling file before replacing the target.

use std::{
    fs::{self, File, OpenOptions},
    hash::{DefaultHasher, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, ensure};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// In-memory change detector, not a persistent or cryptographic identifier.
pub struct FileFingerprint {
    length: u64,
    hash: u64,
}

impl FileFingerprint {
    pub fn of(bytes: &[u8]) -> Self {
        Self::read(&mut std::io::Cursor::new(bytes)).expect("in-memory read")
    }

    fn read(file: &mut impl Read) -> Result<Self> {
        let mut hash = DefaultHasher::new();
        let mut length = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                return Ok(Self {
                    length,
                    hash: hash.finish(),
                });
            }
            hash.write(&buffer[..count]);
            length += count as u64;
        }
    }
}

pub fn fingerprint(path: &Path) -> Result<Option<FileFingerprint>> {
    match File::open(path) {
        Ok(mut file) => Ok(Some(FileFingerprint::read(&mut file)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("无法读取目标文件；请重试或另存为"),
    }
}

pub fn same_file_path(a: &Path, b: &Path) -> bool {
    a == b
        || fs::canonicalize(a)
            .ok()
            .zip(fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
}

struct StagingFile(PathBuf);

impl Drop for StagingFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn save_atomic(
    target: &Path,
    expected: Option<FileFingerprint>,
    bytes: &[u8],
    verify: impl FnOnce(&Path) -> Result<()>,
) -> Result<FileFingerprint> {
    ensure!(
        fingerprint(target)? == expected,
        "目标 PDF 已被外部修改或删除；请另存为以保留当前笔迹"
    );
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);
    let (staging, mut file) = loop {
        let id = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".naive-note-{}-{id}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (StagingFile(path), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .context("无法创建保存暂存文件；请检查目录权限和磁盘空间，或另存为");
            }
        }
    };
    file.write_all(bytes)
        .context("写入 PDF 失败；原文件未替换，请检查磁盘空间")?;
    file.sync_all().context("同步 PDF 到磁盘失败")?;
    drop(file);
    verify(&staging.0).context("保存回读校验失败；原文件未替换")?;

    // Keep a read handle which denies concurrent writers through the commit.
    // DELETE sharing is necessary for Windows' atomic replacement operation.
    let guard = if expected.is_some() {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0x1 | 0x4); // FILE_SHARE_READ | FILE_SHARE_DELETE
        }
        let mut guard = options
            .open(target)
            .context("目标 PDF 被占用或已删除；请重试或另存为")?;
        ensure!(
            !guard.metadata()?.permissions().readonly(),
            "目标 PDF 为只读；请另存为"
        );
        ensure!(
            Some(FileFingerprint::read(&mut guard)?) == expected,
            "目标 PDF 已被外部修改；请另存为"
        );
        Some(guard)
    } else {
        ensure!(
            fingerprint(target)?.is_none(),
            "目标文件已出现；请重新选择另存为路径"
        );
        None
    };
    fs::rename(&staging.0, target)
        .context("无法提交目标 PDF；请检查文件占用和权限，重试或另存为")?;
    drop(guard);
    Ok(FileFingerprint::of(bytes))
}
