//! JSON file helpers matching `libraryFileIO.ts` (2-space pretty JSON + trailing newline,
//! temp file + rename).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{CoreError, CoreResult};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> CoreResult<T> {
    let bytes = fs::read(path).map_err(|source| CoreError::io(path, source))?;
    serde_json::from_slice(&bytes).map_err(|source| CoreError::json(path, source))
}

/// Reads `path`, returning `None` when it does not exist.
pub fn read_json_optional<T: DeserializeOwned>(path: &Path) -> CoreResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|source| CoreError::json(path, source)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CoreError::io(path, source)),
    }
}

/// Serializes like `JSON.stringify(value, null, 2) + "\n"`: whole-number floats are written
/// without a fractional part so files stay diff-friendly with the TypeScript app.
pub fn to_pretty_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut json = serde_json::to_value(value)?;
    normalize_whole_numbers(&mut json);
    let mut text = serde_json::to_string_pretty(&json)?;
    text.push('\n');
    Ok(text)
}

/// Atomically replaces `path` (write to a sibling temp file, fsync, rename).
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let text = to_pretty_json(value).map_err(|source| CoreError::json(path, source))?;
    write_bytes_atomic(path, text.as_bytes())
}

/// Like `write_json_atomic` without the fsync: the file is never half-written, but the
/// last write may be lost on a power cut. For state that is cheap to lose.
pub fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let text = to_pretty_json(value).map_err(|source| CoreError::json(path, source))?;
    write_bytes(path, text.as_bytes(), false)
}

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    write_bytes(path, bytes, true)
}

fn write_bytes(path: &Path, bytes: &[u8], sync: bool) -> CoreResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| CoreError::io(parent, source))?;
    let temp_path = temp_sibling(path);
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        if sync {
            file.sync_all()?;
        }
        Ok(())
    };
    if let Err(source) = write() {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::io(&temp_path, source));
    }
    fs::rename(&temp_path, path).map_err(|source| {
        let _ = fs::remove_file(&temp_path);
        CoreError::io(path, source)
    })
}

/// Copies `path` to `<path>.native-backup` the first time the native app overwrites it.
pub fn backup_once(path: &Path) -> CoreResult<()> {
    let backup = append_extension(path, "native-backup");
    if backup.exists() || !path.exists() {
        return Ok(());
    }
    fs::copy(path, &backup).map(|_| ()).map_err(|source| CoreError::io(&backup, source))
}

fn temp_sibling(path: &Path) -> PathBuf {
    append_extension(path, &format!("{}.{}.tmp", std::process::id(), uuid::Uuid::new_v4()))
}

fn append_extension(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|name| name.to_os_string()).unwrap_or_default();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

fn normalize_whole_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if number.is_f64()
                && let Some(float) = number.as_f64()
                && float.fract() == 0.0
                && float.abs() < 9_007_199_254_740_992.0
            {
                *value = Value::from(float as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_whole_numbers),
        Value::Object(map) => map.values_mut().for_each(normalize_whole_numbers),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pretty_json_matches_js_stringify_style() {
        let text = to_pretty_json(&json!({ "a": 24.0, "b": 1.18, "c": [1.0, -0.0], "d": "한글" })).unwrap();
        assert_eq!(text, "{\n  \"a\": 24,\n  \"b\": 1.18,\n  \"c\": [\n    1,\n    0\n  ],\n  \"d\": \"한글\"\n}\n");
    }

    #[test]
    fn atomic_write_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/chapter.json");
        write_json_atomic(&path, &json!({ "v": 1 })).unwrap();
        backup_once(&path).unwrap();
        write_json_atomic(&path, &json!({ "v": 2 })).unwrap();
        backup_once(&path).unwrap();

        let current: Value = read_json(&path).unwrap();
        let backup: Value = read_json(&path.with_file_name("chapter.json.native-backup")).unwrap();
        assert_eq!(current["v"], 2);
        assert_eq!(backup["v"], 1, "backup must keep the first version");
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn optional_read_of_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let value: Option<Value> = read_json_optional(&dir.path().join("missing.json")).unwrap();
        assert!(value.is_none());
    }
}
