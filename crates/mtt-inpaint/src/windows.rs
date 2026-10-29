//! Model inpainting window by window (IOPaint's crop strategy plus `koharu`'s bubble fill):
//! mask components on a plain background are filled with that color, and the rest go to
//! the model in windows around groups of components, each at most `WINDOW_LIMIT` pixels a
//! side so a page full of text never becomes one huge forward pass.

use image::{GrayImage, RgbaImage};

use crate::{Rect, dilate};

/// Context kept around each group of components.
const WINDOW_MARGIN: u32 = 128;
/// Longest window side handed to the model. Groups only merge while they fit, and a
/// single component that doesn't is scaled down to it.
const WINDOW_LIMIT: u32 = 1024;
/// A component sits on a plain background when the ring of pixels this far from it is
/// one color: at least `PLAIN_SHARE` of them within `PLAIN_TOLERANCE` of the median.
const RING_INNER: usize = 2;
const RING_OUTER: usize = 5;
const PLAIN_TOLERANCE: u8 = 24;
const PLAIN_SHARE: f32 = 0.97;
const PLAIN_MIN_RING: usize = 16;

/// Fills `mask` (coverage, white = inpaint) in `source`, running `forward` on windows for
/// what isn't plain background. `forward` gets a window and its mask and returns the
/// window inpainted. Pixels outside the mask are returned unchanged.
pub fn inpaint_by_windows(
    source: &RgbaImage,
    mask: &GrayImage,
    mut forward: impl FnMut(&RgbaImage, &GrayImage) -> Result<RgbaImage, String>,
) -> Result<RgbaImage, String> {
    let (width, height) = source.dimensions();
    let (labels, bounds) = label_components(mask);
    let mut output = source.clone();
    // Masked pixels the model still has to fill.
    let mut pending: Vec<bool> = mask.as_raw().iter().map(|value| *value > 0).collect();
    let mut model_components = Vec::new();
    for (index, component) in bounds.iter().enumerate() {
        let label = index as u32 + 1;
        let Some(color) = plain_background(source, &labels, label, *component) else {
            model_components.push(index);
            continue;
        };
        for (x, y, page) in pixels(*component, width) {
            if labels[page] == label {
                output.get_pixel_mut(x, y).0[..3].copy_from_slice(&color);
                pending[page] = false;
            }
        }
    }
    let mut groups = group_components(&bounds, &model_components, width, height);
    groups.sort_by_key(|(content, _)| (content.y, content.x));
    let mut group_of = vec![usize::MAX; bounds.len()];
    for (group, (_, members)) in groups.iter().enumerate() {
        for member in members {
            group_of[*member] = group;
        }
    }
    for (group, (content, _)) in groups.iter().enumerate() {
        let content = *content;
        let window = expand(content, WINDOW_MARGIN, width, height);
        let image = image::imageops::crop_imm(&output, window.x, window.y, window.width, window.height).to_image();
        let window_mask = GrayImage::from_fn(window.width, window.height, |x, y| {
            let page = ((window.y + y) * width + window.x + x) as usize;
            image::Luma([if pending[page] { mask.as_raw()[page] } else { 0 }])
        });
        let filled = forward_within_limit(&image, &window_mask, &mut forward)?;
        for (x, y, page) in pixels(window, width) {
            let label = labels[page];
            if label != 0 && group_of[label as usize - 1] == group {
                let color = filled.get_pixel(x - window.x, y - window.y).0;
                output.get_pixel_mut(x, y).0[..3].copy_from_slice(&color[..3]);
                pending[page] = false;
            }
        }
    }
    Ok(output)
}

/// `forward` on `image`, scaled down first when a side exceeds `WINDOW_LIMIT`.
fn forward_within_limit(
    image: &RgbaImage,
    mask: &GrayImage,
    forward: &mut impl FnMut(&RgbaImage, &GrayImage) -> Result<RgbaImage, String>,
) -> Result<RgbaImage, String> {
    use image::imageops::{FilterType, resize};
    let (width, height) = image.dimensions();
    let long = width.max(height);
    let filled = if long <= WINDOW_LIMIT {
        forward(image, mask)?
    } else {
        let scale = WINDOW_LIMIT as f32 / long as f32;
        let (w, h) = (((width as f32 * scale).round() as u32).max(1), ((height as f32 * scale).round() as u32).max(1));
        // Any coverage left after scaling stays masked, so no text survives at the edges.
        let mut small_mask = resize(mask, w, h, FilterType::Triangle);
        small_mask.iter_mut().filter(|value| **value > 0).for_each(|value| *value = 255);
        let small = forward(&resize(image, w, h, FilterType::Triangle), &small_mask)?;
        if small.dimensions() != (w, h) {
            return Err("인페인트 결과 크기가 맞지 않습니다.".into());
        }
        resize(&small, width, height, FilterType::Triangle)
    };
    if filled.dimensions() != (width, height) {
        return Err("인페인트 결과 크기가 맞지 않습니다.".into());
    }
    Ok(filled)
}

/// 8-connected components of the nonzero mask pixels: a label per pixel (0 = unmasked,
/// `n` = the `n - 1`th component) and each component's bounds.
fn label_components(mask: &GrayImage) -> (Vec<u32>, Vec<Rect>) {
    let (width, height) = (mask.width() as usize, mask.height() as usize);
    let raw = mask.as_raw();
    let mut labels = vec![0u32; raw.len()];
    let mut bounds = Vec::new();
    let mut stack = Vec::new();
    for start in 0..raw.len() {
        if raw[start] == 0 || labels[start] != 0 {
            continue;
        }
        let label = bounds.len() as u32 + 1;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
        labels[start] = label;
        stack.push(start);
        while let Some(index) = stack.pop() {
            let (x, y) = (index % width, index / width);
            (min_x, max_x) = (min_x.min(x), max_x.max(x));
            (min_y, max_y) = (min_y.min(y), max_y.max(y));
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let neighbor = ny * width + nx;
                    if raw[neighbor] > 0 && labels[neighbor] == 0 {
                        labels[neighbor] = label;
                        stack.push(neighbor);
                    }
                }
            }
        }
        bounds.push(Rect {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        });
    }
    (labels, bounds)
}

/// The color around component `label` when it sits on a plain background.
fn plain_background(source: &RgbaImage, labels: &[u32], label: u32, bounds: Rect) -> Option<[u8; 3]> {
    let (width, height) = source.dimensions();
    let area = expand(bounds, RING_OUTER as u32, width, height);
    let (area_width, area_height) = (area.width as usize, area.height as usize);
    let own: Vec<u8> = pixels(area, width).map(|(_, _, page)| if labels[page] == label { 255 } else { 0 }).collect();
    let outer = dilate(&own, area_width, area_height, RING_OUTER);
    let inner = dilate(&own, area_width, area_height, RING_INNER - 1);
    let ring: Vec<[u8; 3]> = pixels(area, width)
        .enumerate()
        .filter(|(index, (_, _, page))| outer[*index] > 0 && inner[*index] == 0 && labels[*page] == 0)
        .map(|(_, (x, y, _))| {
            let p = source.get_pixel(x, y).0;
            [p[0], p[1], p[2]]
        })
        .collect();
    if ring.len() < PLAIN_MIN_RING {
        return None;
    }
    let median = [0, 1, 2].map(|channel| {
        let mut values: Vec<u8> = ring.iter().map(|p| p[channel]).collect();
        let middle = values.len() / 2;
        *values.select_nth_unstable(middle).1
    });
    let close = ring.iter().filter(|p| (0..3).all(|c| p[c].abs_diff(median[c]) <= PLAIN_TOLERANCE)).count();
    (close as f32 >= ring.len() as f32 * PLAIN_SHARE).then_some(median)
}

/// Groups of `members` (indices into `bounds`) whose windows overlap, merged as long as
/// the merged window stays within `WINDOW_LIMIT`: each group's bounds and members.
fn group_components(bounds: &[Rect], members: &[usize], width: u32, height: u32) -> Vec<(Rect, Vec<usize>)> {
    let mut groups: Vec<(Rect, Vec<usize>)> = members.iter().map(|index| (bounds[*index], vec![*index])).collect();
    let fits = |content: Rect| {
        let window = expand(content, WINDOW_MARGIN, width, height);
        window.width.max(window.height) <= WINDOW_LIMIT
    };
    loop {
        let mut merged = false;
        let mut i = 0;
        while i < groups.len() {
            let mut j = i + 1;
            while j < groups.len() {
                let window = expand(groups[i].0, WINDOW_MARGIN, width, height);
                let other = expand(groups[j].0, WINDOW_MARGIN, width, height);
                let content = union(groups[i].0, groups[j].0);
                if touch(window, other) && fits(content) {
                    let (_, absorbed) = groups.swap_remove(j);
                    groups[i].0 = content;
                    groups[i].1.extend(absorbed);
                    merged = true;
                    // The window grew, so earlier candidates may touch it now.
                    j = i + 1;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if !merged {
            return groups;
        }
    }
}

/// `(x, y, page index)` of every pixel in `rect`, row by row.
fn pixels(rect: Rect, width: u32) -> impl Iterator<Item = (u32, u32, usize)> {
    (rect.y..rect.y + rect.height)
        .flat_map(move |y| (rect.x..rect.x + rect.width).map(move |x| (x, y, (y * width + x) as usize)))
}

fn expand(rect: Rect, by: u32, width: u32, height: u32) -> Rect {
    let x = rect.x.saturating_sub(by);
    let y = rect.y.saturating_sub(by);
    let right = (rect.x + rect.width + by).min(width);
    let bottom = (rect.y + rect.height + by).min(height);
    Rect { x, y, width: right - x, height: bottom - y }
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect { x, y, width: right - x, height: bottom - y }
}

fn touch(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(mask: &mut GrayImage, rect: Rect) {
        for (x, y, _) in pixels(rect, mask.width()) {
            mask.put_pixel(x, y, image::Luma([255]));
        }
    }

    #[test]
    fn components_are_labeled_with_bounds() {
        let mut mask = GrayImage::new(10, 6);
        fill(&mut mask, Rect { x: 1, y: 1, width: 2, height: 2 });
        mask.put_pixel(3, 3, image::Luma([255])); // diagonal neighbour joins the first
        fill(&mut mask, Rect { x: 7, y: 0, width: 2, height: 6 });
        let (labels, bounds) = label_components(&mask);
        // Numbered in raster order of their first pixel.
        assert_eq!(bounds, vec![Rect { x: 7, y: 0, width: 2, height: 6 }, Rect { x: 1, y: 1, width: 3, height: 3 }]);
        assert_eq!(labels[3 * 10 + 3], 2);
        assert_eq!(labels[0], 0);
    }

    #[test]
    fn text_on_plain_background_is_filled_without_the_model() {
        let mut source = RgbaImage::from_pixel(80, 80, image::Rgba([250, 248, 246, 255]));
        let mut mask = GrayImage::new(80, 80);
        for y in 30..40 {
            source.put_pixel(40, y, image::Rgba([0, 0, 0, 255]));
        }
        fill(&mut mask, Rect { x: 39, y: 29, width: 3, height: 12 });
        let result = inpaint_by_windows(&source, &mask, |_, _| panic!("no model needed")).unwrap();
        assert_eq!(result.get_pixel(40, 35).0, [250, 248, 246, 255]);
    }

    #[test]
    fn text_on_texture_goes_to_the_model_with_margin() {
        let source = RgbaImage::from_fn(400, 300, |x, y| {
            let v = if (x / 3 + y / 3) % 2 == 0 { 40 } else { 220 };
            image::Rgba([v, v, v, 255])
        });
        let mut mask = GrayImage::new(400, 300);
        fill(&mut mask, Rect { x: 200, y: 150, width: 10, height: 10 });
        let mut windows = Vec::new();
        let result = inpaint_by_windows(&source, &mask, |image, mask| {
            windows.push(image.dimensions());
            assert!(mask.get_pixel(WINDOW_MARGIN + 5, WINDOW_MARGIN + 5)[0] > 0);
            Ok(RgbaImage::from_pixel(image.width(), image.height(), image::Rgba([1, 2, 3, 255])))
        })
        .unwrap();
        assert_eq!(windows, vec![(10 + 2 * WINDOW_MARGIN, 10 + 2 * WINDOW_MARGIN)]);
        assert_eq!(result.get_pixel(205, 155).0, [1, 2, 3, 255]);
        assert_eq!(result.get_pixel(150, 150), source.get_pixel(150, 150), "outside the mask untouched");
    }

    #[test]
    fn windows_merge_only_within_the_limit() {
        // A column of components 250 pixels apart: each window touches the next, and the
        // merged window would pass the limit at the fifth. The last one is far away.
        let mut bounds: Vec<Rect> = (0..5).map(|i| Rect { x: 100, y: 100 + 250 * i, width: 10, height: 10 }).collect();
        bounds.push(Rect { x: 100, y: 2500, width: 10, height: 10 });
        let groups = group_components(&bounds, &[0, 1, 2, 3, 4, 5], 3000, 3000);
        let mut members: Vec<Vec<usize>> = groups
            .into_iter()
            .map(|(_, mut members)| {
                members.sort_unstable();
                members
            })
            .collect();
        members.sort();
        assert_eq!(members, vec![vec![0, 1, 2, 3], vec![4], vec![5]]);
    }

    #[test]
    fn oversized_windows_are_scaled_to_the_limit() {
        let source = RgbaImage::from_fn(1600, 400, |x, y| {
            let v = if (x / 2 + y / 2) % 2 == 0 { 30 } else { 230 };
            image::Rgba([v, v, v, 255])
        });
        let mut mask = GrayImage::new(1600, 400);
        fill(&mut mask, Rect { x: 50, y: 150, width: 1500, height: 60 });
        let mut seen = Vec::new();
        let result = inpaint_by_windows(&source, &mask, |image, _| {
            seen.push(image.dimensions());
            Ok(RgbaImage::from_pixel(image.width(), image.height(), image::Rgba([9, 9, 9, 255])))
        })
        .unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, WINDOW_LIMIT);
        assert_eq!(result.get_pixel(800, 180).0, [9, 9, 9, 255]);
    }
}
