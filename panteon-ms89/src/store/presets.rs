//! Persistence for named profile presets (`<dir>/<name>.toml`).
//!
//! All preset file I/O lives here — the GUI layer only calls this module
//! (AGENTS.md §3: «Персистентность TOML → src/store/»).
//!
//! Two invariants:
//! 1. **Names are validated before they touch the filesystem.** A preset name is a
//!    user-supplied path fragment; without validation `../../evil` escapes the
//!    presets directory (path traversal). Every public entry point funnels through
//!    [`validate_name`] via [`preset_path`] — there is no bypass.
//! 2. **Writes are atomic**: content goes to `<name>.toml.tmp` in the same
//!    directory and is renamed over the target, so a crash never leaves a
//!    half-written preset behind. Overwriting an existing preset is a legal
//!    operation at this layer — the confirmation dialog is the GUI's job.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::model::Profile;
use crate::store::{parse_toml, profile_to_toml_string};
use crate::PanteonError;

/// Maximum preset name length in characters (user-facing limit).
pub const MAX_NAME_CHARS: usize = 64;
/// Maximum preset name length in bytes: the name must survive as a filename
/// even for multi-byte scripts (`<name>.toml.tmp` must stay below 255 bytes).
pub const MAX_NAME_BYTES: usize = 240;

fn invalid(reason: impl Into<String>) -> PanteonError {
    PanteonError::Io(reason.into())
}

/// Human-readable reason for a preset error, for tooltips and the status bar.
/// `PanteonError::Io` carries a complete sentence here, so its `"I/O error: "`
/// prefix would be noise; other variants fall back to their normal rendering.
pub fn error_message(err: &PanteonError) -> String {
    match err {
        PanteonError::Io(msg) => msg.clone(),
        other => other.to_string(),
    }
}

/// Validates a user-supplied preset name.
///
/// Rejected: empty/whitespace-only, longer than [`MAX_NAME_CHARS`] characters
/// (or [`MAX_NAME_BYTES`] bytes), leading dot (hidden files), `..` (traversal),
/// path separators `/` and `\`, `:` (Windows portability) and control characters.
pub fn validate_name(name: &str) -> Result<(), PanteonError> {
    let name = name.trim();

    if name.is_empty() {
        return Err(invalid("Имя профиля не может быть пустым"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "Слишком длинное имя: {} символов (максимум {})",
            name.chars().count(),
            MAX_NAME_CHARS
        )));
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(invalid(format!(
            "Слишком длинное имя для файловой системы (максимум {} байт)",
            MAX_NAME_BYTES
        )));
    }
    // `..` is checked before the leading-dot rule so traversal gets the specific
    // message ("../evil" is rejected for "..", not for being hidden).
    if name.contains("..") {
        return Err(invalid("Имя не может содержать «..»"));
    }
    if name.starts_with('.') {
        return Err(invalid("Имя не может начинаться с точки (скрытые файлы)"));
    }
    for ch in name.chars() {
        match ch {
            '/' => return Err(invalid("Имя не может содержать «/»")),
            '\\' => return Err(invalid("Имя не может содержать «\\»")),
            ':' => return Err(invalid("Имя не может содержать «:»")),
            c if c.is_control() => {
                return Err(invalid("Имя не может содержать управляющие символы"))
            }
            _ => {}
        }
    }
    Ok(())
}

/// Resolves `<dir>/<name>.toml`, validating the name first. The only way to turn
/// a name into a path — every read/write/delete below goes through it.
pub fn preset_path(dir: &Path, name: &str) -> Result<PathBuf, PanteonError> {
    validate_name(name)?;
    Ok(dir.join(format!("{}.toml", name.trim())))
}

/// Lists saved preset names (`*.toml` file stems) in `dir`, sorted.
/// Missing directory, unreadable entries and non-`.toml` files are ignored;
/// atomic-write leftovers (`*.toml.tmp`) have extension `tmp` → never listed.
pub fn list(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().map(|ext| ext == "toml").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !stem.is_empty() {
                        names.push(stem.to_string());
                    }
                }
            }
        }
    }
    names.sort();
    names
}

/// Reads and parses the preset `name` from `dir`.
pub fn load(dir: &Path, name: &str) -> Result<Profile, PanteonError> {
    let path = preset_path(dir, name)?;
    let content = std::fs::read_to_string(&path).map_err(|e| {
        invalid(format!(
            "Не удалось прочитать '{}': {}",
            path.display(),
            e
        ))
    })?;
    parse_toml(&content)
}

/// Atomically writes `profile` as preset `name` in `dir` (creates `dir`).
/// Overwrites an existing preset — the confirmation is the caller's responsibility.
/// Returns the final path.
pub fn save(dir: &Path, name: &str, profile: &Profile) -> Result<PathBuf, PanteonError> {
    let path = preset_path(dir, name)?;
    std::fs::create_dir_all(dir).map_err(|e| {
        invalid(format!(
            "Не удалось создать каталог '{}': {}",
            dir.display(),
            e
        ))
    })?;

    let toml_data = profile_to_toml_string(profile);
    write_atomic(&path, &toml_data)?;
    Ok(path)
}

/// Deletes preset `name`. A second delete is an error (missing file), never a panic.
pub fn delete(dir: &Path, name: &str) -> Result<(), PanteonError> {
    let path = preset_path(dir, name)?;
    std::fs::remove_file(&path).map_err(|e| {
        invalid(format!(
            "Не удалось удалить '{}': {}",
            path.display(),
            e
        ))
    })
}

/// Imports a profile from an arbitrary user-chosen path (native file dialog).
/// The path is not a preset name: it must resolve to an existing file, and only
/// its *content* is parsed — nothing is written next to it.
pub fn import_from_path(path: &Path) -> Result<Profile, PanteonError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        invalid(format!(
            "Не удалось прочитать файл '{}': {}",
            path.display(),
            e
        ))
    })?;
    parse_toml(&content)
}

/// Exports `profile` to an arbitrary user-chosen path (native save dialog),
/// using the same atomic temp-file + rename strategy as [`save`].
pub fn export_to_path(path: &Path, profile: &Profile) -> Result<(), PanteonError> {
    let toml_data = profile_to_toml_string(profile);
    write_atomic(path, &toml_data)
}

/// Writes `content` to `path` via `<path>.tmp` + `fs::rename`.
fn write_atomic(path: &Path, content: &str) -> Result<(), PanteonError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                invalid(format!(
                    "Не удалось создать каталог '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(invalid(format!(
            "Не удалось записать '{}': {}",
            tmp_path.display(),
            e
        )));
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(invalid(format!(
            "Не удалось заменить '{}': {}",
            path.display(),
            e
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Self-cleaning temp directory (no external crates): pid + counter.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "panteon-presets-{}-{}-{}",
                std::process::id(),
                tag,
                n
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        /// Sorted entry names (files only) — used to prove "nothing was created".
        fn entries(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.path)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_file())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn sample_profile(fire_delay_ms: u16) -> Profile {
        let mut profile = Profile::factory_default();
        profile.fire_delay_ms = fire_delay_ms;
        profile.led_brightness = 3;
        profile
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = TempDir::new("roundtrip");
        let profile = sample_profile(37);

        let path = save(dir.path(), "my profile", &profile).expect("save");
        assert_eq!(path, dir.path().join("my profile.toml"));
        assert!(path.exists());

        let loaded = load(dir.path(), "my profile").expect("load");
        assert_eq!(loaded, profile, "save->load must preserve the whole Profile");
    }

    #[test]
    fn overwrite_replaces_content_and_leaves_no_tmp() {
        let dir = TempDir::new("overwrite");

        save(dir.path(), "base", &sample_profile(50)).expect("first save");
        let first = load(dir.path(), "base").expect("load first");
        assert_eq!(first.fire_delay_ms, 50);

        // Overwrite is legal at the store layer (GUI asks for confirmation).
        save(dir.path(), "base", &sample_profile(120)).expect("second save");
        let second = load(dir.path(), "base").expect("load second");
        assert_eq!(second.fire_delay_ms, 120, "content must be replaced");
        assert_ne!(second, first);

        // Exactly one visible file, no leftover temp artifacts.
        assert_eq!(
            dir.entries(),
            vec!["base.toml".to_string()],
            "no .tmp leftovers, no duplicate files"
        );
        assert_eq!(list(dir.path()), vec!["base".to_string()]);
    }

    #[test]
    fn invalid_names_are_rejected_and_create_nothing() {
        let parent = TempDir::new("traversal-parent");
        let dir = parent.path().join("presets");
        std::fs::create_dir_all(&dir).unwrap();

        let long_name = "a".repeat(MAX_NAME_CHARS + 1);
        let bad_names = [
            "../evil",
            "a/b",
            "a\\b",
            "",
            "   ",
            ".hidden",
            "a:b",
            long_name.as_str(),
        ];

        let profile = sample_profile(50);
        for bad in bad_names {
            assert!(
                validate_name(bad).is_err(),
                "validate_name must reject {bad:?}"
            );
            assert!(
                preset_path(&dir, bad).is_err(),
                "preset_path must reject {bad:?}"
            );
            assert!(
                save(&dir, bad, &profile).is_err(),
                "save must reject {bad:?}"
            );
            assert!(
                load(&dir, bad).is_err(),
                "load must reject {bad:?}"
            );
            assert!(
                delete(&dir, bad).is_err(),
                "delete must reject {bad:?}"
            );
        }

        // Nothing was written — neither inside the presets dir nor one level up.
        assert_eq!(list(&dir), Vec::<String>::new());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "presets dir must stay empty"
        );
        assert!(
            !parent.path().join("evil.toml").exists(),
            "path traversal must not escape the presets dir"
        );
    }

    #[test]
    fn list_sorts_and_ignores_non_toml() {
        let dir = TempDir::new("list");
        let profile = sample_profile(50);

        save(dir.path(), "charlie", &profile).expect("save c");
        save(dir.path(), "alpha", &profile).expect("save a");
        save(dir.path(), "bravo", &profile).expect("save b");

        // Noise: other extensions and a stale temp file.
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        std::fs::write(dir.path().join("delta.toml.tmp"), "x").unwrap();
        std::fs::write(dir.path().join("README"), "x").unwrap();

        assert_eq!(
            list(dir.path()),
            vec![
                "alpha".to_string(),
                "bravo".to_string(),
                "charlie".to_string()
            ]
        );
        assert!(list(&dir.path().join("missing-subdir")).is_empty());
    }

    #[test]
    fn delete_removes_and_second_delete_errors() {
        let dir = TempDir::new("delete");
        save(dir.path(), "gone", &sample_profile(50)).expect("save");
        assert!(load(dir.path(), "gone").is_ok());

        delete(dir.path(), "gone").expect("first delete");
        assert!(load(dir.path(), "gone").is_err());
        assert!(list(dir.path()).is_empty());

        // Second delete: an error, not a panic.
        let err = delete(dir.path(), "gone").expect_err("second delete must fail");
        assert!(
            error_message(&err).contains("Не удалось удалить"),
            "unexpected message: {}",
            error_message(&err)
        );
    }

    #[test]
    fn failed_rename_cleans_up_tmp() {
        let dir = TempDir::new("rename-fail");
        // Occupy the target with a directory: the tmp write succeeds, the rename
        // onto a non-empty target-of-another-kind fails -> tmp must be removed.
        std::fs::create_dir_all(dir.path().join("blocked.toml")).unwrap();

        let err = save(dir.path(), "blocked", &sample_profile(50))
            .expect_err("rename onto a directory must fail");
        assert!(
            error_message(&err).contains("Не удалось заменить"),
            "unexpected message: {}",
            error_message(&err)
        );

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("tmp"))
            .collect();
        assert!(leftovers.is_empty(), "tmp file must not survive: {leftovers:?}");
    }

    #[test]
    fn import_export_roundtrip_via_arbitrary_path() {
        let dir = TempDir::new("import-export");
        std::fs::create_dir_all(dir.path()).unwrap();
        let file = dir.path().join("exported-profile.toml");
        let profile = sample_profile(64);

        export_to_path(&file, &profile).expect("export");
        assert!(file.exists());
        // No leftover temp sibling.
        assert_eq!(dir.entries(), vec!["exported-profile.toml".to_string()]);

        let imported = import_from_path(&file).expect("import");
        assert_eq!(imported, profile);

        let err = import_from_path(&dir.path().join("nope.toml")).expect_err("missing file");
        assert!(error_message(&err).contains("Не удалось прочитать файл"));
    }

    #[test]
    fn validate_name_messages_are_human_readable() {
        let cases = [
            ("", "пустым"),
            ("   ", "пустым"),
            (".hidden", "начинаться с точки"),
            ("../evil", "«..»"),
            ("a/b", "«/»"),
            ("a\\b", "«\\»"),
            ("a:b", "«:»"),
            ("a\tb", "управляющие"),
        ];
        for (name, needle) in cases {
            let err = validate_name(name).expect_err(name);
            let msg = error_message(&err);
            assert!(msg.contains(needle), "{name:?} -> {msg:?} (want {needle:?})");
        }

        // Valid names, including Cyrillic and surrounding spaces (trimmed).
        for name in ["base1", "Игровой профиль", "  padded  ", &"b".repeat(MAX_NAME_CHARS)] {
            assert!(validate_name(name).is_ok(), "{name:?} must be valid");
        }
        // Trimming means "  a  " and "a" address the same preset.
        assert_eq!(
            preset_path(Path::new("/tmp/x"), "  a  ").unwrap(),
            Path::new("/tmp/x/a.toml")
        );
    }
}