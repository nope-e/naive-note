//! Local settings and draft inventory. All disk work runs off the UI thread.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::SystemTime,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::{PenStyle, ReferenceLines};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub autosave_seconds: u64,
    pub color: [u8; 3],
    pub width: f64,
    pub pressure_gamma: f64,
    pub smoothing: bool,
    pub mouse_pen: bool,
    pub reference_lines: ReferenceLines,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            autosave_seconds: 10,
            color: [0, 0, 0],
            width: 4.6,
            pressure_gamma: PenStyle::default().pressure_gamma,
            smoothing: true,
            mouse_pen: true,
            reference_lines: ReferenceLines::default(),
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<()> {
        self.reference_lines.validate()?;
        ensure!(self.version == 1, "不支持的设置版本 {}", self.version);
        ensure!(
            (5..=15).contains(&self.autosave_seconds),
            "无效的自动保存间隔"
        );
        ensure!(
            self.width.is_finite() && (0.1..=100.0).contains(&self.width),
            "无效的笔宽"
        );
        ensure!(
            self.pressure_gamma.is_finite() && (0.0..=10.0).contains(&self.pressure_gamma),
            "无效的压力曲线"
        );
        Ok(())
    }
}

pub fn app_data_dir() -> Result<PathBuf> {
    if cfg!(debug_assertions) {
        return Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".scratch/app-data"));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|path| PathBuf::from(path).join("NaiveNote"))
        .context("找不到本机应用数据目录，草稿自动保存不可用")
}

#[derive(Clone, Debug)]
pub struct DraftCopy {
    pub path: PathBuf,
    pub id: Uuid,
    pub modified: Option<SystemTime>,
    pub bytes: u64,
}

pub fn draft_id(path: &Path) -> Option<Uuid> {
    (path.extension()?.to_str()? == "pdf")
        .then(|| Uuid::parse_str(path.file_stem()?.to_str()?).ok())?
}

fn draft_copies(directory: &Path) -> Result<Vec<DraftCopy>> {
    let mut copies = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = draft_id(&path) else { continue };
        // Do not follow symlinks supplied by something outside this application.
        if !entry.file_type()?.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        copies.push(DraftCopy {
            path,
            id,
            modified: metadata.modified().ok(),
            bytes: metadata.len(),
        });
    }
    copies.sort_by_key(|copy| std::cmp::Reverse(copy.modified));
    Ok(copies)
}

fn read_settings(path: &Path) -> Result<Settings> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 64 * 1024, "设置文件过大");
            let settings: Settings =
                ciborium::from_reader(bytes.as_slice()).context("设置文件损坏")?;
            settings.validate()?;
            Ok(settings)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error.into()),
    }
}

fn write_settings(path: &Path, settings: &Settings) -> Result<()> {
    settings.validate()?;
    let mut bytes = Vec::new();
    ciborium::into_writer(settings, &mut bytes)?;
    super::save_atomic(path, super::fingerprint(path)?, &bytes, |staging| {
        ensure!(read_settings(staging)? == *settings, "设置回读不一致");
        Ok(())
    })?;
    Ok(())
}

pub enum StorageResponse {
    Loaded {
        settings: Settings,
        drafts: Vec<DraftCopy>,
        warnings: Vec<String>,
    },
    SettingsSaved(Result<(), String>),
}

enum Command {
    Save(Settings),
    Shutdown,
}

pub struct SettingsWorker {
    sender: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl SettingsWorker {
    pub fn new(root: PathBuf, deliver: impl Fn(StorageResponse) + Send + 'static) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("local-settings".to_owned())
            .spawn(move || {
                let drafts_dir = root.join("drafts");
                let path = root.join("settings.cbor");
                let mut warnings = Vec::new();
                if let Err(error) = fs::create_dir_all(&drafts_dir) {
                    warnings.push(format!("无法创建本地数据目录：{error}"));
                }
                let settings = read_settings(&path).unwrap_or_else(|error| {
                    warnings.push(format!("{error:#}；已使用默认设置"));
                    Settings::default()
                });
                let drafts = draft_copies(&drafts_dir).unwrap_or_else(|error| {
                    warnings.push(format!("无法读取草稿列表：{error:#}"));
                    Vec::new()
                });
                deliver(StorageResponse::Loaded {
                    settings,
                    drafts,
                    warnings,
                });
                while let Ok(command) = receiver.recv() {
                    let Command::Save(mut settings) = command else {
                        break;
                    };
                    // Collapse slider changes, but flush the last settings before shutdown.
                    let mut shutdown = false;
                    while let Ok(command) = receiver.try_recv() {
                        match command {
                            Command::Save(newer) => settings = newer,
                            Command::Shutdown => {
                                shutdown = true;
                                break;
                            }
                        }
                    }
                    let result = write_settings(&path, &settings)
                        .map_err(|error| format!("设置保存失败：{error:#}"));
                    deliver(StorageResponse::SettingsSaved(result));
                    if shutdown {
                        break;
                    }
                }
            })
            .context("无法启动设置 I/O 任务")?;
        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }

    pub fn save(&self, settings: Settings) {
        let _ = self.sender.send(Command::Save(settings));
    }
}

impl Drop for SettingsWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
