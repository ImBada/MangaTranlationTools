//! Importing images, folders, archives (ZIP/CBZ, RAR/CBR) and PDFs into the library
//! (`libraryImport.ts`).
//!
//! A preview lists chapter drafts; creating the import copies every page into
//! `works/<work>/chapters/<chapter>/pages/NNN-<pageId>.<ext>` and writes `chapter.json`.
//! A failed import removes everything it created.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, CoreResult};
use crate::library::{
    DEFAULT_CHAPTER_TITLE, DEFAULT_WORK_TITLE, Library, resolve_chapter_status, sanitize_title, unique_title,
};
use crate::model::{Chapter, ImportSourceKind, PageAnalysisStatus, PageRecord};
use crate::paths::is_supported_image_path;
use crate::time::now_iso;

#[derive(Debug, Clone, PartialEq)]
pub enum PageSource {
    File(PathBuf),
    /// Entry `index` of the archive at `archive`.
    ZipEntry {
        archive: PathBuf,
        index: usize,
        name: String,
    },
    /// Member `index` of the RAR archive at `archive`.
    RarEntry {
        archive: PathBuf,
        index: usize,
        name: String,
    },
    /// Page `index` of the PDF at `document`, rendered at [`PDF_DPI`].
    PdfPage {
        document: PathBuf,
        index: usize,
    },
}

/// PDF pages become PNGs at this resolution, enough for fine lettering and line art.
pub const PDF_DPI: f32 = 300.0;

/// What a file imports as when it isn't an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    Rar,
    Pdf,
}

impl ArchiveKind {
    /// By extension: `.zip`/`.cbz`, `.rar`/`.cbr`, `.pdf`.
    pub fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "zip" | "cbz" => Some(Self::Zip),
            "rar" | "cbr" => Some(Self::Rar),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// Extensions the archive pickers offer.
pub const ARCHIVE_EXTENSIONS: [&str; 5] = ["zip", "cbz", "rar", "cbr", "pdf"];

#[derive(Debug, Clone, PartialEq)]
pub struct PageDraft {
    pub name: String,
    pub source: PageSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterDraft {
    pub title: String,
    pub source_kind: ImportSourceKind,
    pub pages: Vec<PageDraft>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportPreview {
    pub batch: bool,
    pub source_kind: ImportSourceKind,
    pub suggested_work_title: String,
    pub chapters: Vec<ChapterDraft>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportTarget {
    NewWork(String),
    ExistingWork(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub work_id: String,
    pub chapter_ids: Vec<String>,
}

/// `localeCompare(..., { numeric: true, sensitivity: "base" })` approximation: digit runs
/// compare by value, everything else case-insensitively.
pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let (mut a, mut b) = (left.chars().peekable(), right.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take_number = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let (da, db) = (take_number(&mut a), take_number(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let ordering = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(x), Some(y)) => {
                let ordering = x.to_lowercase().cmp(y.to_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn sort_naturally(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

fn list_image_files(folder: &Path) -> CoreResult<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(|source| CoreError::io(folder, source))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| is_supported_image_path(path))
        .collect();
    sort_naturally(&mut files);
    Ok(files)
}

/// Decodes a ZIP entry name: UTF-8 when valid, otherwise Shift_JIS (common for Japanese
/// archives created on Windows).
pub fn decode_zip_name(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(text) => text.replace('\\', "/"),
        Err(_) => encoding_rs::SHIFT_JIS.decode(raw).0.replace('\\', "/"),
    }
}

fn is_ignored_zip_entry(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name.split('/').any(|part| part.starts_with("._") || part.eq_ignore_ascii_case("__MACOSX"))
}

/// Image members of a RAR archive in natural order.
pub fn list_rar_images(archive: &Path) -> CoreResult<Vec<PageDraft>> {
    let rar = rars::ArchiveReader::read_path(archive)
        .map_err(|error| CoreError::Invalid(format!("{}: RAR을 열지 못했습니다: {error}", archive.display())))?;
    let mut entries: Vec<PageDraft> = rar
        .member_refs()
        .enumerate()
        .filter(|(_, member)| !member.is_directory() && !member.is_redirection())
        .map(|(index, member)| {
            // Old archives store names in the creator's code page (Shift_JIS for Japanese ones).
            let name = if member.name_is_unicode() {
                member.to_owned().meta.name_lossy().replace('\\', "/")
            } else {
                decode_zip_name(member.name_bytes())
            };
            (index, name)
        })
        .filter(|(_, name)| !is_ignored_zip_entry(name) && is_supported_image_path(Path::new(name)))
        .map(|(index, name)| PageDraft {
            name: name.clone(),
            source: PageSource::RarEntry { archive: archive.to_path_buf(), index, name },
        })
        .collect();
    entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(entries)
}

fn open_pdf(document: &Path) -> CoreResult<hayro::hayro_syntax::Pdf> {
    let data = fs::read(document).map_err(|source| CoreError::io(document, source))?;
    hayro::hayro_syntax::Pdf::new(data)
        .map_err(|error| CoreError::Invalid(format!("{}: PDF를 열지 못했습니다: {error:?}", document.display())))
}

/// One page per PDF page, named `{stem}-001.png` and so on.
pub fn list_pdf_pages(document: &Path) -> CoreResult<Vec<PageDraft>> {
    let count = open_pdf(document)?.pages().len();
    let stem = file_stem(document);
    let width = count.to_string().len().max(3);
    Ok((0..count)
        .map(|index| PageDraft {
            name: format!("{stem}-{:0width$}.png", index + 1),
            source: PageSource::PdfPage { document: document.to_path_buf(), index },
        })
        .collect())
}

/// Pages of an archive or PDF by [`ArchiveKind::of`].
pub fn list_archive_pages(path: &Path) -> CoreResult<Vec<PageDraft>> {
    match ArchiveKind::of(path) {
        Some(ArchiveKind::Rar) => list_rar_images(path),
        Some(ArchiveKind::Pdf) => list_pdf_pages(path),
        _ => list_zip_images(path),
    }
}

/// Image entries of an archive in natural order.
pub fn list_zip_images(archive: &Path) -> CoreResult<Vec<PageDraft>> {
    let file = fs::File::open(archive).map_err(|source| CoreError::io(archive, source))?;
    let zip = zip::ZipArchive::new(file)
        .map_err(|error| CoreError::Invalid(format!("{}: ZIP을 열지 못했습니다: {error}", archive.display())))?;
    let mut entries: Vec<PageDraft> = zip
        .file_names_raw()
        .enumerate()
        .map(|(index, raw)| (index, decode_zip_name(raw)))
        .filter(|(_, name)| {
            !name.ends_with('/') && !is_ignored_zip_entry(name) && is_supported_image_path(Path::new(name))
        })
        .map(|(index, name)| PageDraft {
            name: name.clone(),
            source: PageSource::ZipEntry { archive: archive.to_path_buf(), index, name },
        })
        .collect();
    entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(entries)
}

fn single(source_kind: ImportSourceKind, title: String, pages: Vec<PageDraft>) -> ImportPreview {
    ImportPreview {
        batch: false,
        source_kind,
        suggested_work_title: DEFAULT_WORK_TITLE.into(),
        chapters: vec![ChapterDraft { title, source_kind, pages, enabled: true }],
    }
}

pub fn preview_images(paths: &[PathBuf]) -> ImportPreview {
    let mut files: Vec<PathBuf> = paths.iter().filter(|path| is_supported_image_path(path)).cloned().collect();
    sort_naturally(&mut files);
    let pages =
        files.into_iter().map(|path| PageDraft { name: file_name(&path), source: PageSource::File(path) }).collect();
    single(ImportSourceKind::Images, DEFAULT_CHAPTER_TITLE.into(), pages)
}

pub fn preview_folder(folder: &Path) -> CoreResult<ImportPreview> {
    let pages = list_image_files(folder)?
        .into_iter()
        .map(|path| PageDraft { name: file_name(&path), source: PageSource::File(path) })
        .collect();
    Ok(single(ImportSourceKind::Folder, file_name(folder), pages))
}

/// One chapter from an archive or PDF.
pub fn preview_archive(archive: &Path) -> CoreResult<ImportPreview> {
    Ok(single(archive_source_kind(archive), file_stem(archive), list_archive_pages(archive)?))
}

fn archive_source_kind(path: &Path) -> ImportSourceKind {
    if ArchiveKind::of(path) == Some(ArchiveKind::Pdf) { ImportSourceKind::Pdf } else { ImportSourceKind::Zip }
}

/// Every archive or PDF directly inside `folder` plus every nested folder containing
/// images becomes one chapter (`previewZipFolder`).
pub fn preview_zip_folder(folder: &Path) -> CoreResult<ImportPreview> {
    let mut sources: Vec<(String, ChapterDraft)> = Vec::new();
    for entry in fs::read_dir(folder).map_err(|source| CoreError::io(folder, source))?.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_file()) && ArchiveKind::of(&path).is_some() {
            let pages = list_archive_pages(&path)?;
            sources.push((
                file_name(&path),
                ChapterDraft {
                    title: file_stem(&path),
                    source_kind: ImportSourceKind::ZipFolder,
                    pages,
                    enabled: true,
                },
            ));
        }
    }
    let mut stack = vec![folder.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut children: Vec<PathBuf> = fs::read_dir(&current)
            .map_err(|source| CoreError::io(&current, source))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect();
        sort_naturally(&mut children);
        stack.extend(children.into_iter().rev());
        if current == folder {
            continue;
        }
        let images = list_image_files(&current)?;
        if images.is_empty() {
            continue;
        }
        let relative = current.strip_prefix(folder).unwrap_or(&current).to_string_lossy().replace('\\', "/");
        let pages = images
            .into_iter()
            .map(|path| PageDraft { name: file_name(&path), source: PageSource::File(path) })
            .collect();
        sources.push((
            relative.clone(),
            ChapterDraft { title: relative, source_kind: ImportSourceKind::Folder, pages, enabled: true },
        ));
    }
    sources.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    Ok(ImportPreview {
        batch: true,
        source_kind: ImportSourceKind::ZipFolder,
        suggested_work_title: file_name(folder),
        chapters: sources.into_iter().map(|(_, chapter)| chapter).collect(),
    })
}

/// Creates chapters for the enabled drafts. `progress(done, total)` is called per page and
/// `cancelled()` aborts (rolling back everything created).
pub fn create_import(
    library: &Library,
    preview: &ImportPreview,
    target: &ImportTarget,
    mut progress: impl FnMut(usize, usize),
    cancelled: impl Fn() -> bool,
) -> CoreResult<ImportResult> {
    let (work, created_work) = match target {
        ImportTarget::NewWork(title) => {
            let title = if title.trim().is_empty() { preview.suggested_work_title.as_str() } else { title.as_str() };
            (library.create_work(title)?, true)
        }
        ImportTarget::ExistingWork(id) => {
            (library.read_work(id)?.ok_or_else(|| CoreError::NotFound("선택한 작품을 찾지 못했습니다.".into()))?, false)
        }
    };
    let total: usize = preview.chapters.iter().filter(|c| c.enabled).map(|c| c.pages.len()).sum();
    let mut done = 0usize;
    let mut created: Vec<(String, PathBuf)> = Vec::new();
    let result = (|| -> CoreResult<Vec<String>> {
        // Read once; each new chapter's title is added as it is taken.
        let mut used_titles = library.chapter_titles(&work.id)?;
        for draft in preview.chapters.iter().filter(|chapter| chapter.enabled) {
            if draft.pages.is_empty() {
                continue;
            }
            let chapter_id = uuid::Uuid::new_v4().to_string();
            let chapter_dir = library.chapter_dir(&work.id, &chapter_id);
            created.push((chapter_id.clone(), chapter_dir.clone()));
            let pages_dir = chapter_dir.join("pages");
            fs::create_dir_all(&pages_dir).map_err(|source| CoreError::io(&pages_dir, source))?;
            let mut archive: Option<OpenSource> = None;
            let mut pages = Vec::with_capacity(draft.pages.len());
            for (index, page) in draft.pages.iter().enumerate() {
                if cancelled() {
                    return Err(CoreError::Invalid("가져오기를 취소했습니다.".into()));
                }
                pages.push(materialize_page(page, &pages_dir, index, &mut archive)?);
                done += 1;
                progress(done, total);
            }
            let now = now_iso();
            let title = unique_title(&sanitize_title(&draft.title, DEFAULT_CHAPTER_TITLE), &used_titles);
            used_titles.insert(title.clone());
            let chapter = Chapter {
                id: chapter_id.clone(),
                work_id: work.id.clone(),
                title,
                source_kind: draft.source_kind,
                status: resolve_chapter_status(&pages),
                page_order: pages.iter().map(|page: &PageRecord| page.id.clone()).collect(),
                pages,
                created_at: now.clone(),
                updated_at: now.clone(),
                ..Chapter::default()
            };
            let mut current_work = library.read_work(&work.id)?.unwrap_or_else(|| work.clone());
            current_work.chapter_order.push(chapter_id.clone());
            current_work.updated_at = now;
            library.write_work(&current_work)?;
            library.write_chapter_file(&chapter)?;
        }
        Ok(created.iter().map(|(id, _)| id.clone()).collect())
    })();
    match result {
        Ok(chapter_ids) if !chapter_ids.is_empty() => Ok(ImportResult { work_id: work.id, chapter_ids }),
        other => {
            rollback(library, &work.id, created_work, &created);
            match other {
                Err(error) => Err(error),
                Ok(_) => Err(CoreError::Invalid("생성할 화가 없습니다.".into())),
            }
        }
    }
}

/// The archive or PDF the last page came from, kept open for the next.
enum OpenSource {
    Zip(PathBuf, zip::ZipArchive<fs::File>),
    Rar(PathBuf, rars::Archive),
    Pdf(PathBuf, hayro::hayro_syntax::Pdf),
}

impl OpenSource {
    fn path(&self) -> &Path {
        match self {
            Self::Zip(path, _) | Self::Rar(path, _) | Self::Pdf(path, _) => path,
        }
    }
}

/// `source` opened (or the one already open when it is the same file).
fn open_source<'a>(open: &'a mut Option<OpenSource>, path: &Path, kind: ArchiveKind) -> CoreResult<&'a mut OpenSource> {
    if open.as_ref().is_none_or(|current| current.path() != path) {
        *open = Some(match kind {
            ArchiveKind::Zip => {
                let file = fs::File::open(path).map_err(|source| CoreError::io(path, source))?;
                let zip = zip::ZipArchive::new(file)
                    .map_err(|error| CoreError::Invalid(format!("ZIP을 열지 못했습니다: {error}")))?;
                OpenSource::Zip(path.to_path_buf(), zip)
            }
            ArchiveKind::Rar => {
                let rar = rars::ArchiveReader::read_path(path)
                    .map_err(|error| CoreError::Invalid(format!("RAR을 열지 못했습니다: {error}")))?;
                OpenSource::Rar(path.to_path_buf(), rar)
            }
            ArchiveKind::Pdf => OpenSource::Pdf(path.to_path_buf(), open_pdf(path)?),
        });
    }
    Ok(open.as_mut().expect("source opened"))
}

fn render_pdf_page(pdf: &hayro::hayro_syntax::Pdf, index: usize) -> CoreResult<Vec<u8>> {
    let cache = hayro::RenderCache::new();
    let pages = pdf.pages();
    let page = pages.get(index).ok_or_else(|| CoreError::Invalid(format!("PDF {}쪽을 찾지 못했습니다.", index + 1)))?;
    let scale = PDF_DPI / 72.0;
    let settings = hayro::PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: hayro::vello_cpu::color::palette::css::WHITE,
    };
    hayro::render(
        page,
        &cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &hayro::RenderSettings::default(),
        &settings,
    )
    .into_png()
    .map_err(|error| CoreError::Invalid(format!("PDF {}쪽을 PNG로 만들지 못했습니다: {error:?}", index + 1)))
}

fn materialize_page(
    draft: &PageDraft,
    pages_dir: &Path,
    index: usize,
    archive: &mut Option<OpenSource>,
) -> CoreResult<PageRecord> {
    let page_id = uuid::Uuid::new_v4().to_string();
    let source_name = match &draft.source {
        PageSource::File(path) => path.to_string_lossy().into_owned(),
        PageSource::ZipEntry { name, .. } | PageSource::RarEntry { name, .. } => name.clone(),
        PageSource::PdfPage { .. } => draft.name.clone(),
    };
    let extension = Path::new(&source_name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "png".into());
    let output = pages_dir.join(format!("{:03}-{page_id}.{extension}", index + 1));
    match &draft.source {
        PageSource::File(path) => {
            fs::copy(path, &output).map_err(|source| CoreError::io(path, source))?;
        }
        PageSource::ZipEntry { archive: archive_path, index: entry_index, name } => {
            let OpenSource::Zip(_, zip) = open_source(archive, archive_path, ArchiveKind::Zip)? else {
                unreachable!("opened as ZIP")
            };
            let mut entry = zip
                .by_index(*entry_index)
                .map_err(|error| CoreError::Invalid(format!("ZIP 항목을 찾지 못했습니다: {name}: {error}")))?;
            let mut target = fs::File::create(&output).map_err(|source| CoreError::io(&output, source))?;
            std::io::copy(&mut entry, &mut target).map_err(|source| CoreError::io(&output, source))?;
        }
        PageSource::RarEntry { archive: archive_path, index: member, name } => {
            let OpenSource::Rar(_, rar) = open_source(archive, archive_path, ArchiveKind::Rar)? else {
                unreachable!("opened as RAR")
            };
            let bytes = rar
                .read_member_at(*member, None)
                .map_err(|error| CoreError::Invalid(format!("RAR 항목을 풀지 못했습니다: {name}: {error}")))?
                .ok_or_else(|| CoreError::Invalid(format!("RAR 항목을 찾지 못했습니다: {name}")))?;
            fs::write(&output, bytes).map_err(|source| CoreError::io(&output, source))?;
        }
        PageSource::PdfPage { document, index: page } => {
            let OpenSource::Pdf(_, pdf) = open_source(archive, document, ArchiveKind::Pdf)? else {
                unreachable!("opened as PDF")
            };
            fs::write(&output, render_pdf_page(pdf, *page)?).map_err(|source| CoreError::io(&output, source))?;
        }
    }
    let (width, height) = image::image_dimensions(&output).unwrap_or((1000, 1400));
    let now = now_iso();
    Ok(PageRecord {
        id: page_id,
        name: draft.name.clone(),
        image_path: output.to_string_lossy().into_owned(),
        width,
        height,
        analysis_status: PageAnalysisStatus::Idle,
        created_at: now.clone(),
        updated_at: now,
        ..PageRecord::default()
    })
}

/// `cleanupFailedImport`.
fn rollback(library: &Library, work_id: &str, created_work: bool, created: &[(String, PathBuf)]) {
    if created_work {
        if let Err(error) = library.delete_work(work_id) {
            log::warn!("import rollback failed: {error}");
        }
        return;
    }
    if let Ok(Some(mut work)) = library.read_work(work_id) {
        let before = work.chapter_order.len();
        work.chapter_order.retain(|id| !created.iter().any(|(created_id, _)| created_id == id));
        if work.chapter_order.len() != before {
            work.updated_at = now_iso();
            let _ = library.write_work(&work);
        }
    }
    for (_, dir) in created {
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::new(width, height).save(path).unwrap();
    }

    #[test]
    fn natural_sorting() {
        let mut names = vec!["p10.png", "P2.png", "p1.png", "p02b.png"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["p1.png", "P2.png", "p02b.png", "p10.png"]);
    }

    #[test]
    fn shift_jis_zip_names_are_decoded() {
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode("第1話/001.jpg");
        assert_eq!(decode_zip_name(&encoded), "第1話/001.jpg");
        assert_eq!(decode_zip_name("a\\b.png".as_bytes()), "a/b.png");
    }

    #[test]
    fn folder_and_zip_import_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        fs::create_dir_all(source.join("2화")).unwrap();
        fs::create_dir_all(source.join("1화")).unwrap();
        write_png(&source.join("1화/10.png"), 3, 4);
        write_png(&source.join("1화/2.png"), 3, 4);
        write_png(&source.join("2화/1.png"), 5, 6);
        // Archive with an image, a directory entry and macOS junk.
        let archive_path = source.join("3화.zip");
        {
            let mut writer = zip::ZipWriter::new(fs::File::create(&archive_path).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.add_directory("pages/", options).unwrap();
            let mut png = Vec::new();
            image::RgbaImage::new(7, 8).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
            writer.start_file("pages/01.png", options).unwrap();
            writer.write_all(&png).unwrap();
            writer.start_file("__MACOSX/pages/._01.png", options).unwrap();
            writer.write_all(b"junk").unwrap();
            writer.finish().unwrap();
        }

        let preview = preview_zip_folder(&source).unwrap();
        let titles: Vec<&str> = preview.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["1화", "2화", "3화"]);
        assert_eq!(preview.chapters[0].pages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["2.png", "10.png"]);
        assert_eq!(preview.chapters[2].pages.len(), 1, "junk and directories skipped");

        let library = Library::new(dir.path().join("library"));
        let mut done = Vec::new();
        let result = create_import(
            &library,
            &preview,
            &ImportTarget::NewWork(String::new()),
            |d, t| done.push((d, t)),
            || false,
        )
        .unwrap();
        assert_eq!(result.chapter_ids.len(), 3);
        assert_eq!(done.last(), Some(&(4, 4)));
        let listed = library.list().unwrap();
        assert_eq!(listed[0].work.title, "src", "suggested title used");
        let chapter = library.open_chapter(&result.chapter_ids[2]).unwrap();
        assert_eq!((chapter.pages[0].width, chapter.pages[0].height), (7, 8));
        let image_path = Path::new(&chapter.pages[0].image_path);
        assert!(image_path.is_file());
        // Separator-agnostic: Windows stores `\pages\001-…`.
        assert_eq!(image_path.parent().and_then(Path::file_name), Some(std::ffi::OsStr::new("pages")));
        assert!(image_path.file_name().unwrap().to_string_lossy().starts_with("001-"));
    }

    #[test]
    fn failed_import_rolls_back_new_work() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path().join("library"));
        let preview = preview_images(&[dir.path().join("missing.png")]);
        let result = create_import(&library, &preview, &ImportTarget::NewWork("w".into()), |_, _| {}, || false);
        assert!(result.is_err());
        assert!(library.list().unwrap().is_empty(), "work removed again");
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    /// A one-page PDF, 72×72pt, with a black bar on its left half.
    fn single_page_pdf() -> Vec<u8> {
        use std::fmt::Write as _;
        let content = "0 0 0 rg\n0 0 36 72 re f\n";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Resources << >> /Contents 4 0 R >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            write!(pdf, "{} 0 obj\n{object}\nendobj\n", index + 1).unwrap();
        }
        let xref = pdf.len();
        write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).unwrap();
        for offset in offsets {
            writeln!(pdf, "{offset:010} 00000 n ").unwrap();
        }
        write!(pdf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).unwrap();
        pdf.into_bytes()
    }

    #[test]
    fn pdf_pages_import_at_300_dpi() {
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("1화.pdf");
        fs::write(&document, single_page_pdf()).unwrap();
        let preview = preview_archive(&document).unwrap();
        assert_eq!(preview.chapters[0].title, "1화");
        assert_eq!(preview.chapters[0].source_kind, ImportSourceKind::Pdf);
        assert_eq!(preview.chapters[0].pages[0].name, "1화-001.png");
        let library = Library::new(dir.path().join("library"));
        let result =
            create_import(&library, &preview, &ImportTarget::NewWork("w".into()), |_, _| {}, || false).unwrap();
        let chapter = library.open_chapter(&result.chapter_ids[0]).unwrap();
        let page = &chapter.pages[0];
        assert_eq!((page.width, page.height), (300, 300));
        let image = image::open(&page.image_path).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(10, 10).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(290, 10).0, [255, 255, 255, 255]);
    }

    #[test]
    fn rar_images_import_in_natural_order() {
        use rars::rar50::{ArchiveEntry, ArchiveExtras, WriterOptions, write_streaming_archive_to};
        use rars::{EntrySource, WriterResources};
        let dir = tempfile::tempdir().unwrap();
        let entries = [
            ArchiveEntry::new("ComicInfo.xml", EntrySource::from_bytes(&b"<ComicInfo />"[..])),
            ArchiveEntry::new("pages/page10.png", EntrySource::from_bytes(png(3, 4))),
            ArchiveEntry::new("pages/page2.png", EntrySource::from_bytes(png(5, 6))),
            ArchiveEntry::new("__MACOSX/pages/._page3.png", EntrySource::from_bytes(&b"junk"[..])),
        ];
        let mut bytes = Vec::new();
        write_streaming_archive_to(
            &entries,
            WriterOptions::default(),
            ArchiveExtras::default(),
            &WriterResources::default(),
            &mut bytes,
        )
        .unwrap();
        let archive = dir.path().join("2화.cbr");
        fs::write(&archive, bytes).unwrap();
        let preview = preview_archive(&archive).unwrap();
        let names: Vec<&str> = preview.chapters[0].pages.iter().map(|page| page.name.as_str()).collect();
        assert_eq!(names, ["pages/page2.png", "pages/page10.png"]);
        let library = Library::new(dir.path().join("library"));
        let result =
            create_import(&library, &preview, &ImportTarget::NewWork("w".into()), |_, _| {}, || false).unwrap();
        let chapter = library.open_chapter(&result.chapter_ids[0]).unwrap();
        let sizes: Vec<(u32, u32)> = chapter.pages.iter().map(|page| (page.width, page.height)).collect();
        assert_eq!(sizes, [(5, 6), (3, 4)]);
    }

    #[test]
    fn archive_kinds_by_extension() {
        assert_eq!(ArchiveKind::of(Path::new("a.CBZ")), Some(ArchiveKind::Zip));
        assert_eq!(ArchiveKind::of(Path::new("a.cbr")), Some(ArchiveKind::Rar));
        assert_eq!(ArchiveKind::of(Path::new("a.pdf")), Some(ArchiveKind::Pdf));
        assert_eq!(ArchiveKind::of(Path::new("a.png")), None);
    }
}
