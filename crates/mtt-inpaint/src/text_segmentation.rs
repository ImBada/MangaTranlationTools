//! Glyph masks from the manga text segmentation model
//! (`<data>/models/manga-text-segmentation/model.safetensors`, run in-process on LibTorch
//! by `text_segmentation_torch.rs`): block text masks take the model's glyph pixels in
//! place of the luma heuristic's strokes when the model and LibTorch are there, and fall
//! back to the heuristic when they aren't or the model fails.
//!
//! The model sees a crop around the blocks; pre- and post-processing follow koharu's
//! `koharu-ml/src/manga_text_mask/processor.rs` (https://github.com/koharu-rs/koharu,
//! Copyright 2025-2026 Mayo Takanashi and Koharu contributors, MIT OR Apache-2.0), itself
//! the original inference script's: ImageNet normalization, zero padding to a multiple of
//! 32, probabilities above 0.5, a 3×3 elliptic closing, holes filled and a 2px dilation.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::{GrayImage, RgbImage, RgbaImage};
use mtt_core::model::TranslationBlock;
use mtt_core::paths::AppPaths;

use crate::lama::Download;
use crate::model_cache::{self, ModelCache};
use crate::text_mask::{self, Glyphs};
use crate::text_segmentation_torch::{PAD_MULTIPLE, TorchSegmenter};
use crate::{Rect, dilate, torch_runtime};

/// `mayocream/manga-text-segmentation-2025` (a safetensors port of
/// `a-b-c-x-y-z/Manga-Text-Segmentation-2025`, trained on Manga109; no license is stated),
/// pinned to a revision.
pub const MODEL_URL: &str = "https://huggingface.co/mayocream/manga-text-segmentation-2025/resolve/efd866e3ac6595ea20722f35ae343c403056ba76/model.safetensors";
const MODEL_SIZE: u64 = 216_516_884;
const MODEL_SHA256: &str = "f7e68a0c3e53dda4aba056a2370bcfb99f4dd189e9867d4570bc12578f9c7c4a";

/// ImageNet's mean and standard deviation (the script's `A.Normalize`).
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// koharu's cleaning defaults: probabilities above this are glyph pixels (strictly, as
/// upstream), gaps closed with a 3×3 ellipse, holes filled, then grown this far.
const THRESHOLD: f32 = 0.5;
pub(crate) const GLYPH_DILATION: usize = 2;
/// The model runs on pages up to this long side as they are (the original script never
/// resizes; Manga109's scans are 1654×1170 spreads); bigger pages are scaled down to it
/// and the probabilities scaled back.
const MAX_PAGE_SIDE: u32 = 2048;
/// Context around the blocks' area in the crop the model sees.
const CONTEXT_MARGIN: u32 = 64;
/// Longest tile side handed to the model (memory grows with the tile); bigger crops go in
/// tiles whose cores split the crop evenly, each with `TILE_MARGIN` of context.
const TILE_LIMIT: usize = 1024;
const TILE_MARGIN: usize = 64;

pub fn model_path(paths: &AppPaths) -> PathBuf {
    paths.models_dir.join("manga-text-segmentation").join("model.safetensors")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentationStatus {
    pub model_exists: bool,
    pub model_path: PathBuf,
    /// The LibTorch runtime's folder; `None` where none is published for this platform.
    pub runtime_path: Option<PathBuf>,
    pub runtime_exists: bool,
}

impl SegmentationStatus {
    pub fn ready(&self) -> bool {
        self.model_exists && self.runtime_exists
    }
}

pub fn status(paths: &AppPaths) -> SegmentationStatus {
    let model_path = model_path(paths);
    let runtime_path = torch_runtime::runtime_dir(paths);
    SegmentationStatus {
        model_exists: model_path.is_file(),
        model_path,
        runtime_exists: runtime_path.as_deref().is_some_and(torch_runtime::installed),
        runtime_path,
    }
}

/// Where the model runs (or will, before it has loaded).
pub fn device_label() -> &'static str {
    MODEL.device_label()
}

/// The downloaded model on the LibTorch runtime in `runtime`.
#[derive(Debug, Clone)]
pub struct TextSegmentation {
    model: PathBuf,
    runtime: PathBuf,
}

/// The model, loaded on first use and kept for later masks.
static MODEL: ModelCache<TorchSegmenter> = ModelCache::new("Text segmentation", TorchSegmenter::device);

impl TextSegmentation {
    /// The model when it and the runtime have been downloaded.
    pub fn detect(paths: &AppPaths) -> Option<Self> {
        let model = model_path(paths);
        let runtime = torch_runtime::runtime_dir(paths).filter(|dir| torch_runtime::installed(dir))?;
        model.is_file().then_some(Self { model, runtime })
    }

    /// The glyph mask (white = glyph) of `crop`, part of a page whose long side is
    /// `page_side`.
    pub fn glyph_mask(&self, crop: &RgbaImage, page_side: u32) -> Result<GrayImage, String> {
        let model = MODEL
            .get(&self.model, &self.runtime, || TorchSegmenter::load(&self.model, torch_runtime::default_device()))?;
        let _running = model_cache::exclusive();
        segment(crop, page_side, |input, height, width| model.probabilities(input, height, width))
            .map_err(|e| format!("글자 분할 실패: {e}"))
    }
}

/// Text mask for `blocks` like [`text_mask::mask_for_blocks`], with the glyph pixels from
/// `segmentation` in place of the luma heuristic's strokes when it's given and runs.
pub fn mask_for_blocks(
    source: &RgbaImage,
    blocks: &[&TranslationBlock],
    base: Option<&GrayImage>,
    segmentation: Option<&TextSegmentation>,
) -> GrayImage {
    match segmentation {
        Some(segmentation) => {
            mask_for_blocks_with(source, blocks, base, |crop, side| segmentation.glyph_mask(crop, side))
        }
        None => text_mask::mask_for_blocks(source, blocks, base),
    }
}

/// `mask_for_blocks` with `segment` giving the glyph mask of a crop (given the page's long
/// side); the heuristic alone when it fails.
fn mask_for_blocks_with(
    source: &RgbaImage,
    blocks: &[&TranslationBlock],
    base: Option<&GrayImage>,
    segment: impl FnOnce(&RgbaImage, u32) -> Result<GrayImage, String>,
) -> GrayImage {
    let (width, height) = source.dimensions();
    let Some(area) = text_mask::block_area(blocks, width, height) else {
        return text_mask::mask_for_blocks(source, blocks, base);
    };
    let started = Instant::now();
    let crop = expand(area.area, CONTEXT_MARGIN, width, height);
    let image = image::imageops::crop_imm(source, crop.x, crop.y, crop.width, crop.height).to_image();
    let glyphs = match segment(&image, width.max(height)) {
        Ok(glyphs) if glyphs.dimensions() == image.dimensions() => glyphs,
        Ok(_) => {
            log::warn!("text segmentation returned the wrong size; using the luma mask");
            return text_mask::mask_for_blocks(source, blocks, base);
        }
        Err(error) => {
            log::warn!("{error}; using the luma mask");
            return text_mask::mask_for_blocks(source, blocks, base);
        }
    };
    let inner = area.area;
    let glyph_area: Vec<u8> = (0..inner.height)
        .flat_map(|y| {
            let row = (inner.y - crop.y + y) as usize * crop.width as usize + (inner.x - crop.x) as usize;
            glyphs.as_raw()[row..row + inner.width as usize].iter().copied()
        })
        .collect();
    let mut mask = base.cloned().unwrap_or_else(|| GrayImage::new(width, height));
    text_mask::paint_block_mask(&mut mask, source, &area, Some(Glyphs { mask: &glyph_area, grown_by: GLYPH_DILATION }));
    log::info!(
        "text mask from the segmentation model ({}×{} crop) in {:.2}s",
        crop.width,
        crop.height,
        started.elapsed().as_secs_f32()
    );
    mask
}

/// The glyph mask of `crop` with `forward` giving the probabilities of a normalized,
/// padded input (`(3, H, W)` planes; returns `H * W`): scaled for the page, in tiles, then
/// cleaned.
fn segment(
    crop: &RgbaImage,
    page_side: u32,
    mut forward: impl FnMut(&[f32], usize, usize) -> Result<Vec<f32>, String>,
) -> Result<GrayImage, String> {
    let (width, height) = crop.dimensions();
    let rgb = over_white(crop);
    let probabilities = if page_side > MAX_PAGE_SIDE {
        let scale = MAX_PAGE_SIDE as f32 / page_side as f32;
        let scaled = |side: u32| ((side as f32 * scale).round() as u32).max(1);
        let (scaled_width, scaled_height) = (scaled(width), scaled(height));
        let small = image::imageops::resize(&rgb, scaled_width, scaled_height, image::imageops::FilterType::Triangle);
        let probabilities = probabilities_by_tiles(&small, &mut forward)?;
        resize_bilinear(&probabilities, scaled_width as usize, scaled_height as usize, width as usize, height as usize)
    } else {
        probabilities_by_tiles(&rgb, &mut forward)?
    };
    let mask = clean(&probabilities, width as usize, height as usize);
    GrayImage::from_raw(width, height, mask).ok_or_else(|| "마스크 크기가 맞지 않습니다".to_owned())
}

/// RGB with any transparency over white.
fn over_white(image: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let [r, g, b, a] = image.get_pixel(x, y).0;
        let over = |value: u8| ((u32::from(value) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

/// `image` normalized as `(3, H, W)` planes, zero-padded on the right and bottom to
/// multiples of `PAD_MULTIPLE` (the script's `F.pad` after `A.Normalize`: the padding is
/// the mean color). Returns the planes and the padded width and height.
pub(crate) fn model_input(image: &RgbImage) -> (Vec<f32>, usize, usize) {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let padded_width = width.div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
    let padded_height = height.div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
    let plane = padded_width * padded_height;
    let mut input = vec![0f32; 3 * plane];
    for (x, y, pixel) in image.enumerate_pixels() {
        let index = y as usize * padded_width + x as usize;
        for channel in 0..3 {
            input[channel * plane + index] = (f32::from(pixel[channel]) / 255.0 - MEAN[channel]) / STD[channel];
        }
    }
    (input, padded_width, padded_height)
}

/// The probabilities of `image` (`width * height`), in tiles of at most `TILE_LIMIT` a
/// side, each tile's core taken from a run with `TILE_MARGIN` of context around it.
fn probabilities_by_tiles(
    image: &RgbImage,
    forward: &mut impl FnMut(&[f32], usize, usize) -> Result<Vec<f32>, String>,
) -> Result<Vec<f32>, String> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut probabilities = vec![0f32; width * height];
    let core_limit = TILE_LIMIT - TILE_MARGIN * 2;
    let spans = |length: usize| {
        if length <= TILE_LIMIT {
            return vec![(0, length)];
        }
        let count = length.div_ceil(core_limit);
        let size = length.div_ceil(count);
        (0..count).map(|index| (index * size, ((index + 1) * size).min(length))).collect()
    };
    for (top, bottom) in spans(height) {
        for &(left, right) in &spans(width) {
            let (tile_left, tile_top) = (left.saturating_sub(TILE_MARGIN), top.saturating_sub(TILE_MARGIN));
            let (tile_right, tile_bottom) = ((right + TILE_MARGIN).min(width), (bottom + TILE_MARGIN).min(height));
            let (tile_width, tile_height) = (tile_right - tile_left, tile_bottom - tile_top);
            let tile = image::imageops::crop_imm(
                image,
                tile_left as u32,
                tile_top as u32,
                tile_width as u32,
                tile_height as u32,
            )
            .to_image();
            let (input, padded_width, padded_height) = model_input(&tile);
            let output = forward(&input, padded_height, padded_width)?;
            if output.len() != padded_width * padded_height {
                return Err(format!("확률 {}개, {padded_width}×{padded_height}여야 합니다", output.len()));
            }
            for y in top..bottom {
                let source = (y - tile_top) * padded_width + (left - tile_left);
                probabilities[y * width + left..y * width + right]
                    .copy_from_slice(&output[source..source + (right - left)]);
            }
        }
    }
    Ok(probabilities)
}

/// Bilinear resize (`F.interpolate(mode="bilinear", align_corners=False)`).
fn resize_bilinear(values: &[f32], width: usize, height: usize, to_width: usize, to_height: usize) -> Vec<f32> {
    let sample = |to: usize, from: usize, i: usize| {
        let position = ((i as f32 + 0.5) * from as f32 / to as f32 - 0.5).clamp(0.0, (from - 1) as f32);
        let low = position.floor() as usize;
        (low, (low + 1).min(from - 1), position - low as f32)
    };
    let columns: Vec<_> = (0..to_width).map(|x| sample(to_width, width, x)).collect();
    let mut output = Vec::with_capacity(to_width * to_height);
    for y in 0..to_height {
        let (top, bottom, fy) = sample(to_height, height, y);
        for &(left, right, fx) in &columns {
            let row = |row: usize| values[row * width + left] * (1.0 - fx) + values[row * width + right] * fx;
            output.push(row(top) * (1.0 - fy) + row(bottom) * fy);
        }
    }
    output
}

/// The glyph mask from probabilities: above `THRESHOLD`, closed with a 3×3 ellipse (a
/// cross, as OpenCV draws it), holes filled, grown by `GLYPH_DILATION` (the script's 3×3
/// dilation, twice).
fn clean(probabilities: &[f32], width: usize, height: usize) -> Vec<u8> {
    let mask: Vec<u8> = probabilities.iter().map(|p| if *p > THRESHOLD { 255 } else { 0 }).collect();
    let mut mask = erode_cross(&dilate_cross(&mask, width, height), width, height);
    fill_holes(&mut mask, width, height);
    dilate(&mask, width, height, GLYPH_DILATION)
}

/// Whether any of the pixel and its four neighbours inside the image is set (`any`) or all
/// of them are, pixels outside the image aside.
fn cross(mask: &[u8], width: usize, height: usize, any: bool) -> Vec<u8> {
    let mut output = vec![0u8; mask.len()];
    for y in 0..height {
        for x in 0..width {
            let mut values = [Some(mask[y * width + x]), None, None, None, None];
            if x > 0 {
                values[1] = Some(mask[y * width + x - 1]);
            }
            if x + 1 < width {
                values[2] = Some(mask[y * width + x + 1]);
            }
            if y > 0 {
                values[3] = Some(mask[(y - 1) * width + x]);
            }
            if y + 1 < height {
                values[4] = Some(mask[(y + 1) * width + x]);
            }
            let mut present = values.iter().flatten();
            let set = if any { present.any(|value| *value > 0) } else { present.all(|value| *value > 0) };
            output[y * width + x] = if set { 255 } else { 0 };
        }
    }
    output
}

fn dilate_cross(mask: &[u8], width: usize, height: usize) -> Vec<u8> {
    cross(mask, width, height, true)
}

fn erode_cross(mask: &[u8], width: usize, height: usize) -> Vec<u8> {
    cross(mask, width, height, false)
}

/// Fills the background not reachable from the image edge through 4-connected background
/// (the script's filled external contours: OpenCV traces 8-connected shapes).
fn fill_holes(mask: &mut [u8], width: usize, height: usize) {
    let mut outside = vec![false; mask.len()];
    let mut queue = Vec::new();
    let visit = |index: usize, outside: &mut [bool], queue: &mut Vec<usize>| {
        if mask[index] == 0 && !outside[index] {
            outside[index] = true;
            queue.push(index);
        }
    };
    for x in 0..width {
        visit(x, &mut outside, &mut queue);
        visit((height - 1) * width + x, &mut outside, &mut queue);
    }
    for y in 0..height {
        visit(y * width, &mut outside, &mut queue);
        visit(y * width + width - 1, &mut outside, &mut queue);
    }
    while let Some(index) = queue.pop() {
        let (x, y) = (index % width, index / width);
        if x > 0 {
            visit(index - 1, &mut outside, &mut queue);
        }
        if x + 1 < width {
            visit(index + 1, &mut outside, &mut queue);
        }
        if y > 0 {
            visit(index - width, &mut outside, &mut queue);
        }
        if y + 1 < height {
            visit(index + width, &mut outside, &mut queue);
        }
    }
    for (value, outside) in mask.iter_mut().zip(outside) {
        if !outside {
            *value = 255;
        }
    }
}

fn expand(rect: Rect, by: u32, width: u32, height: u32) -> Rect {
    let (x, y) = (rect.x.saturating_sub(by), rect.y.saturating_sub(by));
    let (right, bottom) = ((rect.x + rect.width + by).min(width), (rect.y + rect.height + by).min(height));
    Rect { x, y, width: right - x, height: bottom - y }
}

/// `startTextSegmentationDownload`: whatever is missing of the model (into
/// `models/manga-text-segmentation/`, checked against the pinned size and SHA-256) and the
/// LibTorch runtime.
pub fn download_segmentation(
    paths: &AppPaths,
    mut progress: impl FnMut(Download, u64, Option<u64>),
) -> Result<(), String> {
    let model = model_path(paths);
    if !model.is_file() {
        let dir = model.parent().expect("model path has a parent");
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        crate::lama::write_model_readme(dir, MODEL_URL, &model);
        let unverified = model.with_extension("safetensors.unverified");
        crate::lama::download(MODEL_URL, &unverified, |written, total| progress(Download::Model, written, total))?;
        let checked = check_model(&unverified, MODEL_SIZE, MODEL_SHA256);
        if checked.is_err() {
            let _ = std::fs::remove_file(&unverified);
        }
        checked?;
        std::fs::rename(&unverified, &model).map_err(|e| e.to_string())?;
    }
    torch_runtime::download(paths, |written, total| progress(Download::Runtime, written, total))
}

fn check_model(path: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let actual = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if actual != size {
        return Err(format!(
            "글자 분할 모델 크기가 맞지 않습니다 ({actual}바이트, {size}바이트여야 합니다). 다시 받아 주세요."
        ));
    }
    let actual = torch_runtime::sha256(path)?;
    if actual != sha256 {
        return Err(format!("글자 분할 모델 파일이 손상되었습니다 (SHA-256 {actual}). 다시 받아 주세요."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use mtt_core::model::BBox;

    use super::*;

    #[test]
    fn status_of_empty_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_root(dir.path());
        let status = status(&paths);
        assert!(!status.model_exists && !status.runtime_exists && !status.ready());
        assert!(status.model_path.ends_with("models/manga-text-segmentation/model.safetensors"));
        assert!(TextSegmentation::detect(&paths).is_none());
    }

    #[test]
    fn downloads_are_checked_by_size_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("model");
        std::fs::write(&file, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        check_model(&file, 3, abc).unwrap();
        assert!(check_model(&file, 4, abc).unwrap_err().contains("크기"));
        assert!(check_model(&file, 3, "00").unwrap_err().contains("SHA-256"));
    }

    #[test]
    fn input_is_normalized_and_zero_padded() {
        let image = RgbImage::from_fn(33, 2, |x, _| image::Rgb([if x == 0 { 255 } else { 0 }, 128, 0]));
        let (input, width, height) = model_input(&image);
        assert_eq!((width, height), (64, 32));
        let plane = width * height;
        assert_eq!(input.len(), 3 * plane);
        assert!((input[0] - (1.0 - 0.485) / 0.229).abs() < 1e-6);
        assert!((input[1] - (0.0 - 0.485) / 0.229).abs() < 1e-6);
        assert!((input[plane] - (128.0 / 255.0 - 0.456) / 0.224).abs() < 1e-6);
        // Padding past column 33 and row 2 is zero (the mean color).
        assert_eq!(input[33], 0.0);
        assert_eq!(input[2 * width], 0.0);
        assert_eq!(input[2 * plane + 63], 0.0);
    }

    #[test]
    fn transparency_goes_over_white() {
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        assert_eq!(over_white(&image).get_pixel(0, 0).0, [255, 255, 255]);
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        assert_eq!(over_white(&image).get_pixel(0, 0).0, [10, 20, 30]);
    }

    #[test]
    fn threshold_is_strict() {
        // Single pixels: the closing leaves them, the dilation then covers 5×5 around each.
        let mut probabilities = vec![0f32; 3 * 15];
        probabilities[7] = 0.5;
        probabilities[3 * 15 - 3] = 0.51;
        let mask = clean(&probabilities, 15, 3);
        assert_eq!(mask[7], 0, "0.5 is not above the threshold");
        assert_eq!(mask[15 + 12], 255);
        assert_eq!(mask[15 + 10], 255);
        assert_eq!(mask[15 + 9], 0);
    }

    #[test]
    fn closing_matches_opencv() {
        // `cv2.morphologyEx(MORPH_CLOSE)` with the 3×3 ellipse on two 3px bars a pixel
        // apart: the gap closes on the middle row only, and the image edge doesn't erode.
        let (width, height) = (9, 7);
        let mut mask = vec![0u8; width * height];
        for y in 2..5 {
            mask[y * width + 1] = 255;
            mask[y * width + 3] = 255;
        }
        let closed = erode_cross(&dilate_cross(&mask, width, height), width, height);
        let rows: Vec<Vec<u8>> =
            closed.chunks(width).map(|row| row.iter().map(|value| u8::from(*value > 0)).collect()).collect();
        let mut expected = vec![vec![0u8; width]; height];
        expected[2][1] = 1;
        expected[2][3] = 1;
        expected[3][..4].fill(1);
        expected[4][1] = 1;
        expected[4][3] = 1;
        assert_eq!(rows, expected);
        let dilated = dilate_cross(&mask, width, height);
        assert_eq!(dilated[width + 1], 255);
        assert_eq!(dilated[width], 0, "no corners");
    }

    #[test]
    fn holes_fill_but_the_outside_stays() {
        // A ring with a hole, and a C open to the edge.
        let (width, height) = (9, 5);
        let mut mask = vec![0u8; width * height];
        for (x, y) in [(1, 1), (2, 1), (3, 1), (1, 2), (3, 2), (1, 3), (2, 3), (3, 3)] {
            mask[y * width + x] = 255;
        }
        for (x, y) in [(6, 1), (7, 1), (8, 1), (6, 2), (6, 3), (7, 3), (8, 3)] {
            mask[y * width + x] = 255;
        }
        fill_holes(&mut mask, width, height);
        assert_eq!(mask[2 * width + 2], 255, "hole filled");
        assert_eq!(mask[2 * width + 7], 0, "open to the edge");
        assert_eq!(mask[0], 0);
        // A hole that only touches the outside diagonally is still a hole.
        let mut mask = vec![255u8; 9];
        mask[4] = 0;
        mask[0] = 0;
        fill_holes(&mut mask, 3, 3);
        assert_eq!(mask[4], 255);
        assert_eq!(mask[0], 0);
    }

    #[test]
    fn bilinear_resize_matches_align_corners_false() {
        let values = [0.0, 1.0];
        let up = resize_bilinear(&values, 2, 1, 4, 1);
        assert_eq!(up, vec![0.0, 0.25, 0.75, 1.0]);
        assert_eq!(resize_bilinear(&[0.5; 4], 2, 2, 3, 5), vec![0.5; 15]);
    }

    /// A stand-in for the model: probability 1 where the normalized red is below the mean
    /// (dark pixels), checking the input's size and padding.
    fn dark_pixels(input: &[f32], height: usize, width: usize) -> Result<Vec<f32>, String> {
        assert!(height.is_multiple_of(PAD_MULTIPLE) && width.is_multiple_of(PAD_MULTIPLE));
        assert!(height <= TILE_LIMIT + PAD_MULTIPLE && width <= TILE_LIMIT + PAD_MULTIPLE);
        Ok(input[..width * height].iter().map(|value| if *value < -1.0 { 1.0 } else { 0.0 }).collect())
    }

    #[test]
    fn tiles_cover_big_crops_at_the_right_places() {
        let (width, height) = (2500u32, 1100u32);
        let crop = RgbaImage::from_fn(width, height, |x, y| {
            let dark = (x % 97 < 3) || (y % 89 < 3);
            let value = if dark { 0 } else { 255 };
            image::Rgba([value, value, value, 255])
        });
        let mut calls = 0;
        let probabilities = probabilities_by_tiles(&over_white(&crop), &mut |input, h, w| {
            calls += 1;
            dark_pixels(input, h, w)
        })
        .unwrap();
        assert_eq!(calls, 3 * 2);
        for (index, probability) in probabilities.iter().enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let dark = (x % 97 < 3) || (y % 89 < 3);
            assert_eq!(*probability, if dark { 1.0 } else { 0.0 }, "({x}, {y})");
        }
    }

    #[test]
    fn big_pages_are_segmented_scaled_down() {
        let crop = RgbaImage::from_fn(300, 200, |x, _| {
            let value = if (100..140).contains(&x) { 0 } else { 255 };
            image::Rgba([value, value, value, 255])
        });
        let mut sizes = Vec::new();
        let mask = segment(&crop, MAX_PAGE_SIDE * 2, |input, h, w| {
            sizes.push((w, h));
            dark_pixels(input, h, w)
        })
        .unwrap();
        assert_eq!(sizes, vec![(160, 128)], "150×100, padded");
        assert_eq!(mask.dimensions(), (300, 200));
        assert_eq!(mask.get_pixel(120, 100)[0], 255);
        assert_eq!(mask.get_pixel(98, 100)[0], 255, "dilated by 2");
        assert_eq!(mask.get_pixel(95, 100)[0], 0);
        assert_eq!(mask.get_pixel(20, 100)[0], 0);
    }

    fn block(x: f64, y: f64, w: f64, h: f64, page: (f64, f64)) -> TranslationBlock {
        let bbox = BBox::new(x / page.0 * 1000.0, y / page.1 * 1000.0, w / page.0 * 1000.0, h / page.1 * 1000.0);
        TranslationBlock { bbox, ..TranslationBlock::default() }
    }

    fn paint(source: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32, v: u8) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                source.put_pixel(x, y, image::Rgba([v, v, v, 255]));
            }
        }
    }

    #[test]
    fn failing_model_falls_back_to_the_heuristic() {
        let mut source = RgbaImage::from_pixel(400, 300, image::Rgba([255, 255, 255, 255]));
        paint(&mut source, 150, 120, 10, 40, 10);
        let block = block(100.0, 100.0, 200.0, 100.0, (400.0, 300.0));
        let heuristic = text_mask::mask_for_blocks(&source, &[&block], None);
        let failed = mask_for_blocks_with(&source, &[&block], None, |_, _| Err("no model".into()));
        assert_eq!(failed, heuristic);
        let wrong_size = mask_for_blocks_with(&source, &[&block], None, |_, _| Ok(GrayImage::new(3, 3)));
        assert_eq!(wrong_size, heuristic);
        assert_eq!(mask_for_blocks(&source, &[&block], None, None), heuristic);
        assert!(heuristic.get_pixel(155, 140)[0] > 0);
    }

    /// A stand-in model that sees only pixels of luma `glyph` as text, dilated like the
    /// cleaning does.
    fn glyphs_of(glyph: u8) -> impl FnOnce(&RgbaImage, u32) -> Result<GrayImage, String> {
        move |crop, _| {
            let (width, height) = (crop.width() as usize, crop.height() as usize);
            let raw: Vec<u8> = crop.pixels().map(|p| if p[0] == glyph { 255 } else { 0 }).collect();
            Ok(GrayImage::from_raw(crop.width(), crop.height(), dilate(&raw, width, height, GLYPH_DILATION)).unwrap())
        }
    }

    #[test]
    fn model_glyphs_replace_the_heuristic_strokes() {
        // 400×300 page (growth radius 2): in the block a glyph and a dark bar the heuristic
        // takes for text; outside it, text the model sees but no block asks for.
        let page = (400.0, 300.0);
        let mut source = RgbaImage::from_pixel(400, 300, image::Rgba([255, 255, 255, 255]));
        paint(&mut source, 150, 120, 10, 40, 10);
        // A thin stroke of the glyph the model is unsure of.
        paint(&mut source, 160, 140, 30, 1, 60);
        paint(&mut source, 230, 130, 6, 20, 30);
        paint(&mut source, 60, 240, 10, 10, 10);
        let target = block(100.0, 100.0, 200.0, 100.0, page);
        let heuristic = text_mask::mask_for_blocks(&source, &[&target], None);
        assert!(heuristic.get_pixel(232, 140)[0] > 0, "the heuristic takes the bar");
        let mut seen = None;
        let mask = mask_for_blocks_with(&source, &[&target], None, |crop, side| {
            seen = Some((crop.dimensions(), side));
            glyphs_of(10)(crop, side)
        });
        // Box 98..302 × 98..202, grown by 2, plus 64 of context.
        assert_eq!(seen, Some(((336, 236), 400)));
        assert_eq!(mask.get_pixel(155, 140)[0], 255, "glyph");
        assert_eq!(mask.get_pixel(148, 140)[0], 255, "the model's dilation");
        assert_eq!(mask.get_pixel(147, 140)[0], 0, "no growth past the box radius");
        assert_eq!(mask.get_pixel(189, 140)[0], 255, "the heuristic's stroke touching the glyph");
        assert_eq!(mask.get_pixel(232, 140)[0], 0, "the model's call: not text");
        assert_eq!(mask.get_pixel(65, 245)[0], 0, "outside the blocks");
    }

    #[test]
    fn boxes_the_model_finds_nothing_in_keep_the_heuristic() {
        let page = (600.0, 300.0);
        let mut source = RgbaImage::from_pixel(600, 300, image::Rgba([255, 255, 255, 255]));
        paint(&mut source, 150, 120, 10, 40, 10);
        // Colored lettering the model misses.
        paint(&mut source, 420, 120, 10, 40, 30);
        let blocks = [block(100.0, 100.0, 120.0, 100.0, page), block(380.0, 100.0, 120.0, 100.0, page)];
        let refs: Vec<&TranslationBlock> = blocks.iter().collect();
        let mask = mask_for_blocks_with(&source, &refs, None, glyphs_of(10));
        assert_eq!(mask.get_pixel(155, 140)[0], 255);
        assert_eq!(mask.get_pixel(425, 140)[0], 255, "heuristic strokes");
        assert_eq!(mask.get_pixel(418, 140)[0], 255, "grown by the full radius");
    }

    #[test]
    fn pieces_inside_the_box_join_the_glyphs_but_outlines_stay() {
        // 2048×400 page (growth radius 6), box 898..1142 × 148..252. The model sees the
        // glyph only; a dash above it (outside the box's centre, so the heuristic drops it)
        // and a balloon line under it crossing the box both touch its mask.
        let page = (2048.0, 400.0);
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([255, 255, 255, 255]));
        paint(&mut source, 1000, 162, 10, 44, 10);
        paint(&mut source, 990, 159, 25, 2, 60);
        paint(&mut source, 880, 207, 280, 3, 20);
        let target = block(900.0, 150.0, 240.0, 100.0, page);
        let heuristic = text_mask::mask_for_blocks(&source, &[&target], None);
        assert_eq!(heuristic.get_pixel(990, 159)[0], 0, "the heuristic drops the dash");
        let mask = mask_for_blocks_with(&source, &[&target], None, glyphs_of(10));
        assert_eq!(mask.get_pixel(990, 159)[0], 255, "dash joined");
        assert_eq!(mask.get_pixel(1014, 190)[0], 255, "grown by 6 in all");
        assert_eq!(mask.get_pixel(1014, 208)[0], 0, "balloon line kept");
        assert_eq!(mask.get_pixel(1005, 207)[0], 255, "except under the glyph mask itself");
        assert_eq!(mask.get_pixel(1005, 208)[0], 0);
    }
}
