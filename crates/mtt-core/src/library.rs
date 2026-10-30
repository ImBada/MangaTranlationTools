//! Library store (`libraryStore.ts`, `libraryCrud.ts`, `libraryPaths.ts`).
//!
//! Layout under `<data>/library`:
//! `index.json`, `works/<workId>/work.json`, `works/<workId>/chapters/<chapterId>/chapter.json`
//! plus `pages/`, `inpaint/`, `renders/`, `runs/` next to each chapter file.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Deserialize;

use crate::error::{CoreError, CoreResult};
use crate::json_io::{backup_once, read_json_optional, set_aside_broken, write_json_atomic, write_json_replace};
use crate::model::{
    Chapter, ChapterStatus, LibraryIndexFile, PageAnalysisStatus, PageRecord, Work, de_null_as_default,
    de_string_lenient,
};
use crate::paths::sanitize_file_basename;
use crate::time::now_iso;

pub const DEFAULT_WORK_TITLE: &str = "미정 작품";
pub const DEFAULT_CHAPTER_TITLE: &str = "제목없음";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayer {
    Source,
    InpaintMask,
    InpaintResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterSummary {
    pub id: String,
    pub work_id: String,
    pub title: String,
    pub status: ChapterStatus,
    pub page_count: usize,
    pub completed_page_count: usize,
    /// The first page's stored image path (empty without pages); see [`Library::cover_path`].
    pub cover_image: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkSummary {
    pub work: Work,
    pub chapters: Vec<ChapterSummary>,
}

/// A library file that couldn't be read, or was rebuilt, while listing.
#[derive(Debug, Clone, PartialEq)]
pub struct UnreadableEntry {
    /// The damaged file (or, after a rebuild, where it was set aside).
    pub path: PathBuf,
    /// What happened, for the library screen.
    pub message: String,
    /// The file was replaced by one rebuilt from the folders. Reported only by the listing
    /// that repaired it.
    pub rebuilt: bool,
}

/// [`Library::list_all`]: the readable works plus what had to be skipped or rebuilt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryListing {
    pub works: Vec<WorkSummary>,
    pub unreadable: Vec<UnreadableEntry>,
}

/// Clones share the summary cache and the metadata lock.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
    cache: Arc<Mutex<SummaryCache>>,
    /// Held around every read-modify-write of `work.json` and `index.json`: the save
    /// writer, an import and the UI change them from different threads, and an unlocked
    /// write from a stale read would drop the other's change (a new chapter's id, notes).
    metadata: Arc<Mutex<()>>,
}

/// What a file looked like when it was read, to tell whether it changed since.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    len: u64,
}

impl FileStamp {
    fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self { modified: metadata.modified().ok()?, len: metadata.len() })
    }
}

/// Chapter summaries by `chapter.json` path, valid while the file's stamp is unchanged (the
/// library is listed often; a chapter file holds every block). Also which work a chapter
/// belongs to.
#[derive(Debug, Default)]
struct SummaryCache {
    summaries: HashMap<PathBuf, (FileStamp, ChapterSummary)>,
    owners: HashMap<String, String>,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), cache: Arc::default(), metadata: Arc::default() }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, SummaryCache> {
        self.cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn metadata_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.metadata.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The summary of one chapter: from the cache when its file is unchanged, else read.
    fn chapter_summary(&self, work_id: &str, chapter_id: &str) -> CoreResult<Option<ChapterSummary>> {
        if !is_safe_id(work_id) || !is_safe_id(chapter_id) {
            return Ok(None);
        }
        let path = self.chapter_file(work_id, chapter_id);
        let Some(stamp) = FileStamp::of(&path) else { return Ok(None) };
        if let Some((cached, summary)) = self.cache().summaries.get(&path)
            && *cached == stamp
        {
            return Ok(Some(summary.clone()));
        }
        let Some(header) = read_json_optional::<ChapterHeader>(&path)? else { return Ok(None) };
        let summary = ChapterSummary {
            id: chapter_id.to_owned(),
            work_id: work_id.to_owned(),
            title: header.title,
            status: header.status,
            page_count: header.pages.len(),
            completed_page_count: header.pages.iter().filter(|page| page.progress_completed == Some(true)).count(),
            cover_image: header.pages.first().map(|page| page.image_path.clone()).unwrap_or_default(),
            created_at: header.created_at,
            updated_at: header.updated_at,
        };
        self.cache().summaries.insert(path, (stamp, summary.clone()));
        Ok(Some(summary))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    pub fn works_root(&self) -> PathBuf {
        self.root.join("works")
    }

    pub fn work_dir(&self, work_id: &str) -> PathBuf {
        self.works_root().join(work_id)
    }

    pub fn work_file(&self, work_id: &str) -> PathBuf {
        self.work_dir(work_id).join("work.json")
    }

    pub fn chapter_dir(&self, work_id: &str, chapter_id: &str) -> PathBuf {
        self.work_dir(work_id).join("chapters").join(chapter_id)
    }

    pub fn chapter_file(&self, work_id: &str, chapter_id: &str) -> PathBuf {
        self.chapter_dir(work_id, chapter_id).join("chapter.json")
    }

    pub fn read_index(&self) -> CoreResult<LibraryIndexFile> {
        Ok(read_json_optional(&self.index_path())?.unwrap_or_default())
    }

    pub fn write_index(&self, index: &LibraryIndexFile) -> CoreResult<()> {
        write_json_atomic(&self.index_path(), index)
    }

    /// The folder name is the work's identity: a missing or copied `id` in `work.json` must
    /// never redirect writes or deletes to another work.
    pub fn read_work(&self, work_id: &str) -> CoreResult<Option<Work>> {
        if !is_safe_id(work_id) {
            return Ok(None);
        }
        let work: Option<Work> = read_json_optional(&self.work_file(work_id))?;
        Ok(work.map(|mut work| {
            work.id = work_id.to_owned();
            work
        }))
    }

    /// Replaces `work.json` with `work` as given. To change a field of the stored work, use
    /// [`Library::update_work`], which can't lose another thread's change.
    pub fn write_work(&self, work: &Work) -> CoreResult<()> {
        let _lock = self.metadata_lock();
        write_json_atomic(&self.work_file(&work.id), work)
    }

    /// Reads `work.json`, applies `change` and writes it back, all under the metadata lock.
    /// `None` (nothing written) when the work doesn't exist.
    pub fn update_work<R>(&self, work_id: &str, change: impl FnOnce(&mut Work) -> R) -> CoreResult<Option<R>> {
        let _lock = self.metadata_lock();
        self.update_work_locked(work_id, true, change)
    }

    /// [`Library::update_work`] for a caller holding the metadata lock; `sync` flushes the
    /// file to disk before it replaces the old one.
    fn update_work_locked<R>(
        &self,
        work_id: &str,
        sync: bool,
        change: impl FnOnce(&mut Work) -> R,
    ) -> CoreResult<Option<R>> {
        let Some(mut work) = self.read_work_repairing_locked(work_id, &mut Vec::new())? else { return Ok(None) };
        let result = change(&mut work);
        let path = self.work_file(&work.id);
        if sync { write_json_atomic(&path, &work) } else { write_json_replace(&path, &work) }?;
        Ok(Some(result))
    }

    /// Reads `index.json`, applies `change` and writes it back under the metadata lock.
    fn update_index<R>(&self, change: impl FnOnce(&mut LibraryIndexFile) -> R) -> CoreResult<R> {
        let _lock = self.metadata_lock();
        let mut index = self.read_index_repairing_locked(&mut Vec::new())?;
        let result = change(&mut index);
        self.write_index(&index)?;
        Ok(result)
    }

    /// Like [`Library::read_work`], the location is the chapter's identity.
    pub fn read_chapter(&self, work_id: &str, chapter_id: &str) -> CoreResult<Option<Chapter>> {
        if !is_safe_id(work_id) || !is_safe_id(chapter_id) {
            return Ok(None);
        }
        let chapter: Option<Chapter> = read_json_optional(&self.chapter_file(work_id, chapter_id))?;
        Ok(chapter.map(|mut chapter| {
            chapter.id = chapter_id.to_owned();
            chapter.work_id = work_id.to_owned();
            chapter
        }))
    }

    /// Writes `chapter.json` atomically (backing up the TypeScript app's version once) and
    /// bumps the parent work's `updatedAt`.
    pub fn write_chapter(&self, chapter: &Chapter) -> CoreResult<()> {
        self.write_chapter_file(chapter)?;
        let _lock = self.metadata_lock();
        // Only a timestamp: not worth a second disk flush per save.
        self.update_work_locked(&chapter.work_id, false, |work| work.updated_at = chapter.updated_at.clone())?;
        Ok(())
    }

    /// Writes `chapter.json` alone (the caller updates `work.json`) and refreshes its
    /// cached summary from the chapter in memory.
    pub fn write_chapter_file(&self, chapter: &Chapter) -> CoreResult<()> {
        let path = self.chapter_file(&chapter.work_id, &chapter.id);
        backup_once(&path)?;
        write_json_atomic(&path, chapter)?;
        if let Some(stamp) = FileStamp::of(&path) {
            let summary = ChapterSummary {
                id: chapter.id.clone(),
                work_id: chapter.work_id.clone(),
                title: chapter.title.clone(),
                status: chapter.status,
                page_count: chapter.pages.len(),
                completed_page_count: chapter.pages.iter().filter(|page| page.progress_completed == Some(true)).count(),
                cover_image: chapter.pages.first().map(|page| page.image_path.clone()).unwrap_or_default(),
                created_at: chapter.created_at.clone(),
                updated_at: chapter.updated_at.clone(),
            };
            let mut cache = self.cache();
            cache.summaries.insert(path, (stamp, summary));
            cache.owners.insert(chapter.id.clone(), chapter.work_id.clone());
        }
        Ok(())
    }

    pub fn find_chapter_work_id(&self, chapter_id: &str) -> CoreResult<Option<String>> {
        let known = self.cache().owners.get(chapter_id).cloned();
        if let Some(work_id) = known
            && self.chapter_file(&work_id, chapter_id).is_file()
        {
            return Ok(Some(work_id));
        }
        // A work that can't be read doesn't hide the chapters of the others.
        for work_id in self.readable_index(&mut Vec::new())?.work_order {
            if let Ok(Some(work)) = self.read_work(&work_id)
                && work.chapter_order.iter().any(|id| id == chapter_id)
            {
                self.cache().owners.insert(chapter_id.to_owned(), work_id.clone());
                return Ok(Some(work_id));
            }
        }
        Ok(None)
    }

    pub fn open_chapter(&self, chapter_id: &str) -> CoreResult<Chapter> {
        let work_id = self
            .find_chapter_work_id(chapter_id)?
            .ok_or_else(|| CoreError::NotFound("열려는 화를 찾지 못했습니다.".into()))?;
        let mut chapter = self
            .read_chapter(&work_id, chapter_id)?
            .ok_or_else(|| CoreError::NotFound("열려는 화를 찾지 못했습니다.".into()))?;
        chapter.normalize_page_order();
        Ok(chapter)
    }

    /// `listLibrary`: works in index order with lightweight chapter summaries. Works and
    /// chapters that can't be read are left out (see [`Library::list_all`]).
    pub fn list(&self) -> CoreResult<Vec<WorkSummary>> {
        Ok(self.list_all()?.works)
    }

    /// [`Library::list`] plus what it left out, so one damaged file never hides the rest of
    /// the library: a chapter that can't be read is skipped, and an `index.json` or
    /// `work.json` that isn't valid JSON (a power cut can leave it empty) is rebuilt from
    /// the folders, keeping the damaged file next to it as `<name>.bad`.
    pub fn list_all(&self) -> CoreResult<LibraryListing> {
        let mut unreadable = Vec::new();
        let mut works = Vec::new();
        for work_id in self.readable_index(&mut unreadable)?.work_order {
            let work = match self.read_work(&work_id) {
                Ok(Some(work)) => work,
                Ok(None) => continue,
                Err(CoreError::Json { .. }) => {
                    let _lock = self.metadata_lock();
                    match self.read_work_repairing_locked(&work_id, &mut unreadable) {
                        Ok(Some(work)) => work,
                        Ok(None) => continue,
                        Err(error) => {
                            unreadable.push(unreadable_work(&self.work_file(&work_id), &work_id, &error));
                            continue;
                        }
                    }
                }
                Err(error) => {
                    unreadable.push(unreadable_work(&self.work_file(&work_id), &work_id, &error));
                    continue;
                }
            };
            let mut chapters = Vec::new();
            for chapter_id in &work.chapter_order {
                match self.chapter_summary(&work_id, chapter_id) {
                    Ok(Some(summary)) => chapters.push(summary),
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!("skipping unreadable chapter: {error}");
                        unreadable.push(UnreadableEntry {
                            path: self.chapter_file(&work_id, chapter_id),
                            message: format!(
                                "{} › 화 폴더 {chapter_id}: 화 파일을 읽지 못했습니다. ({})",
                                work.title,
                                error_reason(&error)
                            ),
                            rebuilt: false,
                        });
                    }
                }
            }
            let mut cache = self.cache();
            for chapter in &chapters {
                cache.owners.insert(chapter.id.clone(), work_id.clone());
            }
            drop(cache);
            works.push(WorkSummary { work, chapters });
        }
        Ok(LibraryListing { works, unreadable })
    }

    /// `index.json`, rebuilt from the work folders when it isn't valid JSON.
    fn readable_index(&self, unreadable: &mut Vec<UnreadableEntry>) -> CoreResult<LibraryIndexFile> {
        match self.read_index() {
            Err(CoreError::Json { .. }) => {
                let _lock = self.metadata_lock();
                self.read_index_repairing_locked(unreadable)
            }
            other => other,
        }
    }

    /// [`Library::read_index`], repairing a damaged file. The caller holds the metadata lock
    /// (another thread may have repaired it already, so it is read again here).
    fn read_index_repairing_locked(&self, unreadable: &mut Vec<UnreadableEntry>) -> CoreResult<LibraryIndexFile> {
        let error = match self.read_index() {
            Err(error @ CoreError::Json { .. }) => error,
            other => return other,
        };
        let path = self.index_path();
        let mut works: Vec<(String, String)> = Vec::new();
        if self.works_root().is_dir() {
            for entry in read_dir_entries(&self.works_root())? {
                let id = entry.file_name().to_string_lossy().into_owned();
                if is_safe_id(&id) && entry.file_type().is_ok_and(|kind| kind.is_dir()) && self.work_file(&id).is_file()
                {
                    let created_at = self.read_work(&id).ok().flatten().map(|work| work.created_at).unwrap_or_default();
                    works.push((created_at, id));
                }
            }
        }
        // New works are appended to the order, so creation time is the best guess.
        works.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| crate::import::natural_cmp(&a.1, &b.1)));
        let index =
            LibraryIndexFile { work_order: works.into_iter().map(|(_, id)| id).collect(), ..Default::default() };
        let kept = set_aside_broken(&path)?;
        self.write_index(&index)?;
        log::warn!("rebuilt {} from the work folders ({error}); the old file is {}", path.display(), kept.display());
        unreadable.push(UnreadableEntry {
            message: format!(
                "보관함 목록(index.json)을 읽지 못해 작품 폴더로 다시 만들었습니다. 원래 파일은 {}에 있습니다. ({})",
                file_name_of(&kept),
                error_reason(&error)
            ),
            path: kept,
            rebuilt: true,
        });
        Ok(index)
    }

    /// [`Library::read_work`], rebuilding a `work.json` that isn't valid JSON from the
    /// chapter folders (title and notes are kept when they can still be read). The caller
    /// holds the metadata lock.
    fn read_work_repairing_locked(
        &self,
        work_id: &str,
        unreadable: &mut Vec<UnreadableEntry>,
    ) -> CoreResult<Option<Work>> {
        let error = match self.read_work(work_id) {
            Err(error @ CoreError::Json { .. }) => error,
            other => return other,
        };
        let path = self.work_file(work_id);
        let damaged = fs::read(&path).map_err(|source| CoreError::io(&path, source))?;
        let damaged = String::from_utf8_lossy(&damaged);
        let mut chapters: Vec<(String, String)> = Vec::new();
        let chapters_root = self.work_dir(work_id).join("chapters");
        if chapters_root.is_dir() {
            for entry in read_dir_entries(&chapters_root)? {
                let id = entry.file_name().to_string_lossy().into_owned();
                if is_safe_id(&id) && self.chapter_file(work_id, &id).is_file() {
                    let created_at =
                        self.chapter_summary(work_id, &id).ok().flatten().map(|chapter| chapter.created_at);
                    chapters.push((created_at.unwrap_or_default(), id));
                }
            }
        }
        chapters.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| crate::import::natural_cmp(&a.1, &b.1)));
        let now = now_iso();
        let work = Work {
            id: work_id.to_owned(),
            title: salvage_string(&damaged, "title")
                .map(|title| sanitize_title(&title, DEFAULT_WORK_TITLE))
                .unwrap_or_else(|| DEFAULT_WORK_TITLE.to_owned()),
            created_at: chapters
                .iter()
                .map(|(created_at, _)| created_at)
                .find(|value| !value.is_empty())
                .cloned()
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
            translation_notes: salvage_string(&damaged, "translationNotes").unwrap_or_default(),
            chapter_order: chapters.into_iter().map(|(_, id)| id).collect(),
            ..Work::default()
        };
        let kept = set_aside_broken(&path)?;
        write_json_atomic(&path, &work)?;
        log::warn!("rebuilt {} from its chapter folders ({error}); the old file is {}", path.display(), kept.display());
        unreadable.push(UnreadableEntry {
            message: format!(
                "{}: 작품 정보(work.json)를 읽지 못해 화 폴더로 다시 만들었습니다. 원래 파일은 {}에 있습니다. ({})",
                work.title,
                file_name_of(&kept),
                error_reason(&error)
            ),
            path: kept,
            rebuilt: true,
        });
        Ok(Some(work))
    }

    pub fn create_work(&self, title: &str) -> CoreResult<Work> {
        let now = now_iso();
        let work = Work {
            id: uuid::Uuid::new_v4().to_string(),
            title: sanitize_title(title, DEFAULT_WORK_TITLE),
            chapter_order: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            ..Work::default()
        };
        self.update_index(|index| index.work_order.push(work.id.clone()))?;
        self.write_work(&work)?;
        Ok(work)
    }

    pub fn rename_work(&self, work_id: &str, title: &str) -> CoreResult<()> {
        self.update_work(work_id, |work| {
            work.title = sanitize_title(title, DEFAULT_WORK_TITLE);
            work.updated_at = now_iso();
        })?
        .ok_or_else(|| CoreError::NotFound("작품을 찾지 못했습니다.".into()))
    }

    pub fn rename_chapter(&self, chapter_id: &str, title: &str) -> CoreResult<()> {
        let mut chapter = self.open_chapter(chapter_id)?;
        chapter.title = self.make_unique_chapter_title(
            &chapter.work_id,
            &sanitize_title(title, DEFAULT_CHAPTER_TITLE),
            Some(chapter_id),
        )?;
        chapter.updated_at = now_iso();
        self.write_chapter(&chapter)
    }

    /// `deleteWork`: removes the work from the index and deletes its directory. Like the TS
    /// app it refuses ids without a readable `work.json`.
    pub fn delete_work(&self, work_id: &str) -> CoreResult<()> {
        if self.read_work(work_id)?.is_none() {
            return Err(CoreError::NotFound("작품을 찾지 못했습니다.".into()));
        }
        self.update_index(|index| index.work_order.retain(|id| id != work_id))?;
        remove_dir_if_exists(&self.work_dir(work_id))
    }

    pub fn delete_chapter(&self, chapter_id: &str) -> CoreResult<()> {
        if !is_safe_id(chapter_id) {
            return Err(CoreError::NotFound("화를 찾지 못했습니다.".into()));
        }
        let work_id = self
            .find_chapter_work_id(chapter_id)?
            .ok_or_else(|| CoreError::NotFound("화를 찾지 못했습니다.".into()))?;
        self.update_work(&work_id, |work| {
            work.chapter_order.retain(|id| id != chapter_id);
            work.updated_at = now_iso();
        })?
        .ok_or_else(|| CoreError::NotFound("작품을 찾지 못했습니다.".into()))?;
        remove_dir_if_exists(&self.chapter_dir(&work_id, chapter_id))
    }

    /// `reorderChapters`: stores `work.chapterOrder` reordered by `chapter_ids` (see [`reorder_ids`]).
    pub fn reorder_chapters(&self, work_id: &str, chapter_ids: &[String]) -> CoreResult<()> {
        self.update_work(work_id, |work| {
            work.chapter_order = reorder_ids(&work.chapter_order, chapter_ids);
            work.updated_at = now_iso();
        })?
        .ok_or_else(|| CoreError::NotFound("작품을 찾지 못했습니다.".into()))
    }

    /// `reorderPages`: [`reorder_chapter_pages`] on the stored chapter, then saves it.
    pub fn reorder_pages(&self, chapter_id: &str, page_ids: &[String]) -> CoreResult<Chapter> {
        let (_, mut chapter) = self.read_located_chapter(chapter_id)?;
        reorder_chapter_pages(&mut chapter, page_ids);
        self.write_chapter(&chapter)?;
        Ok(chapter)
    }

    /// `deletePage`: [`remove_chapter_page`] on the stored chapter, saves it, then deletes the
    /// page's files ([`Library::remove_page_files`]). An unknown page id changes nothing.
    pub fn delete_page(&self, chapter_id: &str, page_id: &str) -> CoreResult<Chapter> {
        let (work_id, mut chapter) = self.read_located_chapter(chapter_id)?;
        let Some(removed) = remove_chapter_page(&mut chapter, page_id) else {
            return Ok(chapter);
        };
        self.write_chapter(&chapter)?;
        self.remove_page_files(&work_id, chapter_id, &removed)?;
        Ok(chapter)
    }

    /// File cleanup half of `deletePage`, run after the chapter without `page` was saved:
    /// unlinks the page's image (errors ignored, like `safeUnlink`) and then
    /// [`Library::remove_page_artifacts`]. Rendered pages in `renders/` are kept, as in TS.
    ///
    /// The image is looked up by file name in this chapter's `pages/` folder, never at the
    /// stored absolute `imagePath`: after the data folder was copied or moved, that path
    /// points into the other library.
    pub fn remove_page_files(&self, work_id: &str, chapter_id: &str, page: &PageRecord) -> CoreResult<()> {
        if !is_safe_id(work_id) || !is_safe_id(chapter_id) {
            return Ok(());
        }
        if let Some(name) = stored_file_name(&page.image_path).filter(|name| is_safe_id(name)) {
            safe_unlink(&self.chapter_dir(work_id, chapter_id).join("pages").join(name));
        }
        self.remove_page_artifacts(work_id, chapter_id, &page.id)
    }

    /// `removePageArtifacts`: deletes regular files in `inpaint/` whose name starts with `page_id`
    /// and every `runs/<run>/pages/<page_id>` entry. Ids that are not a single path component are
    /// skipped so a corrupt id can never widen the recursive delete.
    pub fn remove_page_artifacts(&self, work_id: &str, chapter_id: &str, page_id: &str) -> CoreResult<()> {
        if ![work_id, chapter_id, page_id].into_iter().all(is_safe_id) {
            log::warn!("skipping artifact cleanup for unsafe id {work_id:?}/{chapter_id:?}/{page_id:?}");
            return Ok(());
        }
        let chapter_dir = self.chapter_dir(work_id, chapter_id);
        let inpaint_root = chapter_dir.join("inpaint");
        if inpaint_root.exists() {
            for entry in read_dir_entries(&inpaint_root)? {
                let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
                if is_file && entry.file_name().to_string_lossy().starts_with(page_id) {
                    safe_unlink(&entry.path());
                }
            }
        }

        let runs_root = chapter_dir.join("runs");
        if !runs_root.exists() {
            return Ok(());
        }
        for run in read_dir_entries(&runs_root)? {
            if !run.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let target = run.path().join("pages").join(page_id);
            if target.exists() {
                remove_path_all(&target)?;
            }
        }
        Ok(())
    }

    /// Locates and reads `chapter.json` as stored (no page-order normalization), returning the
    /// owning work id alongside it.
    fn read_located_chapter(&self, chapter_id: &str) -> CoreResult<(String, Chapter)> {
        let not_found = || CoreError::NotFound("화를 찾지 못했습니다.".into());
        let work_id = self.find_chapter_work_id(chapter_id)?.ok_or_else(not_found)?;
        let chapter = self.read_chapter(&work_id, chapter_id)?.ok_or_else(not_found)?;
        Ok((work_id, chapter))
    }

    pub fn make_unique_chapter_title(
        &self,
        work_id: &str,
        desired: &str,
        exclude_chapter_id: Option<&str>,
    ) -> CoreResult<String> {
        Ok(unique_title(desired, &self.titles_except(work_id, exclude_chapter_id)?))
    }

    /// Titles of the work's chapters (to pick unused ones).
    pub fn chapter_titles(&self, work_id: &str) -> CoreResult<HashSet<String>> {
        self.titles_except(work_id, None)
    }

    fn titles_except(&self, work_id: &str, excluded: Option<&str>) -> CoreResult<HashSet<String>> {
        let work =
            self.read_work(work_id)?.ok_or_else(|| CoreError::NotFound("선택한 작품을 찾지 못했습니다.".into()))?;
        let mut used = HashSet::new();
        for chapter_id in work.chapter_order.iter().filter(|id| Some(id.as_str()) != excluded) {
            // An unreadable chapter can't clash with a title.
            if let Ok(Some(summary)) = self.chapter_summary(work_id, chapter_id) {
                used.insert(summary.title);
            }
        }
        Ok(used)
    }

    /// The image of a chapter's first page, for the library's covers.
    pub fn cover_path(&self, summary: &ChapterSummary) -> Option<PathBuf> {
        self.source_path(&summary.work_id, &summary.id, &summary.cover_image)
    }

    /// A page's source image: where it was stored, else by file name in `pages/` (the stored
    /// path may come from another OS, `C:\…\pages\x.png`).
    fn source_path(&self, work_id: &str, chapter_id: &str, image_path: &str) -> Option<PathBuf> {
        if let Some(path) = non_empty_path(image_path).filter(|path| path.is_file()) {
            return Some(path);
        }
        stored_file_name(image_path)
            .filter(|name| is_safe_id(name))
            .map(|name| self.chapter_dir(work_id, chapter_id).join("pages").join(name))
            .filter(|path| path.is_file())
    }

    /// Path of a page layer image, tolerating a moved data directory: stored absolute paths
    /// are tried first, then the conventional location inside the chapter directory.
    pub fn resolve_page_layer_path(&self, chapter: &Chapter, page: &PageRecord, layer: PageLayer) -> Option<PathBuf> {
        let stored = match layer {
            PageLayer::Source => return self.source_path(&chapter.work_id, &chapter.id, &page.image_path),
            PageLayer::InpaintMask => page.inpaint_mask_path.as_deref(),
            PageLayer::InpaintResult => page.inpaint_result_path.as_deref(),
        };
        // Only fall back for inpaint layers the page claims to have; otherwise a stale file
        // left behind by a cleared layer would reappear.
        let stored = stored?;
        if let Some(path) = non_empty_path(stored).filter(|path| path.is_file()) {
            return Some(path);
        }
        Some(self.inpaint_layer_path(chapter, &page.id, layer)).filter(|path| path.is_file())
    }

    /// Conventional `inpaint/<pageId>-mask.png` / `-result.png` location.
    pub fn inpaint_layer_path(&self, chapter: &Chapter, page_id: &str, layer: PageLayer) -> PathBuf {
        let suffix = match layer {
            PageLayer::InpaintMask => "mask",
            PageLayer::InpaintResult => "result",
            PageLayer::Source => "source",
        };
        self.chapter_dir(&chapter.work_id, &chapter.id)
            .join("inpaint")
            .join(format!("{}-{suffix}.png", sanitize_file_basename(page_id, "page")))
    }

    /// `renders/<page name>.png` (`saveRenderedPage`). Pages whose names make the same file
    /// (`001.jpg` and `001.png`, `1권/001.jpg` and `2권/001.jpg`, or names differing only in
    /// case) get ` (2)`, ` (3)`… in page order, so exporting every page never overwrites one
    /// page with another, and exporting one page writes the same file as exporting all.
    pub fn render_output_path(&self, chapter: &Chapter, page: &PageRecord) -> PathBuf {
        self.chapter_dir(&chapter.work_id, &chapter.id)
            .join("renders")
            .join(format!("{}.png", render_stem(chapter, page)))
    }
}

/// File name (without `.png`) of `page`'s render; see [`Library::render_output_path`].
fn render_stem(chapter: &Chapter, page: &PageRecord) -> String {
    let stems: Vec<String> = chapter.pages.iter().map(|page| sanitize_file_basename(&page.name, &page.id)).collect();
    // Compared case-insensitively: macOS and Windows file systems are.
    let own: HashSet<String> = stems.iter().map(|stem| stem.to_lowercase()).collect();
    let mut used = HashSet::new();
    for (other, stem) in chapter.pages.iter().zip(&stems) {
        let mut name = stem.clone();
        let mut n = 1;
        // A numbered name must not take another page's own name either.
        while used.contains(&name.to_lowercase()) || (n > 1 && own.contains(&name.to_lowercase())) {
            n += 1;
            name = format!("{stem} ({n})");
        }
        if other.id == page.id {
            return name;
        }
        used.insert(name.to_lowercase());
    }
    sanitize_file_basename(&page.name, &page.id)
}

/// `resolveChapterStatus`.
/// `desired`, or `desired (n)` with the first `n` not in `used`.
pub fn unique_title(desired: &str, used: &HashSet<String>) -> String {
    if !used.contains(desired) {
        return desired.to_owned();
    }
    let mut index = 1;
    while used.contains(&format!("{desired} ({index})")) {
        index += 1;
    }
    format!("{desired} ({index})")
}

pub fn resolve_chapter_status(pages: &[PageRecord]) -> ChapterStatus {
    if pages.is_empty() {
        return ChapterStatus::Idle;
    }
    let all = |status| pages.iter().all(|page| page.analysis_status == status);
    let any = |status| pages.iter().any(|page| page.analysis_status == status);
    if all(PageAnalysisStatus::Completed) {
        ChapterStatus::Completed
    } else if any(PageAnalysisStatus::Running) {
        ChapterStatus::Running
    } else if all(PageAnalysisStatus::Failed) {
        ChapterStatus::Failed
    } else if any(PageAnalysisStatus::Completed) {
        ChapterStatus::Partial
    } else {
        ChapterStatus::Idle
    }
}

/// `reorderIds`: ids of `next_order` that exist in `current_order` (in requested order, duplicates
/// kept), followed by the remaining current ids in their existing order.
pub fn reorder_ids(current_order: &[String], next_order: &[String]) -> Vec<String> {
    let current: std::collections::HashSet<&str> = current_order.iter().map(String::as_str).collect();
    let filtered: Vec<&String> = next_order.iter().filter(|id| current.contains(id.as_str())).collect();
    let placed: std::collections::HashSet<&str> = filtered.iter().map(|id| id.as_str()).collect();
    filtered.into_iter().chain(current_order.iter().filter(|id| !placed.contains(id.as_str()))).cloned().collect()
}

/// Drop side for [`reorder_by_target`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DropPosition {
    #[default]
    Before,
    After,
}

/// `reorderByTarget` (client `editorUtils.ts`): moves `source_id` next to `target_id`. The order is
/// returned unchanged when either id is missing or both are the same.
pub fn reorder_by_target(
    current_order: &[String],
    source_id: &str,
    target_id: &str,
    position: DropPosition,
) -> Vec<String> {
    let mut next = current_order.to_vec();
    let has_target = source_id != target_id && next.iter().any(|id| id == target_id);
    let Some(source_index) = next.iter().position(|id| id == source_id).filter(|_| has_target) else {
        return next;
    };
    let item = next.remove(source_index);
    // The ids differ, so removing the source cannot remove the target.
    let target_index = next.iter().position(|id| id == target_id).unwrap_or(next.len());
    let insert_at = if position == DropPosition::After { target_index + 1 } else { target_index };
    next.insert(insert_at, item);
    next
}

/// In-memory part of `reorderPages`: reorders `page_order` by `page_ids` ([`reorder_ids`]), sorts
/// `pages` to match, bumps `updated_at` and recomputes `status`. The caller saves the chapter.
pub fn reorder_chapter_pages(chapter: &mut Chapter, page_ids: &[String]) {
    chapter.page_order = reorder_ids(&chapter.page_order, page_ids);
    chapter.apply_page_order();
    chapter.updated_at = now_iso();
    chapter.status = resolve_chapter_status(&chapter.pages);
}

/// In-memory part of `deletePage`: drops the page from `pages`/`page_order`, points
/// `last_opened_page_id` at the first remaining `page_order` entry when it was the deleted page,
/// bumps `updated_at` and recomputes `status`. Returns the removed page for
/// [`Library::remove_page_files`], or `None` (chapter untouched) when no page has that id.
pub fn remove_chapter_page(chapter: &mut Chapter, page_id: &str) -> Option<PageRecord> {
    let index = chapter.pages.iter().position(|page| page.id == page_id)?;
    let removed = chapter.pages.remove(index);
    chapter.pages.retain(|page| page.id != page_id);
    chapter.page_order.retain(|id| id != page_id);
    if chapter.last_opened_page_id.as_deref() == Some(page_id) {
        chapter.last_opened_page_id = chapter.page_order.first().cloned();
    }
    chapter.updated_at = now_iso();
    chapter.status = resolve_chapter_status(&chapter.pages);
    Some(removed)
}

pub fn sanitize_title(title: &str, fallback: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() { fallback.to_owned() } else { trimmed.to_owned() }
}

fn non_empty_path(value: &str) -> Option<PathBuf> {
    (!value.trim().is_empty()).then(|| PathBuf::from(value))
}

/// A work file that couldn't be read at all.
fn unreadable_work(path: &Path, work_id: &str, error: &CoreError) -> UnreadableEntry {
    log::warn!("skipping unreadable work: {error}");
    UnreadableEntry {
        path: path.to_path_buf(),
        message: format!("작품 폴더 {work_id}: 작품 정보를 읽지 못했습니다. ({})", error_reason(error)),
        rebuilt: false,
    }
}

/// The error without the file path (shown separately).
fn error_reason(error: &CoreError) -> String {
    match error {
        CoreError::Io { source, .. } => source.to_string(),
        CoreError::Json { source, .. } => format!("JSON 형식 오류: {source}"),
        other => other.to_string(),
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The string value of `"key"` in a damaged JSON file, when the file still has it (a
/// truncated file keeps its first keys).
fn salvage_string(damaged: &str, key: &str) -> Option<String> {
    let rest = &damaged[damaged.find(&format!("\"{key}\""))? + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?;
    serde_json::Deserializer::from_str(rest).into_iter::<String>().next()?.ok()
}

/// Last component of a stored path, whichever OS wrote it (`C:\…\pages\x.png` or
/// `/…/pages/x.png`).
fn stored_file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).next().filter(|name| !name.is_empty())
}

fn remove_dir_if_exists(path: &Path) -> CoreResult<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CoreError::io(path, source)),
    }
}

/// `safeUnlink`: removes a file, ignoring every error.
fn safe_unlink(path: &Path) {
    let _ = fs::remove_file(path);
}

/// `rm(path, { recursive: true, force: true })`: removes a file, symlink or directory tree.
fn remove_path_all(path: &Path) -> CoreResult<()> {
    let result = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CoreError::io(path, source)),
    }
}

fn read_dir_entries(path: &Path) -> CoreResult<Vec<fs::DirEntry>> {
    fs::read_dir(path)
        .and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>())
        .map_err(|source| CoreError::io(path, source))
}

/// Minimal view of `chapter.json` for listings; skips block data.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterHeader {
    #[serde(default, deserialize_with = "de_string_lenient")]
    title: String,
    #[serde(default)]
    status: ChapterStatus,
    #[serde(default, deserialize_with = "de_null_as_default")]
    pages: Vec<PageHeader>,
    #[serde(default, deserialize_with = "de_string_lenient")]
    created_at: String,
    #[serde(default, deserialize_with = "de_string_lenient")]
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageHeader {
    #[serde(default)]
    progress_completed: Option<bool>,
    #[serde(default, deserialize_with = "de_string_lenient")]
    image_path: String,
}

/// Ids are folder names and must stay one plain path component on every OS: non-empty,
/// not `.`/`..`, no path separators, no `:` (on Windows `base.join("C:")` replaces `base`
/// with the drive's current folder) and no control characters.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.chars().any(|ch| matches!(ch, '/' | '\\' | ':') || ch.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PageRecord;

    fn sample_chapter(library: &Library, work: &Work) -> Chapter {
        let chapter = Chapter {
            id: "c1".into(),
            work_id: work.id.clone(),
            title: "1화".into(),
            page_order: vec!["p2".into(), "p1".into()],
            pages: vec![
                PageRecord { id: "p1".into(), progress_completed: Some(true), ..PageRecord::default() },
                PageRecord { id: "p2".into(), ..PageRecord::default() },
            ],
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            ..Chapter::default()
        };
        let mut work = library.read_work(&work.id).unwrap().unwrap();
        work.chapter_order.push(chapter.id.clone());
        library.write_work(&work).unwrap();
        library.write_chapter(&chapter).unwrap();
        chapter
    }

    #[test]
    fn create_list_open_rename_delete() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("  ").unwrap();
        assert_eq!(work.title, DEFAULT_WORK_TITLE);
        sample_chapter(&library, &work);

        let listed = library.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].chapters[0].page_count, 2);
        assert_eq!(listed[0].chapters[0].completed_page_count, 1);
        assert_eq!(library.read_work(&work.id).unwrap().unwrap().updated_at, "2026-01-01T00:00:00.000Z");

        let opened = library.open_chapter("c1").unwrap();
        assert_eq!(opened.page_order, ["p2", "p1"]);
        assert_eq!(opened.pages[0].id, "p2");

        library.rename_chapter("c1", " 2화 ").unwrap();
        assert_eq!(library.open_chapter("c1").unwrap().title, "2화");

        library.delete_chapter("c1").unwrap();
        assert!(library.open_chapter("c1").is_err());
        library.delete_work(&work.id).unwrap();
        assert!(library.list().unwrap().is_empty());
    }

    #[test]
    fn listing_reuses_summaries_until_the_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        let mut chapter = sample_chapter(&library, &work);
        assert_eq!(library.list().unwrap()[0].chapters[0].title, "1화");
        // A save through the library updates the summary without a re-read.
        chapter.title = "저장됨".into();
        library.write_chapter(&chapter).unwrap();
        assert_eq!(library.list().unwrap()[0].chapters[0].title, "저장됨");
        // A change from outside (another app version) has a new size, so it is read again.
        let path = library.chapter_file(&work.id, &chapter.id);
        let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json["title"] = "바깥에서 바뀜".into();
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(library.list().unwrap()[0].chapters[0].title, "바깥에서 바뀜");
        assert_eq!(library.find_chapter_work_id(&chapter.id).unwrap(), Some(work.id.clone()));
    }

    #[test]
    fn a_chapter_saved_with_nan_opens_again() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        let mut chapter = sample_chapter(&library, &work);
        chapter.pages[0].blocks.push(crate::model::TranslationBlock { opacity: f64::NAN, ..Default::default() });
        library.write_chapter(&chapter).unwrap();
        assert_eq!(library.open_chapter("c1").unwrap().pages[1].blocks[0].opacity, 1.0);
        // A file written by the TS app (or an older build) with `null` in it.
        let path = library.chapter_file(&work.id, "c1");
        let text = fs::read_to_string(&path).unwrap().replace("\"opacity\": 1", "\"opacity\": null");
        fs::write(&path, text.replace("\"title\": \"1화\"", "\"title\": null")).unwrap();
        assert_eq!(library.open_chapter("c1").unwrap().pages[1].blocks[0].opacity, 1.0);
        assert_eq!(library.list().unwrap()[0].chapters[0].title, "");
    }

    #[test]
    fn concurrent_work_updates_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        let chapter = sample_chapter(&library, &work);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // The save writer bumps `updatedAt` while imports add chapters.
        let saver = {
            let (library, stop) = (library.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    library.write_chapter(&chapter).unwrap();
                }
            })
        };
        let importers: Vec<_> = (0..4)
            .map(|thread| {
                let (library, work_id) = (library.clone(), work.id.clone());
                std::thread::spawn(move || {
                    for n in 0..20 {
                        library
                            .update_work(&work_id, |work| work.chapter_order.push(format!("t{thread}-{n}")))
                            .unwrap();
                    }
                })
            })
            .collect();
        importers.into_iter().for_each(|thread| thread.join().unwrap());
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        saver.join().unwrap();
        let order = library.read_work(&work.id).unwrap().unwrap().chapter_order;
        assert_eq!(order.len(), 1 + 4 * 20, "no chapter id lost");
    }

    #[test]
    fn one_damaged_file_does_not_hide_the_rest_of_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let first = library.create_work("첫째").unwrap();
        sample_chapter(&library, &first);
        let mut broken = Chapter { id: "c2".into(), work_id: first.id.clone(), ..Chapter::default() };
        library.write_chapter(&broken).unwrap();
        library.update_work(&first.id, |work| work.chapter_order.push("c2".into())).unwrap();
        fs::write(library.chapter_file(&first.id, "c2"), "").unwrap();

        let second = library.create_work("둘째").unwrap();
        for (id, created_at) in [("b", "2026-02-02T00:00:00.000Z"), ("a", "2026-01-01T00:00:00.000Z")] {
            let chapter = Chapter {
                id: id.into(),
                work_id: second.id.clone(),
                created_at: created_at.into(),
                ..Chapter::default()
            };
            library.write_chapter_file(&chapter).unwrap();
        }
        library.update_work(&second.id, |work| work.translation_notes = "말투".into()).unwrap();
        let damaged = fs::read_to_string(library.work_file(&second.id)).unwrap().replace("[]", "[,]");
        fs::write(library.work_file(&second.id), &damaged).unwrap();

        let listing = library.list_all().unwrap();
        assert_eq!(listing.works.len(), 2);
        assert_eq!(listing.works[0].chapters.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["c1"]);
        let rebuilt = &listing.works[1];
        assert_eq!(rebuilt.work.title, "둘째", "title salvaged from the damaged file");
        assert_eq!(rebuilt.work.chapter_order, ["a", "b"], "chapters in creation order");
        assert_eq!(listing.unreadable.len(), 2, "{:?}", listing.unreadable);
        assert!(listing.unreadable.iter().any(|entry| entry.path == library.chapter_file(&first.id, "c2")));
        let kept = library.work_dir(&second.id).join("work.json.bad");
        assert!(listing.unreadable.iter().any(|entry| entry.path == kept));
        assert_eq!(fs::read_to_string(&kept).unwrap(), damaged);
        let stored = library.read_work(&second.id).unwrap().unwrap();
        assert_eq!((stored.chapter_order.len(), stored.translation_notes.as_str()), (2, "말투"));

        // Repaired once; the damaged chapter is still reported and still opens nothing else.
        let again = library.list_all().unwrap();
        assert_eq!(again.unreadable.len(), 1);
        assert!(library.open_chapter("a").is_ok());
        assert!(library.open_chapter("c2").is_err());
        broken.title = "고침".into();
        library.write_chapter(&broken).unwrap();
        assert!(library.list_all().unwrap().unreadable.is_empty());
    }

    #[test]
    fn damaged_index_and_work_files_are_rebuilt_by_saves_and_lookups() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let older = library.create_work("older").unwrap();
        let chapter = sample_chapter(&library, &older);
        let newer = library.create_work("newer").unwrap();
        fs::write(library.index_path(), "").unwrap();
        // Cut off mid-write.
        let text = fs::read_to_string(library.work_file(&older.id)).unwrap();
        fs::write(library.work_file(&older.id), &text[..text.find("chapterOrder").unwrap()]).unwrap();

        // Saving a chapter repairs its work instead of failing every autosave.
        library.write_chapter(&chapter).unwrap();
        let work = library.read_work(&older.id).unwrap().unwrap();
        assert_eq!((work.title.as_str(), work.chapter_order.as_slice()), ("older", &["c1".to_owned()][..]));
        fs::write(library.work_file(&older.id), "").unwrap();
        library.write_chapter(&chapter).unwrap();
        assert_eq!(library.read_work(&older.id).unwrap().unwrap().title, DEFAULT_WORK_TITLE, "nothing to salvage");
        assert_eq!(library.find_chapter_work_id("missing").unwrap(), None);
        assert!(library.index_path().with_file_name("index.json.bad").is_file());
        let order = library.read_index().unwrap().work_order;
        assert_eq!(order.len(), 2);
        assert!(order.contains(&older.id) && order.contains(&newer.id));
        assert_eq!(library.list().unwrap().len(), 2);
    }

    #[test]
    fn unique_chapter_titles() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        sample_chapter(&library, &work);
        assert_eq!(library.make_unique_chapter_title(&work.id, "1화", None).unwrap(), "1화 (1)");
        assert_eq!(library.make_unique_chapter_title(&work.id, "1화", Some("c1")).unwrap(), "1화");
    }

    #[test]
    fn page_layer_paths_fall_back_to_chapter_directory() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let chapter = Chapter { id: "c".into(), work_id: "w".into(), ..Chapter::default() };
        let pages_dir = library.chapter_dir("w", "c").join("pages");
        fs::create_dir_all(&pages_dir).unwrap();
        fs::write(pages_dir.join("001-p.jpg"), b"x").unwrap();
        let page =
            PageRecord { id: "p".into(), image_path: "/old/location/pages/001-p.jpg".into(), ..PageRecord::default() };
        assert_eq!(
            library.resolve_page_layer_path(&chapter, &page, PageLayer::Source),
            Some(pages_dir.join("001-p.jpg"))
        );

        let mask = library.inpaint_layer_path(&chapter, "p", PageLayer::InpaintMask);
        fs::create_dir_all(mask.parent().unwrap()).unwrap();
        fs::write(&mask, b"x").unwrap();
        assert_eq!(library.resolve_page_layer_path(&chapter, &page, PageLayer::InpaintMask), None, "unclaimed layer");
        let page = PageRecord { inpaint_mask_path: Some("/old/mask.png".into()), ..page };
        assert_eq!(library.resolve_page_layer_path(&chapter, &page, PageLayer::InpaintMask), Some(mask));

        // Imported on Windows, opened elsewhere (and the other way round).
        for stored in
            [r"C:\Users\a\Documents\MangaTranslationTools\library\pages\001-p.jpg", "/Users/a/pages/001-p.jpg"]
        {
            let page = PageRecord { id: "p".into(), image_path: stored.into(), ..PageRecord::default() };
            assert_eq!(
                library.resolve_page_layer_path(&chapter, &page, PageLayer::Source),
                Some(pages_dir.join("001-p.jpg")),
                "{stored}"
            );
        }
    }

    #[test]
    fn pages_with_the_same_name_export_to_different_files() {
        let library = Library::new("/library");
        let names = ["001.jpg", "001.png", "1권/001.jpg", "001 (2).png", "A.png", "a.jpg", "002.png"];
        let chapter = Chapter {
            id: "c".into(),
            work_id: "w".into(),
            pages: names
                .iter()
                .enumerate()
                .map(|(index, name)| PageRecord { id: format!("p{index}"), name: (*name).into(), ..Default::default() })
                .collect(),
            ..Chapter::default()
        };
        let files: Vec<String> = chapter
            .pages
            .iter()
            .map(|page| {
                let path = library.render_output_path(&chapter, page);
                assert_eq!(path.parent(), Some(library.chapter_dir("w", "c").join("renders").as_path()));
                path.file_name().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        assert_eq!(files, ["001.png", "001 (3).png", "001 (4).png", "001 (2).png", "A.png", "a (2).png", "002.png"]);
    }

    #[test]
    fn chapter_status_resolution() {
        let page = |status| PageRecord { analysis_status: status, ..PageRecord::default() };
        use PageAnalysisStatus::*;
        assert_eq!(resolve_chapter_status(&[]), ChapterStatus::Idle);
        assert_eq!(resolve_chapter_status(&[page(Completed), page(Completed)]), ChapterStatus::Completed);
        assert_eq!(resolve_chapter_status(&[page(Completed), page(Running)]), ChapterStatus::Running);
        assert_eq!(resolve_chapter_status(&[page(Failed), page(Failed)]), ChapterStatus::Failed);
        assert_eq!(resolve_chapter_status(&[page(Completed), page(Failed)]), ChapterStatus::Partial);
        assert_eq!(resolve_chapter_status(&[page(Idle), page(Failed)]), ChapterStatus::Idle);
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn page_ids(chapter: &Chapter) -> Vec<&str> {
        chapter.pages.iter().map(|page| page.id.as_str()).collect()
    }

    const OLD: &str = "2026-01-01T00:00:00.000Z";

    fn status_chapter() -> Chapter {
        use PageAnalysisStatus::*;
        let page = |id: &str, status| PageRecord { id: id.into(), analysis_status: status, ..PageRecord::default() };
        Chapter {
            id: "c".into(),
            work_id: "w".into(),
            status: ChapterStatus::Idle,
            last_opened_page_id: Some("p2".into()),
            page_order: ids(&["p1", "p2", "p3"]),
            pages: vec![page("p1", Completed), page("p2", Idle), page("p3", Completed)],
            updated_at: OLD.into(),
            ..Chapter::default()
        }
    }

    #[test]
    fn reorder_ids_matches_ts() {
        let current = ids(&["a", "b", "c"]);
        assert_eq!(reorder_ids(&current, &ids(&["c", "x", "a"])), ["c", "a", "b"]);
        assert_eq!(reorder_ids(&current, &[]), ["a", "b", "c"]);
        assert_eq!(reorder_ids(&current, &ids(&["b", "b"])), ["b", "b", "a", "c"], "TS keeps requested duplicates");
    }

    #[test]
    fn reorder_by_target_moves_before_or_after() {
        let order = ids(&["a", "b", "c", "d"]);
        assert_eq!(reorder_by_target(&order, "a", "c", DropPosition::Before), ["b", "a", "c", "d"]);
        assert_eq!(reorder_by_target(&order, "a", "c", DropPosition::After), ["b", "c", "a", "d"]);
        assert_eq!(reorder_by_target(&order, "d", "a", DropPosition::default()), ["d", "a", "b", "c"]);
        assert_eq!(reorder_by_target(&order, "d", "a", DropPosition::After), ["a", "d", "b", "c"]);
        assert_eq!(reorder_by_target(&order, "x", "a", DropPosition::Before), order);
        assert_eq!(reorder_by_target(&order, "a", "x", DropPosition::Before), order);
        assert_eq!(reorder_by_target(&order, "b", "b", DropPosition::After), order);
    }

    #[test]
    fn reorder_chapters_persists_validated_order() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let mut work = library.create_work("w").unwrap();
        work.chapter_order = ids(&["c1", "c2", "c3"]);
        work.updated_at = OLD.into();
        library.write_work(&work).unwrap();

        library.reorder_chapters(&work.id, &ids(&["c3", "unknown", "c1"])).unwrap();
        let stored = library.read_work(&work.id).unwrap().unwrap();
        assert_eq!(stored.chapter_order, ["c3", "c1", "c2"]);
        assert_ne!(stored.updated_at, OLD);
        assert!(matches!(library.reorder_chapters("missing", &[]), Err(CoreError::NotFound(_))));
    }

    #[test]
    fn reorder_chapter_pages_in_memory() {
        let mut chapter = status_chapter();
        reorder_chapter_pages(&mut chapter, &ids(&["p3", "p1"]));
        assert_eq!(chapter.page_order, ["p3", "p1", "p2"]);
        assert_eq!(page_ids(&chapter), ["p3", "p1", "p2"]);
        assert_eq!(chapter.status, ChapterStatus::Partial);
        assert_ne!(chapter.updated_at, OLD);

        // Like TS, pages missing from `page_order` are appended to `pages` but not added to the order.
        let mut chapter = status_chapter();
        chapter.page_order = ids(&["p1", "p2"]);
        reorder_chapter_pages(&mut chapter, &ids(&["p2"]));
        assert_eq!(chapter.page_order, ["p2", "p1"]);
        assert_eq!(page_ids(&chapter), ["p2", "p1", "p3"]);
    }

    #[test]
    fn reorder_pages_saves_chapter_and_touches_work() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        sample_chapter(&library, &work);

        let returned = library.reorder_pages("c1", &ids(&["p1"])).unwrap();
        let stored = library.read_chapter(&work.id, "c1").unwrap().unwrap();
        assert_eq!(stored, returned);
        assert_eq!(stored.page_order, ["p1", "p2"]);
        assert_eq!(page_ids(&stored), ["p1", "p2"]);
        assert_eq!(library.read_work(&work.id).unwrap().unwrap().updated_at, stored.updated_at);
        assert!(matches!(library.reorder_pages("missing", &[]), Err(CoreError::NotFound(_))));
    }

    #[test]
    fn remove_chapter_page_updates_order_last_opened_and_status() {
        let mut chapter = status_chapter();
        let removed = remove_chapter_page(&mut chapter, "p2").unwrap();
        assert_eq!(removed.id, "p2");
        assert_eq!(chapter.page_order, ["p1", "p3"]);
        assert_eq!(page_ids(&chapter), ["p1", "p3"]);
        assert_eq!(chapter.last_opened_page_id.as_deref(), Some("p1"), "falls back to the first remaining page");
        assert_eq!(chapter.status, ChapterStatus::Completed);
        assert_ne!(chapter.updated_at, OLD);

        remove_chapter_page(&mut chapter, "p3").unwrap();
        assert_eq!(chapter.last_opened_page_id.as_deref(), Some("p1"), "other pages keep the last opened page");
        remove_chapter_page(&mut chapter, "p1").unwrap();
        assert_eq!(chapter.last_opened_page_id, None);
        assert_eq!(chapter.status, ChapterStatus::Idle);

        let mut chapter = status_chapter();
        let before = chapter.clone();
        assert!(remove_chapter_page(&mut chapter, "missing").is_none());
        assert_eq!(chapter, before, "unknown page leaves the chapter untouched");
    }

    /// Writes `relative` under `root`, creating parent directories.
    fn touch(root: &Path, relative: &str) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"x").unwrap();
        path
    }

    #[test]
    fn delete_page_saves_chapter_then_removes_page_files() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let work = library.create_work("w").unwrap();
        let mut chapter = sample_chapter(&library, &work);
        let chapter_dir = library.chapter_dir(&work.id, "c1");
        let source = touch(&chapter_dir, "pages/001-p1.png");
        chapter.pages[0].name = "001".into();
        chapter.pages[0].image_path = source.to_string_lossy().into_owned();
        library.write_chapter(&chapter).unwrap();

        let removed = [
            touch(&chapter_dir, "inpaint/p1-mask.png"),
            touch(&chapter_dir, "inpaint/p1-last-import.psd"),
            touch(&chapter_dir, "inpaint/p10-mask.png"), // TS matches by `startsWith(pageId)`.
            touch(&chapter_dir, "runs/r1/pages/p1/blocks.json"),
            touch(&chapter_dir, "runs/r2/pages/p1/nested/crop.png"),
        ];
        let kept = [
            touch(&chapter_dir, "inpaint/p2-mask.png"),
            touch(&chapter_dir, "inpaint/p1-dir/file.png"),
            touch(&chapter_dir, "runs/r1/pages/p2/blocks.json"),
            touch(&chapter_dir, "runs/r1/pages/p1x/blocks.json"),
            touch(&chapter_dir, "renders/001.png"),
            touch(&chapter_dir, "pages/002-p2.png"),
        ];
        touch(&chapter_dir, "runs/stray-file");

        let returned = library.delete_page("c1", "p1").unwrap();
        let stored = library.read_chapter(&work.id, "c1").unwrap().unwrap();
        assert_eq!(stored, returned);
        assert_eq!(stored.page_order, ["p2"]);
        assert_eq!(page_ids(&stored), ["p2"]);
        assert_eq!(library.read_work(&work.id).unwrap().unwrap().updated_at, stored.updated_at);
        assert!(!source.exists());
        for path in &removed {
            assert!(!path.exists(), "{} should be deleted", path.display());
        }
        assert!(!chapter_dir.join("runs/r1/pages/p1").exists());
        for path in &kept {
            assert!(path.exists(), "{} should be kept", path.display());
        }

        let unchanged = library.delete_page("c1", "missing").unwrap();
        assert_eq!(unchanged, stored);
        assert!(matches!(library.delete_page("missing", "p2"), Err(CoreError::NotFound(_))));
    }

    #[test]
    fn deleting_a_page_of_a_copied_library_keeps_the_original_image() {
        let dir = tempfile::tempdir().unwrap();
        let original = touch(dir.path(), "original/library/works/w/chapters/c1/pages/001-p1.png");
        let library = Library::new(dir.path().join("copy/library"));
        let work = library.create_work("w").unwrap();
        let mut chapter = sample_chapter(&library, &work);
        let copied = touch(&library.chapter_dir(&work.id, "c1"), "pages/001-p1.png");
        // The copy still stores the original library's absolute path.
        chapter.pages[0].image_path = original.to_string_lossy().into_owned();
        library.write_chapter(&chapter).unwrap();

        library.delete_page("c1", "p1").unwrap();
        assert!(original.exists(), "the other library's image is untouched");
        assert!(!copied.exists(), "this library's copy is removed");

        // A path outside any `pages/` folder only ever matches by name inside this chapter.
        let outside = touch(dir.path(), "elsewhere/002-p2.png");
        let mut chapter = library.read_chapter(&work.id, "c1").unwrap().unwrap();
        chapter.pages[0].image_path = outside.to_string_lossy().into_owned();
        library.write_chapter(&chapter).unwrap();
        library.delete_page("c1", "p2").unwrap();
        assert!(outside.exists());
    }

    #[test]
    fn page_artifact_cleanup_skips_unsafe_ids() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        let chapter_dir = library.chapter_dir("w", "c");
        let files = [
            touch(&chapter_dir, "inpaint/p1-mask.png"),
            touch(&chapter_dir, "runs/r1/pages/p1/blocks.json"),
            #[cfg(not(windows))]
            touch(&chapter_dir, "runs/r1/pages/C:/blocks.json"),
        ];
        for page_id in ["", ".", "..", "../pages", "a\\b", "C:", "c:p1", "p1\0"] {
            library.remove_page_artifacts("w", "c", page_id).unwrap();
        }
        assert!(files.iter().all(|path| path.exists()));
        library.remove_page_artifacts("w", "missing-chapter", "p1").unwrap();
    }

    #[test]
    fn deletes_refuse_unsafe_or_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path());
        std::fs::create_dir_all(library.work_dir("keep")).unwrap();
        std::fs::write(library.work_file("keep"), r#"{"title":"no id"}"#).unwrap();
        assert!(library.delete_work("").is_err());
        assert!(library.delete_work("..").is_err());
        assert!(library.delete_work("missing").is_err());
        assert!(library.work_dir("keep").exists(), "nothing else was removed");
        assert_eq!(library.read_work("keep").unwrap().unwrap().id, "keep", "folder name wins");
        assert!(library.delete_chapter("../keep").is_err());
        // On Windows `join("C:")` would replace the whole base path.
        for id in ["C:", "C:keep", "a:b", "x\ty"] {
            assert!(!is_safe_id(id), "{id:?}");
            assert!(library.delete_work(id).is_err());
            assert!(library.delete_chapter(id).is_err());
        }
        assert!(is_safe_id("0f9c2a5e-1b7d-4c3e-9a8f-2d6e4b1c7a90"));
        assert!(is_safe_id("p1-block-3"));
        assert!(library.work_dir("keep").exists());
    }
}
