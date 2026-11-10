//! Filesystem failure/commit checks only; no windows or rendering.

use super::*;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".scratch");
        fs::create_dir_all(&root).unwrap();
        loop {
            let path = root.join(format!(
                "storage-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("{error}"),
            }
        }
    }

    fn target(&self) -> PathBuf {
        self.0.join("中文 笔记.pdf")
    }

    fn assert_no_staging(&self) {
        assert!(fs::read_dir(&self.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        for entry in fs::read_dir(&self.0).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn creates_then_replaces_a_synced_verified_file_in_the_same_directory() {
    let directory = Directory::new();
    let target = directory.target();
    let saved = save_atomic(&target, None, b"first", |path| {
        assert_eq!(path.parent(), target.parent());
        assert_eq!(fs::read(path)?, b"first");
        assert!(!target.exists());
        Ok(())
    })
    .unwrap();
    assert_eq!(fingerprint(&target).unwrap(), Some(saved));
    let second = save_atomic(&target, Some(saved), b"second", |path| {
        assert_eq!(fs::read(&target)?, b"first");
        assert_eq!(fs::read(path)?, b"second");
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second");
    assert_eq!(fingerprint(&target).unwrap(), Some(second));
    directory.assert_no_staging();
}

#[test]
fn verification_failure_preserves_the_original_and_cleans_staging() {
    let directory = Directory::new();
    let target = directory.target();
    fs::write(&target, b"original").unwrap();
    assert!(
        save_atomic(
            &target,
            fingerprint(&target).unwrap(),
            b"new",
            |_| anyhow::bail!("injected verification failure")
        )
        .is_err()
    );
    assert_eq!(fs::read(target).unwrap(), b"original");
    directory.assert_no_staging();
}

#[test]
fn detects_changes_before_and_during_save_including_new_and_deleted_destinations() {
    let directory = Directory::new();
    let target = directory.target();
    fs::write(&target, b"opened").unwrap();
    let expected = fingerprint(&target).unwrap();
    fs::write(&target, b"external").unwrap();
    assert!(
        save_atomic(&target, expected, b"new", |_| panic!(
            "must reject before staging"
        ))
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), b"external");
    let expected = fingerprint(&target).unwrap();
    assert!(
        save_atomic(&target, expected, b"new", |_| {
            fs::write(&target, b"external during verification")?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), b"external during verification");
    fs::remove_file(&target).unwrap();
    assert!(save_atomic(&target, expected, b"new", |_| Ok(())).is_err());
    assert!(!target.exists());
    assert!(
        save_atomic(&target, None, b"new", |_| {
            fs::write(&target, b"appeared")?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), b"appeared");
    directory.assert_no_staging();
}

#[cfg(windows)]
#[test]
fn readonly_and_incompatible_open_handles_do_not_replace_the_original() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = Directory::new();
    let target = directory.target();
    fs::write(&target, b"original").unwrap();
    let expected = fingerprint(&target).unwrap();
    let original_permissions = fs::metadata(&target).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions).unwrap();
    let readonly_result = save_atomic(&target, expected, b"new", |_| Ok(()));
    fs::set_permissions(&target, original_permissions).unwrap();
    assert!(readonly_result.is_err());
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&target)
        .unwrap();
    assert!(save_atomic(&target, expected, b"new", |_| Ok(())).is_err());
    drop(locked);
    assert_eq!(fs::read(&target).unwrap(), b"original");
    directory.assert_no_staging();
}
