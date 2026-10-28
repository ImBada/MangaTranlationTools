//! Font preset backups in `<data>/font-preset-backups/<uuid>.json` (`fontPresetBackups.ts`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::json_io::{read_json_optional, write_json_atomic};
use crate::model::{FontPreset, FontSizePreset, JsonMap};
use crate::paths::AppPaths;
use crate::time::now_iso;

pub const DEFAULT_BACKUP_NAME: &str = "폰트 프리셋 백업";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetBackup {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub font_preset_count: usize,
    #[serde(default)]
    pub font_size_preset_count: usize,
    #[serde(default)]
    pub font_presets: Vec<FontPreset>,
    #[serde(default)]
    pub font_size_presets: Vec<FontSizePreset>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

fn backup_dir(paths: &AppPaths) -> PathBuf {
    paths.font_preset_backups_dir.clone()
}

fn is_backup_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn backup_path(paths: &AppPaths, id: &str) -> CoreResult<PathBuf> {
    if !is_backup_id(id) {
        return Err(CoreError::Invalid("폰트 프리셋 백업 ID가 올바르지 않습니다.".into()));
    }
    Ok(backup_dir(paths).join(format!("{id}.json")))
}

/// Backups newest first (`listFontPresetBackups`); unreadable files are skipped.
pub fn list(paths: &AppPaths) -> CoreResult<Vec<PresetBackup>> {
    let dir = backup_dir(paths);
    std::fs::create_dir_all(&dir).map_err(|source| CoreError::io(&dir, source))?;
    let mut backups: Vec<PresetBackup> = std::fs::read_dir(&dir)
        .map_err(|source| CoreError::io(&dir, source))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?.to_owned();
            read(paths, &id).ok().flatten()
        })
        .collect();
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

pub fn read(paths: &AppPaths, id: &str) -> CoreResult<Option<PresetBackup>> {
    let path = backup_path(paths, id)?;
    Ok(read_json_optional::<PresetBackup>(&path)?.map(|mut backup| {
        backup.font_preset_count = backup.font_presets.len();
        backup.font_size_preset_count = backup.font_size_presets.len();
        backup
    }))
}

pub fn create(
    paths: &AppPaths,
    name: &str,
    font_presets: &[FontPreset],
    font_size_presets: &[FontSizePreset],
) -> CoreResult<PresetBackup> {
    let backup = PresetBackup {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.trim().is_empty() { DEFAULT_BACKUP_NAME.into() } else { name.trim().to_owned() },
        created_at: now_iso(),
        font_preset_count: font_presets.len(),
        font_size_preset_count: font_size_presets.len(),
        font_presets: font_presets.to_vec(),
        font_size_presets: font_size_presets.to_vec(),
        extra: JsonMap::new(),
    };
    write_json_atomic(&backup_path(paths, &backup.id)?, &backup)?;
    Ok(backup)
}

pub fn delete(paths: &AppPaths, id: &str) -> CoreResult<()> {
    let path = backup_path(paths, id)?;
    std::fs::remove_file(&path).map_err(|source| CoreError::io(&path, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_read_delete() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_root(dir.path());
        let preset = FontPreset { id: "p".into(), name: "대사".into(), ..FontPreset::default() };
        let first = create(&paths, "  ", std::slice::from_ref(&preset), &[]).unwrap();
        assert_eq!(first.name, DEFAULT_BACKUP_NAME);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = create(&paths, "두번째", &[], &[]).unwrap();
        let listed = list(&paths).unwrap();
        assert_eq!(listed.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), [second.id.as_str(), first.id.as_str()]);
        assert_eq!(read(&paths, &first.id).unwrap().unwrap().font_presets[0].name, "대사");
        delete(&paths, &first.id).unwrap();
        assert_eq!(list(&paths).unwrap().len(), 1);
        assert!(read(&paths, "../etc/passwd").is_err());
    }
}
