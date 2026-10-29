//! Speech balloons around blocks, without a model: from the background inside a block's
//! box, fill outward through pixels of that color and the block's glyphs, stopping at the
//! balloon outline. A fill that stays inside a window around the box is the balloon. Blocks
//! sharing it (one balloon with two texts, or balloons joined at a neck) split it at the
//! narrowest cross-section between them. What's left, minus some air, is where the
//! translation goes, row by row, so lines can follow the outline. koharu fits text to
//! detected balloon contours the same way, with a segmentation model.

use image::RgbaImage;
use mtt_core::model::{BBox, TranslationBlock};

use crate::text_mask::{rect_strokes, scan_rect};
use crate::{Rect, erode};

/// The fill may reach this many box sizes past the box on each side; touching that
/// window's edge means it leaked out of the balloon. The window also covers the windows
/// overlapping it, so a balloon shared with nearby boxes fits.
const WINDOW_REACH: f64 = 1.0;
/// Pixels within this of the background color are inside.
const BACKGROUND_TOLERANCE: u8 = 48;
/// The balloon must be at least this much bigger than the box to be worth using.
const MIN_GROWTH: f64 = 1.15;
/// Air between the text and the balloon outline, as a share of the balloon's shorter side.
const AIR_SHARE: f64 = 0.06;
/// Between two blocks in one fill, a cross-section this much narrower than at both boxes
/// is a neck joining two balloons; without one, the gap between the boxes is split evenly.
const NECK_SHARE: f64 = 0.8;
/// Rows in a balloon's text shape.
const SHAPE_ROWS: usize = 40;

/// A balloon found around a block, in 0..1000 page space: its bounds (its part, when it
/// is shared), the box text can use, and the usable span of each row of that box (see
/// [`TranslationBlock::text_shape`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Balloon {
    pub bounds: BBox,
    pub text_area: BBox,
    pub shape: Vec<[f64; 2]>,
}

/// The balloon around each of `blocks` (all on `source`): `None` where a box isn't on a
/// plain background or the fill leaks out (text on art, a balloon open to the panel).
pub fn find_balloons(source: &RgbaImage, blocks: &[TranslationBlock]) -> Vec<Option<Balloon>> {
    let (width, height) = source.dimensions();
    let scans: Vec<Option<Rect>> = blocks.iter().map(|block| scan_rect(block, width, height)).collect();
    (0..blocks.len()).map(|index| balloon_for(source, &scans, index)).collect()
}

/// `scan` grown by [`WINDOW_REACH`] box sizes on each side.
fn reach_window(scan: Rect, width: u32, height: u32) -> Option<Rect> {
    let reach = (f64::from(scan.width.max(scan.height)) * WINDOW_REACH) as f32;
    Rect::clamped(
        scan.x as f32 - reach,
        scan.y as f32 - reach,
        scan.width as f32 + reach * 2.0,
        scan.height as f32 + reach * 2.0,
        width,
        height,
    )
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    let (right, bottom) = ((a.x + a.width).max(b.x + b.width), (a.y + a.height).max(b.y + b.height));
    Rect { x, y, width: right - x, height: bottom - y }
}

/// A box in window coordinates: `(x, y, width, height)`.
type LocalBox = (f64, f64, f64, f64);
/// A pixel box in window coordinates: `(x, y, width, height)`.
type PixelBox = (usize, usize, usize, usize);

fn balloon_for(source: &RgbaImage, scans: &[Option<Rect>], index: usize) -> Option<Balloon> {
    let (width, height) = source.dimensions();
    let scan = scans[index]?;
    let found = rect_strokes(source, scan);
    let (background, plain) = found.edge?;
    if !plain {
        return None;
    }
    let own = reach_window(scan, width, height)?;
    let window = scans
        .iter()
        .flatten()
        .filter_map(|other| reach_window(*other, width, height))
        .filter(|other| overlaps(own, *other))
        .fold(own, union);
    let (ww, wh) = (window.width as usize, window.height as usize);
    // Inside: the background color, or text-colored pixels in the box (its glyphs, kept
    // as text or not, so a glyph cut by the box edge doesn't wall the fill in).
    let mut passable = vec![false; ww * wh];
    for y in 0..wh {
        for x in 0..ww {
            let p = source.get_pixel(window.x + x as u32, window.y + y as u32).0;
            passable[y * ww + x] = (0..3).all(|c| p[c].abs_diff(background[c]) <= BACKGROUND_TOLERANCE);
        }
    }
    let sw = scan.width as usize;
    for (index, candidate) in found.candidates.iter().enumerate() {
        if *candidate > 0 {
            let (x, y) = ((scan.x - window.x) as usize + index % sw, (scan.y - window.y) as usize + index / sw);
            passable[y * ww + x] = true;
        }
    }
    // Seeds: background pixels in the middle half of the box.
    let (bx, by) = ((scan.x - window.x) as usize, (scan.y - window.y) as usize);
    let (bw, bh) = (scan.width as usize, scan.height as usize);
    let seeds: Vec<usize> = (by + bh / 4..by + bh * 3 / 4)
        .flat_map(|y| (bx + bw / 4..bx + bw * 3 / 4).map(move |x| y * ww + x))
        .filter(|index| passable[*index])
        .collect();
    // Reaching the window's edge (unless it is the page's) means the fill escaped.
    let escapes = |x: usize, y: usize| {
        (x == 0 && window.x > 0)
            || (y == 0 && window.y > 0)
            || (x == ww - 1 && window.x + window.width < width)
            || (y == wh - 1 && window.y + window.height < height)
    };
    let inside = flood(&passable, ww, wh, &seeds, escapes)?;
    // Holes (glyph edges, dots inside letters, other blocks' text) count as inside.
    let mut region = fill_holes(&inside, ww, wh);
    let local = |rect: Rect| -> LocalBox {
        (
            f64::from(rect.x) - f64::from(window.x),
            f64::from(rect.y) - f64::from(window.y),
            f64::from(rect.width),
            f64::from(rect.height),
        )
    };
    let own_box = local(scan);
    for (other_index, other) in scans.iter().enumerate() {
        let Some(other) = other.filter(|_| other_index != index).map(local) else { continue };
        let (cx, cy) = (other.0 + other.2 / 2.0, other.1 + other.3 / 2.0);
        if cx >= 0.0 && cy >= 0.0 && (cx as usize) < ww && (cy as usize) < wh && region[cy as usize * ww + cx as usize]
        {
            cut_towards(&mut region, ww, own_box, other);
        }
    }
    // What stays connected to this block.
    let seeds: Vec<usize> = seeds.into_iter().filter(|index| region[*index]).collect();
    let region = flood(&region, ww, wh, &seeds, |_, _| false)?;
    let (min_x, min_y, max_x, max_y) = bounds(&region, ww)?;
    let (fw, fh) = (max_x - min_x + 1, max_y - min_y + 1);
    if ((fw * fh) as f64) < (bw * bh) as f64 * MIN_GROWTH {
        return None;
    }
    let air = (fw.min(fh) as f64 * AIR_SHARE).round().max(1.0) as usize;
    let mask: Vec<u8> = region.iter().map(|inside| if *inside { 255 } else { 0 }).collect();
    let usable: Vec<bool> = erode(&mask, ww, wh, air).into_iter().map(|value| value > 0).collect();
    let (text, shape) = text_shape(&usable, ww)?;
    let shape_area: f64 = shape.iter().map(|[left, right]| (right - left).max(0.0)).sum::<f64>()
        * text.2 as f64
        * (text.3 as f64 / shape.len() as f64);
    if shape_area < (bw * bh) as f64 * 0.5 {
        return None;
    }
    let to_bbox = |x: usize, y: usize, w: usize, h: usize| {
        let (fx, fy) = (f64::from(width) / 1000.0, f64::from(height) / 1000.0);
        BBox::new(
            (window.x as usize + x) as f64 / fx,
            (window.y as usize + y) as f64 / fy,
            w as f64 / fx,
            h as f64 / fy,
        )
    };
    Some(Balloon { bounds: to_bbox(min_x, min_y, fw, fh), text_area: to_bbox(text.0, text.1, text.2, text.3), shape })
}

/// Pixels of `passable` connected to `seeds`; `None` without seeds or when the fill
/// reaches a pixel `escapes` says is outside.
fn flood(
    passable: &[bool],
    width: usize,
    height: usize,
    seeds: &[usize],
    escapes: impl Fn(usize, usize) -> bool,
) -> Option<Vec<bool>> {
    if seeds.is_empty() {
        return None;
    }
    let mut inside = vec![false; passable.len()];
    let mut stack = seeds.to_vec();
    for index in seeds {
        inside[*index] = true;
    }
    while let Some(index) = stack.pop() {
        let (x, y) = (index % width, index / width);
        if escapes(x, y) {
            return None;
        }
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for next in neighbours.into_iter().flatten() {
            if passable[next] && !inside[next] {
                inside[next] = true;
                stack.push(next);
            }
        }
    }
    Some(inside)
}

/// `(min_x, min_y, max_x, max_y)` of the set pixels.
fn bounds(region: &[bool], width: usize) -> Option<(usize, usize, usize, usize)> {
    region.iter().enumerate().filter(|(_, inside)| **inside).fold(None, |bounds, (index, _)| {
        let (x, y) = (index % width, index / width);
        Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y)),
        })
    })
}

/// Removes the part of `region` on `other`'s side of where it is narrowest between the
/// boxes, measured across the line joining their centres: a neck, or the middle of the gap
/// when there isn't one.
fn cut_towards(region: &mut [bool], width: usize, own: LocalBox, other: LocalBox) {
    let centre = |b: LocalBox| (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0);
    let ((ax, ay), (bx, by)) = (centre(own), centre(other));
    let length = (bx - ax).hypot(by - ay);
    if length < 1.0 {
        return;
    }
    let (ux, uy) = ((bx - ax) / length, (by - ay) / length);
    let project = |x: f64, y: f64| (x - ax) * ux + (y - ay) * uy;
    let corners = |b: LocalBox| [(b.0, b.1), (b.0 + b.2, b.1), (b.0, b.1 + b.3), (b.0 + b.2, b.1 + b.3)];
    let own_edge = corners(own).iter().map(|(x, y)| project(*x, *y)).fold(f64::NEG_INFINITY, f64::max);
    let other_edge = corners(other).iter().map(|(x, y)| project(*x, *y)).fold(f64::INFINITY, f64::min);
    let (low, high) = (own_edge.max(0.0).ceil() as usize, other_edge.min(length).floor() as usize);
    let cut = if high > low + 1 {
        // Cross-section sizes: region pixels at each distance along the line.
        let mut sizes = vec![0usize; high + 1];
        for (index, inside) in region.iter().enumerate() {
            let s = project((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
            if *inside && s >= low as f64 && s < (high + 1) as f64 {
                sizes[s as usize] += 1;
            }
        }
        let middle = (low + high) / 2;
        let narrowest = (low..=high).min_by_key(|s| (sizes[*s], s.abs_diff(middle))).unwrap_or(middle);
        let neck = (sizes[narrowest] as f64) < NECK_SHARE * sizes[low].min(sizes[high]) as f64;
        if neck { narrowest as f64 + 0.5 } else { (own_edge + other_edge) / 2.0 }
    } else {
        (own_edge + other_edge) / 2.0
    };
    for (index, inside) in region.iter_mut().enumerate() {
        if *inside && project((index % width) as f64 + 0.5, (index / width) as f64 + 0.5) > cut {
            *inside = false;
        }
    }
}

/// The box text can use in `usable` and the usable span of each of its rows
/// ([`SHAPE_ROWS`] rows, as fractions of the box width): each pixel row's run through the
/// region's middle column, every pixel row in a shape row sharing it. The box is
/// `(x, y, width, height)`.
fn text_shape(usable: &[bool], width: usize) -> Option<(PixelBox, Vec<[f64; 2]>)> {
    let (min_x, min_y, max_x, max_y) = bounds(usable, width)?;
    let axis = (min_x + max_x) / 2;
    let runs: Vec<Option<(usize, usize)>> = (min_y..=max_y)
        .map(|y| {
            let row = &usable[y * width..(y + 1) * width];
            row[axis].then(|| {
                let left = (min_x..=axis).rev().take_while(|x| row[*x]).last().unwrap_or(axis);
                let right = (axis..=max_x).take_while(|x| row[*x]).last().unwrap_or(axis);
                (left, right + 1)
            })
        })
        .collect();
    let first = runs.iter().position(Option::is_some)?;
    let last = runs.iter().rposition(Option::is_some)?;
    let runs = &runs[first..=last];
    let left = runs.iter().flatten().map(|run| run.0).min()?;
    let right = runs.iter().flatten().map(|run| run.1).max()?;
    let (text_width, text_height) = (right - left, runs.len());
    let rows = SHAPE_ROWS.min(text_height);
    let shape = (0..rows)
        .map(|row| {
            let band =
                &runs[row * text_height / rows..((row + 1) * text_height / rows).max(row * text_height / rows + 1)];
            let span = band.iter().try_fold((left, right), |(l, r), run| run.map(|run| (l.max(run.0), r.min(run.1))));
            match span {
                // Inward rounding keeps the span inside the run.
                Some((l, r)) if r > l => [
                    (((l - left) as f64 / text_width as f64) * 1000.0).ceil() / 1000.0,
                    (((r - left) as f64 / text_width as f64) * 1000.0).floor() / 1000.0,
                ],
                _ => [0.0, 0.0],
            }
        })
        .collect();
    Some(((left, min_y + first, text_width, text_height), shape))
}

/// `inside` plus every pixel not connected to the border through outside pixels.
fn fill_holes(inside: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut outside = vec![false; inside.len()];
    let mut stack: Vec<usize> = (0..width)
        .flat_map(|x| [x, (height - 1) * width + x])
        .chain((0..height).flat_map(|y| [y * width, y * width + width - 1]))
        .filter(|index| !inside[*index])
        .collect();
    for index in &stack {
        outside[*index] = true;
    }
    while let Some(index) = stack.pop() {
        let (x, y) = (index % width, index / width);
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for next in neighbours.into_iter().flatten() {
            if !inside[next] && !outside[next] {
                outside[next] = true;
                stack.push(next);
            }
        }
    }
    outside.into_iter().map(|out| !out).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000×1000 screentone page with balloons drawn white with a 3px black outline:
    /// the union of `ellipses` (centre x, centre y, radius x, radius y), and dark text
    /// columns (left x, top y, bottom y), each 14px wide.
    fn page(ellipses: &[(f64, f64, f64, f64)], columns: &[(u32, u32, u32)]) -> RgbaImage {
        let mut page = RgbaImage::from_fn(1000, 1000, |x, y| {
            let v = if (x / 3 + y / 3) % 2 == 0 { 90 } else { 200 };
            image::Rgba([v, v, v, 255])
        });
        let radius = |x: u32, y: u32, (cx, cy, rx, ry): (f64, f64, f64, f64)| {
            let (dx, dy) = ((f64::from(x) - cx) / rx, (f64::from(y) - cy) / ry);
            (dx * dx + dy * dy).sqrt()
        };
        for y in 0..1000u32 {
            for x in 0..1000u32 {
                let r = ellipses.iter().map(|e| radius(x, y, *e)).fold(f64::INFINITY, f64::min);
                if r < 0.985 {
                    page.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
                } else if r <= 1.0 {
                    page.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        for &(x0, top, bottom) in columns {
            for y in top..bottom {
                for x in x0..x0 + 14 {
                    if y % 30 < 24 {
                        page.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
                    }
                }
            }
        }
        page
    }

    fn block(x: f64, y: f64, w: f64, h: f64) -> TranslationBlock {
        TranslationBlock { bbox: BBox::new(x, y, w, h), ..TranslationBlock::default() }
    }

    /// Every row of `balloon`'s shape lies inside the ellipse `(cx, cy, rx, ry)`.
    fn assert_shape_inside(balloon: &Balloon, (cx, cy, rx, ry): (f64, f64, f64, f64)) {
        let (t, rows) = (balloon.text_area, balloon.shape.len() as f64);
        for (row, [left, right]) in balloon.shape.iter().enumerate() {
            if right <= left {
                continue;
            }
            for y in [t.y + t.h * row as f64 / rows, t.y + t.h * (row + 1) as f64 / rows] {
                for x in [t.x + t.w * left, t.x + t.w * right] {
                    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
                    assert!(dx * dx + dy * dy < 1.0, "row {row} reaches ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn finds_the_balloon_and_its_shape() {
        let ellipse = (500.0, 400.0, 220.0, 160.0);
        let page = page(&[ellipse], &[(475, 320, 480), (515, 320, 480)]);
        // The model's box: tight around the two columns.
        let found = find_balloons(&page, &[block(468.0, 312.0, 68.0, 176.0)]);
        let balloon = found[0].clone().unwrap();
        let b = balloon.bounds;
        assert!((b.x - 280.0).abs() < 6.0 && (b.w - 440.0).abs() < 12.0, "{b:?}");
        assert!((b.y - 240.0).abs() < 6.0 && (b.h - 320.0).abs() < 12.0, "{b:?}");
        let t = balloon.text_area;
        assert!(t.w > 330.0 && t.h > 240.0, "most of the balloon: {t:?}");
        assert_eq!(balloon.shape.len(), SHAPE_ROWS);
        let width = |row: usize| balloon.shape[row][1] - balloon.shape[row][0];
        assert!(width(SHAPE_ROWS / 2) > 0.95, "the middle row spans the box");
        assert!(width(0) < 0.6 && width(SHAPE_ROWS - 1) < 0.6, "the ends narrow with the outline");
        assert_shape_inside(&balloon, ellipse);
    }

    #[test]
    fn text_on_art_or_an_open_balloon_has_none() {
        let mut page = page(&[(500.0, 400.0, 220.0, 160.0)], &[(475, 320, 480), (515, 320, 480)]);
        // A gap in the outline lets the fill out.
        for y in 380..420 {
            for x in 715..1000 {
                page.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        let found = find_balloons(&page, &[block(468.0, 312.0, 68.0, 176.0), block(50.0, 800.0, 80.0, 120.0)]);
        assert_eq!(found[0], None, "leaks through the gap");
        assert_eq!(found[1], None, "no plain background");
    }

    #[test]
    fn joined_balloons_split_at_the_neck() {
        // Two lobes joined around x = 500, where the outline pinches to about 85px.
        let (left, right) = ((380.0, 400.0, 130.0, 110.0), (620.0, 400.0, 130.0, 110.0));
        let page = page(&[left, right], &[(330, 330, 470), (370, 330, 470), (600, 330, 470), (640, 330, 470)]);
        let found = find_balloons(&page, &[block(325.0, 325.0, 65.0, 150.0), block(595.0, 325.0, 65.0, 150.0)]);
        let (a, b) = (found[0].clone().unwrap(), found[1].clone().unwrap());
        let (a_right, b_left) = (a.bounds.x + a.bounds.w, b.bounds.x);
        assert!((a_right - 500.0).abs() < 8.0 && (b_left - 500.0).abs() < 8.0, "{:?} {:?}", a.bounds, b.bounds);
        assert!(a.text_area.x + a.text_area.w <= b.text_area.x, "text areas apart");
        assert_shape_inside(&a, left);
        assert_shape_inside(&b, right);
    }

    #[test]
    fn two_blocks_in_one_balloon_share_it() {
        // No neck: the gap between the boxes (380..620) splits at its middle.
        let ellipse = (500.0, 400.0, 260.0, 130.0);
        let page = page(&[ellipse], &[(335, 335, 465), (630, 335, 465)]);
        let found = find_balloons(&page, &[block(330.0, 330.0, 50.0, 140.0), block(620.0, 330.0, 50.0, 140.0)]);
        let (a, b) = (found[0].clone().unwrap(), found[1].clone().unwrap());
        let (a_right, b_left) = (a.bounds.x + a.bounds.w, b.bounds.x);
        assert!((a_right - 500.0).abs() < 6.0 && (b_left - 500.0).abs() < 6.0, "{:?} {:?}", a.bounds, b.bounds);
        assert!((a.bounds.x - 240.0).abs() < 6.0, "the rest of the balloon: {:?}", a.bounds);
    }
}
