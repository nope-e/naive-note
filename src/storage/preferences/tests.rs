//! Settings/draft filesystem checks; no window or raster rendering.

use super::*;
use std::{sync::mpsc, time::Duration};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".scratch")
            .join(format!("preferences-test-{}", Uuid::new_v4()));
        fs::create_dir_all(path.join("drafts")).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        for directory in [self.0.join("drafts"), self.0.clone()] {
            for entry in fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    fs::remove_file(path).unwrap();
                }
            }
        }
        fs::remove_dir(self.0.join("drafts")).unwrap();
        fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn settings_roundtrip_rejects_corruption_versions_and_out_of_range_values() {
    let directory = Directory::new();
    let path = directory.0.join("settings.cbor");
    assert_eq!(read_settings(&path).unwrap(), Settings::default());
    let settings = Settings {
        color: [240, 50, 10],
        width: 7.2,
        smoothing: false,
        autosave_seconds: 5,
        pressure_gamma: 0.5,
        mouse_pen: false,
        ..Settings::default()
    };
    write_settings(&path, &settings).unwrap();
    assert_eq!(read_settings(&path).unwrap(), settings);
    let before = fs::read(&path).unwrap();
    for invalid in [
        Settings {
            version: 99,
            ..settings.clone()
        },
        Settings {
            autosave_seconds: 4,
            ..settings.clone()
        },
        Settings {
            width: f64::NAN,
            ..settings.clone()
        },
        Settings {
            pressure_gamma: -1.,
            ..settings.clone()
        },
    ] {
        assert!(write_settings(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let mut bytes = Vec::new();
        ciborium::into_writer(&invalid, &mut bytes).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(read_settings(&path).is_err());
        fs::write(&path, &before).unwrap();
    }
    fs::write(&path, b"corrupt settings").unwrap();
    assert!(read_settings(&path).is_err());
}

#[test]
fn startup_survives_bad_settings_lists_only_owned_drafts_and_flushes_latest_settings() {
    let directory = Directory::new();
    fs::write(directory.0.join("settings.cbor"), b"corrupt").unwrap();
    let id = Uuid::new_v4();
    let copy = directory.0.join("drafts").join(format!("{id}.pdf"));
    fs::write(&copy, b"corrupt pdf").unwrap();
    fs::write(directory.0.join("drafts/other.pdf"), b"unowned").unwrap();
    fs::write(directory.0.join("drafts/.naive-note-1.tmp"), b"unfinished").unwrap();
    let (send, received) = mpsc::channel();
    let worker = SettingsWorker::new(directory.0.clone(), move |response| {
        let _ = send.send(response);
    })
    .unwrap();
    let StorageResponse::Loaded {
        settings,
        drafts,
        warnings,
    } = received.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("expected startup inventory");
    };
    assert_eq!(settings, Settings::default());
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].path, copy);
    assert_eq!(drafts[0].id, id);
    assert!(!warnings.is_empty());
    worker.save(Settings {
        autosave_seconds: 15,
        ..Settings::default()
    });
    let latest = Settings {
        autosave_seconds: 7,
        color: [7, 8, 9],
        ..Settings::default()
    };
    worker.save(latest.clone());
    drop(worker);
    assert_eq!(
        read_settings(&directory.0.join("settings.cbor")).unwrap(),
        latest
    );
}

#[test]
fn old_settings_receive_reference_defaults_and_new_defaults_survive_roundtrip() {
    #[derive(serde::Serialize)]
    struct OldSettings {
        version: u32,
        autosave_seconds: u64,
    }
    let mut bytes = Vec::new();
    ciborium::into_writer(
        &OldSettings {
            version: 1,
            autosave_seconds: 7,
        },
        &mut bytes,
    )
    .unwrap();
    let settings: Settings = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(
        settings.reference_lines,
        crate::document::ReferenceLines::default()
    );
    let directory = Directory::new();
    let path = directory.0.join("settings.cbor");
    let changed = Settings {
        reference_lines: crate::document::ReferenceLines {
            pattern: crate::document::ReferencePattern::None,
            width: 32.0,
            color_rgb: [120, 140, 160],
            ..settings.reference_lines
        },
        ..settings
    };
    write_settings(&path, &changed).unwrap();
    assert_eq!(read_settings(&path).unwrap(), changed);
}
