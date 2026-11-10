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
    let path = directory.0.join("settings.json");
    assert_eq!(read_settings(&path).unwrap(), Settings::default());
    let settings = Settings {
        color: [240, 50, 10],
        width: 7.2,
        smoothing: false,
        autosave_seconds: 5,
        pressure_gamma: 0.5,
        mouse_pen: false,
        highlighter_color: [255, 170, 20],
        highlighter_width: 18.,
        highlighter_pressure_gamma: 0.5,
        cursor: crate::cursor::CursorSettings {
            regular: crate::cursor::CursorStyle {
                shape: crate::cursor::CursorShape::TeardropNorthWest,
                size: crate::cursor::CursorSize::Large,
            },
            drawing: crate::cursor::CursorStyle {
                shape: crate::cursor::CursorShape::Crosshair,
                size: crate::cursor::CursorSize::Small,
            },
            show_drawing: false,
        },
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
        Settings {
            highlighter_width: 0.,
            ..settings.clone()
        },
        Settings {
            highlighter_pressure_gamma: -1.,
            ..settings.clone()
        },
    ] {
        assert!(write_settings(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(read_settings(&path).is_err());
        fs::write(&path, &before).unwrap();
    }
    fs::write(&path, b"corrupt settings").unwrap();
    assert!(read_settings(&path).is_err());
}

#[test]
fn older_settings_get_default_cursors_and_partial_cursor_settings_keep_defaults() {
    let directory = Directory::new();
    let path = directory.0.join("settings.json");
    let mut old = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("cursor");
    old["color"] = serde_json::json!([180, 40, 60]);
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let restored = read_settings(&path).unwrap();
    assert_eq!(restored.color, [180, 40, 60]);
    assert_eq!(restored.cursor, crate::cursor::CursorSettings::default());
    old["cursor"] = serde_json::json!({"show_drawing": false});
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        read_settings(&path).unwrap().cursor,
        crate::cursor::CursorSettings {
            show_drawing: false,
            ..Default::default()
        }
    );
}

#[test]
fn startup_survives_bad_settings_lists_only_owned_drafts_and_flushes_latest_settings() {
    let directory = Directory::new();
    fs::write(directory.0.join("settings.json"), b"corrupt").unwrap();
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
        read_settings(&directory.0.join("settings.json")).unwrap(),
        latest
    );
}

#[test]
fn partial_json_settings_receive_defaults_and_new_defaults_survive_roundtrip() {
    let directory = Directory::new();
    let path = directory.0.join("settings.json");
    fs::write(&path, br#"{"version":1,"autosave_seconds":7}"#).unwrap();
    let settings = read_settings(&path).unwrap();
    assert_eq!(settings.autosave_seconds, 7);
    assert!(settings.auto_expand);
    assert_eq!(
        settings.reference_lines,
        crate::document::ReferenceLines::default()
    );
    let changed = Settings {
        auto_expand: false,
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

#[test]
fn startup_ignores_legacy_cbor_and_saves_only_json() {
    let directory = Directory::new();
    let legacy_path = directory.0.join("settings.cbor");
    let legacy = Settings {
        autosave_seconds: 7,
        ..Settings::default()
    };
    let mut legacy_bytes = Vec::new();
    ciborium::into_writer(&legacy, &mut legacy_bytes).unwrap();
    fs::write(&legacy_path, &legacy_bytes).unwrap();
    let (send, received) = mpsc::channel();
    let worker = SettingsWorker::new(directory.0.clone(), move |response| {
        let _ = send.send(response);
    })
    .unwrap();
    let StorageResponse::Loaded {
        settings, warnings, ..
    } = received.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("expected startup inventory");
    };
    assert_eq!(settings, Settings::default());
    assert!(warnings.is_empty());
    let path = directory.0.join("settings.json");
    assert!(!path.exists());
    worker.save(settings.clone());
    drop(worker);
    assert_eq!(read_settings(&path).unwrap(), settings);
    assert_eq!(fs::read(&legacy_path).unwrap(), legacy_bytes);
}
