//! Minimal PSD support for the inpaint round trip (`inpaintPsd.ts`).
//!
//! Export writes an 8-bit RGB document with the source as a locked-name background layer,
//! the masked inpaint result, an optional hidden translation layer and a hidden group of
//! editable text layers (see [`crate::psd_text`]). Import flattens every visible
//! non-background layer (groups, opacity, RLE or raw channels) into a new result layer and
//! a mask covering its painted pixels; text layers and the text group are left out.

use image::{GrayImage, RgbaImage};

use crate::psd_text::{TextLayer, type_tool_info};

pub const BACKGROUND_LAYER_NAME: &str = "배경 (이름변경금지)";
pub const RESULT_LAYER_NAME: &str = "Inpaint Result";
pub const TRANSLATION_LAYER_NAME: &str = "번역 블록";
/// Group of the editable text layers; never imported, even when rasterized.
pub const TEXT_GROUP_NAME: &str = "번역 텍스트 (편집용)";
/// Name Photoshop gives the record closing a group.
pub const GROUP_END_NAME: &str = "</Layer group>";

pub struct ExportLayer<'a> {
    pub name: &'a str,
    pub image: &'a RgbaImage,
    /// Page position of the image's top-left pixel.
    pub left: i32,
    pub top: i32,
    pub hidden: bool,
    /// `lsct` section type of group records: 3 below the children, 1 (open) or 2 (closed)
    /// above them, with the group's name and visibility. `None` for normal layers.
    pub section: Option<u32>,
    /// Editable text (`TySh`); `image` is how it renders.
    pub text: Option<&'a TextLayer>,
}

impl<'a> ExportLayer<'a> {
    pub fn new(name: &'a str, image: &'a RgbaImage, hidden: bool) -> Self {
        Self { name, image, left: 0, top: 0, hidden, section: None, text: None }
    }
}

// ---------------------------------------------------------------------------------- write

pub(crate) fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// ASCII fallback name as a Pascal string padded to a multiple of 4 bytes.
fn pascal_name(name: &str) -> Vec<u8> {
    let ascii: Vec<u8> = name.bytes().filter(|b| b.is_ascii() && !b.is_ascii_control()).take(255).collect();
    // Korean names keep only punctuation in ASCII; the real name is in `luni`.
    let ascii = if ascii.iter().any(u8::is_ascii_alphanumeric) { ascii } else { b"Layer".to_vec() };
    let mut out = vec![ascii.len() as u8];
    out.extend_from_slice(&ascii);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// Additional layer info `key` with `data` padded to a multiple of 4 bytes.
fn layer_info_block(key: &[u8; 4], mut data: Vec<u8>) -> Vec<u8> {
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let mut out = b"8BIM".to_vec();
    out.extend_from_slice(key);
    push_u32(&mut out, data.len() as u32);
    out.extend(data);
    out
}

/// `luni` additional layer info with the UTF-16 name.
fn unicode_name_block(name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut data = Vec::new();
    push_u32(&mut data, units.len() as u32);
    for unit in units {
        push_u16(&mut data, unit);
    }
    layer_info_block(b"luni", data)
}

/// PackBits (Apple RLE, what PSD calls compression 1): runs of 3+ equal bytes as
/// `257 - n, byte`, everything else as `n - 1, bytes…`, each at most 128 long.
fn pack_bits(row: &[u8], out: &mut Vec<u8>) {
    let mut index = 0;
    let mut literal_start = 0;
    let flush_literal = |out: &mut Vec<u8>, bytes: &[u8]| {
        for chunk in bytes.chunks(128) {
            out.push((chunk.len() - 1) as u8);
            out.extend_from_slice(chunk);
        }
    };
    while index < row.len() {
        let value = row[index];
        let run = row[index..].iter().take(128).take_while(|byte| **byte == value).count();
        if run >= 3 {
            flush_literal(out, &row[literal_start..index]);
            out.push((257 - run) as u8);
            out.push(value);
            index += run;
            literal_start = index;
        } else {
            index += run;
        }
    }
    flush_literal(out, &row[literal_start..]);
}

/// One image's channels, PackBits per row: the per-row byte counts of every channel, then
/// the packed rows (`channels` picks byte offsets within RGBA pixels).
fn packed_channels(image: &RgbaImage, channels: &[usize]) -> Vec<(Vec<u16>, Vec<u8>)> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut row = Vec::with_capacity(width);
    channels
        .iter()
        .map(|channel| {
            let mut counts = Vec::with_capacity(height);
            let mut data = Vec::new();
            for pixels in (0..height).map(|y| &image.as_raw()[y * width * 4..][..width * 4]) {
                row.clear();
                row.extend(pixels.iter().skip(*channel).step_by(4));
                let start = data.len();
                pack_bits(&row, &mut data);
                counts.push((data.len() - start) as u16);
            }
            (counts, data)
        })
        .collect()
}

/// Writes a PSD; `layers` are bottom to top. The composite is `composite` (opaque RGB).
/// Channels are RLE compressed: result and translation layers are mostly transparent.
/// Group records can have an empty (0×0) image.
pub fn write_psd(width: u32, height: u32, composite: &RgbaImage, layers: &[ExportLayer<'_>]) -> Vec<u8> {
    let layer_channels: Vec<Vec<(Vec<u16>, Vec<u8>)>> =
        layers.iter().map(|layer| packed_channels(layer.image, &[3, 0, 1, 2])).collect();
    let mut out = Vec::with_capacity(1024);
    out.extend_from_slice(b"8BPS");
    push_u16(&mut out, 1);
    out.extend_from_slice(&[0; 6]);
    push_u16(&mut out, 3);
    push_u32(&mut out, height);
    push_u32(&mut out, width);
    push_u16(&mut out, 8);
    push_u16(&mut out, 3);
    push_u32(&mut out, 0); // color mode data
    push_u32(&mut out, 0); // image resources

    let mut layer_info = Vec::new();
    push_u16(&mut layer_info, layers.len() as u16);
    let mut text_index = 0;
    for (index, layer) in layers.iter().enumerate() {
        push_i32(&mut layer_info, layer.top);
        push_i32(&mut layer_info, layer.left);
        push_i32(&mut layer_info, layer.top + layer.image.height() as i32);
        push_i32(&mut layer_info, layer.left + layer.image.width() as i32);
        push_u16(&mut layer_info, 4);
        // Transparency first, like Photoshop.
        for (id, (counts, data)) in [-1i16, 0, 1, 2].into_iter().zip(&layer_channels[index]) {
            layer_info.extend_from_slice(&id.to_be_bytes());
            push_u32(&mut layer_info, (2 + counts.len() * 2 + data.len()) as u32);
        }
        layer_info.extend_from_slice(b"8BIMnorm");
        layer_info.extend_from_slice(&[255, 0, if layer.hidden { 2 } else { 0 }, 0]);
        let mut extra = Vec::new();
        push_u32(&mut extra, 0); // layer mask
        push_u32(&mut extra, 0); // blending ranges
        extra.extend(pascal_name(layer.name));
        extra.extend(unicode_name_block(layer.name));
        if let Some(section) = layer.section {
            extra.extend_from_slice(b"8BIMlsct");
            push_u32(&mut extra, 4);
            push_u32(&mut extra, section);
        }
        if let Some(text) = layer.text {
            extra.extend(layer_info_block(b"TySh", type_tool_info(text, text_index)));
            text_index += 1;
        }
        push_u32(&mut layer_info, extra.len() as u32);
        layer_info.extend(extra);
    }
    for channels in &layer_channels {
        for (counts, data) in channels {
            push_u16(&mut layer_info, 1);
            counts.iter().for_each(|count| push_u16(&mut layer_info, *count));
            layer_info.extend_from_slice(data);
        }
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }
    push_u32(&mut out, layer_info.len() as u32 + 4 + 4);
    push_u32(&mut out, layer_info.len() as u32);
    out.extend(layer_info);
    push_u32(&mut out, 0); // global layer mask

    // The composite: all row counts first, then all rows.
    let composite_channels = packed_channels(composite, &[0, 1, 2]);
    push_u16(&mut out, 1);
    for (counts, _) in &composite_channels {
        counts.iter().for_each(|count| push_u16(&mut out, *count));
    }
    for (_, data) in &composite_channels {
        out.extend_from_slice(data);
    }
    out
}

// ----------------------------------------------------------------------------------- read

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(len).filter(|end| *end <= self.data.len()).ok_or("PSD 파일이 잘렸습니다.")?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
}

/// A rectangle in document pixels, right and bottom exclusive. In `i64`, so the size of any
/// rectangle a file can state neither overflows nor wraps.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    top: i64,
    left: i64,
    bottom: i64,
    right: i64,
}

impl Bounds {
    fn read(reader: &mut Reader<'_>) -> Result<Self, String> {
        let [top, left, bottom, right] = [reader.i32()?, reader.i32()?, reader.i32()?, reader.i32()?].map(i64::from);
        Ok(Self { top, left, bottom, right })
    }

    /// 0 when right is left of left; at most 2³² - 1.
    fn width(self) -> u64 {
        (self.right - self.left).max(0) as u64
    }

    fn height(self) -> u64 {
        (self.bottom - self.top).max(0) as u64
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            top: self.top.max(other.top),
            left: self.left.max(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.min(other.right),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct LayerRecord {
    bounds: Bounds,
    channels: Vec<(i16, u32)>,
    opacity: u8,
    hidden: bool,
    name: String,
    /// `lsct` section type: 1/2 folder, 3 group end marker.
    section: u32,
    /// Carries type tool data (`TySh`).
    text: bool,
}

#[derive(Debug, Clone)]
pub struct PsdLayer {
    pub name: String,
    /// Page position of `image`, which is the part of the layer on the canvas.
    pub left: i32,
    pub top: i32,
    pub image: RgbaImage,
    pub opacity: f32,
    pub hidden: bool,
    /// An editable text layer (it has `TySh` type tool data).
    pub text: bool,
    /// Names of the groups the layer is in, innermost first.
    pub groups: Vec<String>,
}

#[derive(Debug)]
pub struct PsdDocument {
    pub width: u32,
    pub height: u32,
    /// Leaf layers bottom to top with group visibility/opacity folded in.
    pub layers: Vec<PsdLayer>,
}

/// PackBits decoding of one row.
fn unpack_bits(input: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < input.len() && out.len() < expected {
        let header = input[i] as i8;
        i += 1;
        if header >= 0 {
            let count = header as usize + 1;
            let end = (i + count).min(input.len());
            out.extend_from_slice(&input[i..end]);
            i = end;
        } else if header != -128 {
            let count = (1 - header as isize) as usize;
            let value = *input.get(i).ok_or("PSD RLE 데이터가 잘렸습니다.")?;
            i += 1;
            out.extend(std::iter::repeat_n(value, count));
        }
    }
    out.resize(expected, 0);
    Ok(out)
}

/// Photoshop's largest document side (a PSB's; a PSD's is 30000).
const MAX_SIDE: u32 = 300_000;

const TRUNCATED_LAYER: &str = "PSD 레이어 데이터가 잘렸습니다.";

/// Checks that a channel's `data` holds what a `bounds`-sized layer needs, before anything
/// is allocated for it: raw rows, or RLE row byte counts and the rows they count.
fn check_channel(data: &[u8], bounds: Bounds) -> Result<(), String> {
    if data.len() < 2 {
        return Ok(());
    }
    let (width, height) = (bounds.width(), bounds.height());
    let available = data.len() as u64 - 2;
    match u16::from_be_bytes([data[0], data[1]]) {
        0 if width * height <= available => Ok(()),
        1 => {
            let table = height * 2;
            if table > available {
                return Err(TRUNCATED_LAYER.into());
            }
            let rows: u64 = data[2..2 + table as usize]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|count| u64::from(u16::from_be_bytes(*count)))
                .sum();
            // A PackBits byte stands for at most 64 pixels (a run of 128 is 2 bytes), so
            // fewer can't be the layer's rows.
            if rows > available - table || rows.saturating_mul(64) < width * height {
                return Err(TRUNCATED_LAYER.into());
            }
            Ok(())
        }
        0 => Err(TRUNCATED_LAYER.into()),
        other => Err(format!("지원하지 않는 PSD 압축 방식입니다 ({other}). 압축 없이 또는 RLE로 저장하세요.")),
    }
}

/// Decodes the part of a `bounds`-sized channel inside `clip` (`check_channel` passed),
/// handing `put` each value with its position in `clip`. A channel without data is zeros.
fn decode_channel(
    data: &[u8],
    bounds: Bounds,
    clip: Bounds,
    mut put: impl FnMut(usize, usize, u8),
) -> Result<(), String> {
    let clip = clip.intersect(bounds);
    let (width, rows) = (clip.width() as usize, clip.height() as usize);
    if width == 0 || rows == 0 {
        return Ok(());
    }
    let (skip_x, skip_y) = ((clip.left - bounds.left) as usize, (clip.top - bounds.top) as usize);
    if data.len() < 2 {
        (0..rows).for_each(|y| (0..width).for_each(|x| put(x, y, 0)));
        return Ok(());
    }
    let stride = bounds.width() as usize;
    if u16::from_be_bytes([data[0], data[1]]) == 0 {
        for y in 0..rows {
            let row = &data[2 + (skip_y + y) * stride + skip_x..][..width];
            row.iter().enumerate().for_each(|(x, value)| put(x, y, *value));
        }
        return Ok(());
    }
    let height = bounds.height() as usize;
    let counts = &data[2..2 + height * 2];
    let mut offset = 2 + counts.len();
    for (row, count) in counts.as_chunks::<2>().0.iter().enumerate().take(skip_y + rows) {
        let count = usize::from(u16::from_be_bytes(*count));
        let packed = &data[offset..offset + count];
        offset += count;
        if row >= skip_y {
            let pixels = unpack_bits(packed, skip_x + width)?;
            pixels[skip_x..].iter().enumerate().for_each(|(x, value)| put(x, row - skip_y, *value));
        }
    }
    Ok(())
}

/// The record's color and transparency channels where the layer is on `canvas`.
fn decode_layer(
    record: &LayerRecord,
    channels: &[(i16, &[u8])],
    canvas: Bounds,
) -> Result<(Bounds, RgbaImage), String> {
    let pixel_channels = channels.iter().filter(|(id, _)| (-1..=2).contains(id));
    for (_, data) in pixel_channels.clone() {
        check_channel(data, record.bounds)?;
    }
    let area = record.bounds.intersect(canvas);
    // At most the canvas, which fits u32.
    let width = area.width() as u32;
    let mut image = RgbaImage::from_pixel(width, area.height() as u32, image::Rgba([0, 0, 0, 255]));
    for (id, data) in pixel_channels {
        let channel = if *id == -1 { 3 } else { *id as usize };
        let pixels = image.as_mut();
        decode_channel(data, record.bounds, area, |x, y, value| {
            pixels[(y * width as usize + x) * 4 + channel] = value;
        })?;
    }
    Ok((area, image))
}

fn parse_extra(reader: &mut Reader<'_>, record: &mut LayerRecord, length: usize) -> Result<(), String> {
    let mut extra = Reader { data: reader.take(length)?, pos: 0 };
    let mask_len = extra.u32()? as usize;
    extra.take(mask_len)?;
    let ranges_len = extra.u32()? as usize;
    extra.take(ranges_len)?;
    let name_len = extra.u8()? as usize;
    let name = extra.take(name_len)?;
    record.name = String::from_utf8_lossy(name).into_owned();
    let padded = (1 + name_len).div_ceil(4) * 4;
    extra.take(padded - 1 - name_len)?;
    while extra.remaining() >= 12 {
        let signature = extra.take(4)?;
        if signature != b"8BIM" && signature != b"8B64" {
            break;
        }
        let key = extra.take(4)?;
        let len = extra.u32()? as usize;
        let Ok(block) = extra.take(len) else { break };
        match key {
            b"luni" if block.len() >= 4 => {
                let count = u32::from_be_bytes([block[0], block[1], block[2], block[3]]) as usize;
                let units: Vec<u16> = block[4..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .take(count)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                record.name = String::from_utf16_lossy(&units);
            }
            b"lsct" | b"lsdk" if block.len() >= 4 => {
                record.section = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
            }
            b"TySh" => record.text = true,
            _ => {}
        }
    }
    Ok(())
}

pub fn read_psd(data: &[u8]) -> Result<PsdDocument, String> {
    let mut reader = Reader { data, pos: 0 };
    if reader.take(4)? != b"8BPS" {
        return Err("PSD 파일이 아닙니다.".into());
    }
    if reader.u16()? != 1 {
        return Err("PSB(대형 문서)는 지원하지 않습니다.".into());
    }
    reader.take(6)?;
    let _channels = reader.u16()?;
    let height = reader.u32()?;
    let width = reader.u32()?;
    let depth = reader.u16()?;
    let mode = reader.u16()?;
    if depth != 8 || mode != 3 {
        return Err(format!("8비트 RGB PSD만 지원합니다 (현재: {depth}비트, 색상 모드 {mode})."));
    }
    if width.max(height) > MAX_SIDE {
        return Err(format!("PSD 크기가 너무 큽니다 ({width}x{height}, 한 변 최대 {MAX_SIDE}px)."));
    }
    let color_len = reader.u32()? as usize;
    reader.take(color_len)?;
    let resources_len = reader.u32()? as usize;
    reader.take(resources_len)?;
    let layer_mask_len = reader.u32()? as usize;
    if layer_mask_len == 0 {
        return Ok(PsdDocument { width, height, layers: Vec::new() });
    }
    let layer_info_len = reader.u32()? as usize;
    if layer_info_len == 0 {
        return Ok(PsdDocument { width, height, layers: Vec::new() });
    }
    let count = reader.i16()?.unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let mut record = LayerRecord { bounds: Bounds::read(&mut reader)?, ..LayerRecord::default() };
        let channel_count = reader.u16()?;
        for _ in 0..channel_count {
            record.channels.push((reader.i16()?, reader.u32()?));
        }
        reader.take(8)?; // blend mode signature + key
        record.opacity = reader.u8()?;
        reader.u8()?; // clipping
        let flags = reader.u8()?;
        record.hidden = flags & 2 != 0;
        reader.u8()?;
        let extra_len = reader.u32()? as usize;
        parse_extra(&mut reader, &mut record, extra_len)?;
        records.push(record);
    }
    let canvas = Bounds { top: 0, left: 0, bottom: i64::from(height), right: i64::from(width) };
    let mut images = Vec::with_capacity(count);
    for record in &records {
        // Every channel's bytes must be in the file before any of the layer is allocated.
        let channels = record
            .channels
            .iter()
            .map(|(id, length)| Ok((*id, reader.take(*length as usize)?)))
            .collect::<Result<Vec<_>, String>>()?;
        images.push(decode_layer(record, &channels, canvas)?);
    }

    // Fold groups (bottom-to-top order: end marker, children, folder record).
    let mut stack: Vec<Vec<PsdLayer>> = vec![Vec::new()];
    for (record, (area, image)) in records.into_iter().zip(images) {
        match record.section {
            3 => stack.push(Vec::new()),
            1 | 2 => {
                let children = if stack.len() > 1 { stack.pop().unwrap_or_default() } else { Vec::new() };
                let opacity = f32::from(record.opacity) / 255.0;
                let parent = stack.last_mut().expect("root frame");
                parent.extend(children.into_iter().map(|mut child| {
                    child.hidden |= record.hidden;
                    child.opacity *= opacity;
                    child.groups.push(record.name.clone());
                    child
                }));
            }
            _ => stack.last_mut().expect("root frame").push(PsdLayer {
                name: record.name,
                // Inside the canvas, whose sides fit i32.
                left: area.left as i32,
                top: area.top as i32,
                image,
                opacity: f32::from(record.opacity) / 255.0,
                hidden: record.hidden,
                text: record.text,
                groups: Vec::new(),
            }),
        }
    }
    let layers = stack.into_iter().flatten().collect();
    Ok(PsdDocument { width, height, layers })
}

/// Layers that make up the inpaint result: visible, not the background, not text.
fn is_paint_layer(layer: &PsdLayer) -> bool {
    !layer.hidden
        && !layer.text
        && layer.name.trim() != BACKGROUND_LAYER_NAME
        && !layer.groups.iter().any(|group| group.trim() == TEXT_GROUP_NAME)
}

/// `importInpaintPsd`: merges visible non-background, non-text layers into (result, mask).
pub fn import_inpaint_layers(
    document: &PsdDocument,
    width: u32,
    height: u32,
) -> Result<(RgbaImage, GrayImage), String> {
    if (document.width, document.height) != (width, height) {
        return Err(format!(
            "PSD 크기가 현재 페이지와 다릅니다. PSD: {}x{}, 페이지: {width}x{height}",
            document.width, document.height
        ));
    }
    let paint: Vec<&PsdLayer> = document.layers.iter().filter(|layer| is_paint_layer(layer)).collect();
    if paint.is_empty() {
        return Err("배경 레이어를 제외하고 가져올 PSD 레이어가 없습니다.".into());
    }
    let mut merged = RgbaImage::new(width, height);
    for layer in paint {
        for (x, y, pixel) in layer.image.enumerate_pixels() {
            let (tx, ty) = (layer.left + x as i32, layer.top + y as i32);
            if tx < 0 || ty < 0 || tx >= width as i32 || ty >= height as i32 {
                continue;
            }
            let alpha = f32::from(pixel[3]) / 255.0 * layer.opacity;
            if alpha <= 0.0 {
                continue;
            }
            let dst = merged.get_pixel_mut(tx as u32, ty as u32);
            let dst_alpha = f32::from(dst[3]) / 255.0;
            let out_alpha = alpha + dst_alpha * (1.0 - alpha);
            for c in 0..3 {
                dst[c] = ((f32::from(pixel[c]) * alpha + f32::from(dst[c]) * dst_alpha * (1.0 - alpha)) / out_alpha)
                    .round() as u8;
            }
            dst[3] = (out_alpha * 255.0).round() as u8;
        }
    }
    if merged.pixels().all(|p| p[3] == 0) {
        return Err("배경 레이어를 제외한 PSD 레이어가 비어 있습니다.".into());
    }
    let mask =
        GrayImage::from_fn(width, height, |x, y| image::Luma([if merged.get_pixel(x, y)[3] > 0 { 255 } else { 0 }]));
    Ok((merged, mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_round_trip_with_korean_names() {
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        let mut result = RgbaImage::new(4, 3);
        result.put_pixel(1, 1, image::Rgba([200, 100, 50, 255]));
        let translation = RgbaImage::from_pixel(4, 3, image::Rgba([0, 0, 0, 255]));
        let bytes = write_psd(
            4,
            3,
            &source,
            &[
                ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
                ExportLayer::new(RESULT_LAYER_NAME, &result, false),
                ExportLayer::new(TRANSLATION_LAYER_NAME, &translation, true),
            ],
        );
        let document = read_psd(&bytes).unwrap();
        assert_eq!(document.layers.len(), 3);
        assert_eq!(document.layers[0].name, BACKGROUND_LAYER_NAME);
        assert!(document.layers[2].hidden);
        let (merged, mask) = import_inpaint_layers(&document, 4, 3).unwrap();
        assert_eq!(merged.get_pixel(1, 1).0, [200, 100, 50, 255]);
        assert_eq!(merged.get_pixel(0, 0)[3], 0, "background and hidden layers excluded");
        assert_eq!(mask.get_pixel(1, 1)[0], 255);
        assert!(import_inpaint_layers(&document, 5, 3).is_err());
    }

    #[test]
    fn packbits_round_trips_and_shrinks_flat_rows() {
        let mut row: Vec<u8> = (0..700).map(|i| (i * 37 % 251) as u8).collect();
        row[100..400].fill(0);
        row[500..502].fill(9);
        for sample in [row.clone(), vec![0; 1000], vec![5], vec![1, 1], vec![1, 1, 1], Vec::new()] {
            let mut packed = Vec::new();
            pack_bits(&sample, &mut packed);
            assert_eq!(unpack_bits(&packed, sample.len()).unwrap(), sample);
        }
        let mut packed = Vec::new();
        pack_bits(&[0; 1000], &mut packed);
        assert!(packed.len() <= 16, "{} bytes", packed.len());
    }

    #[test]
    fn packbits_decoding() {
        // 3 literal bytes, then 4 repeats of 0xAA, then a no-op.
        let row = unpack_bits(&[2, 1, 2, 3, (-3i8) as u8, 0xAA, (-128i8) as u8], 7).unwrap();
        assert_eq!(row, vec![1, 2, 3, 0xAA, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    fn groups_fold_visibility_and_opacity() {
        let empty = RgbaImage::new(2, 2);
        let paint = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        // Bottom to top: [hidden group: end marker, child, folder], visible top layer.
        let bytes = write_psd(
            2,
            2,
            &empty,
            &[
                ExportLayer { section: Some(3), ..ExportLayer::new("</Layer group>", &empty, false) },
                ExportLayer::new("숨은 그룹 안", &paint, false),
                ExportLayer { section: Some(1), ..ExportLayer::new("그룹", &empty, true) },
                ExportLayer::new("보이는 레이어", &paint, false),
            ],
        );
        let document = read_psd(&bytes).unwrap();
        let names: Vec<(&str, bool)> = document.layers.iter().map(|l| (l.name.as_str(), l.hidden)).collect();
        assert_eq!(names, [("숨은 그룹 안", true), ("보이는 레이어", false)]);
    }

    fn text_layer(text: &str) -> TextLayer {
        TextLayer {
            text: text.into(),
            font: "AppleSDGothicNeo-Regular".into(),
            font_size: 2.0,
            faux_bold: false,
            faux_italic: false,
            leading: 2.4,
            tracking: 0,
            color: [0, 0, 0, 255],
            justification: crate::psd_text::Justification::Center,
            vertical: false,
            box_width: 3.0,
            box_height: 2.0,
            transform: [1.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn text_layers_are_written_and_left_out_of_the_import() {
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        let mut result = RgbaImage::new(4, 3);
        result.put_pixel(1, 1, image::Rgba([200, 100, 50, 255]));
        let base = [
            ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
            ExportLayer::new(RESULT_LAYER_NAME, &result, false),
        ];
        let plain = read_psd(&write_psd(4, 3, &source, &base)).unwrap();

        let (empty, glyphs) = (RgbaImage::new(0, 0), RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 255])));
        let (first, second) = (text_layer("첫 줄\n둘째 줄"), text_layer("밖"));
        let text =
            |name, layer| ExportLayer { left: 2, top: 2, text: Some(layer), ..ExportLayer::new(name, &glyphs, false) };
        let mut layers = Vec::from(base);
        layers.extend([
            // A visible text group, a visible text layer and a rasterized one inside it.
            ExportLayer { section: Some(3), ..ExportLayer::new(GROUP_END_NAME, &empty, false) },
            text("첫 줄", &first),
            ExportLayer { left: 2, top: 2, ..ExportLayer::new("래스터화된 텍스트", &glyphs, false) },
            ExportLayer { section: Some(2), ..ExportLayer::new(TEXT_GROUP_NAME, &empty, false) },
            // A text layer outside the group.
            text("밖", &second),
        ]);
        let bytes = write_psd(4, 3, &source, &layers);
        let contains = |needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
        assert!(contains(b"8BIMTySh") && contains(b"EngineData"));
        let utf16: Vec<u8> = "둘째 줄".encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert!(contains(&utf16));

        let document = read_psd(&bytes).unwrap();
        let summary: Vec<(&str, i32, u32, bool, Vec<String>)> = document
            .layers
            .iter()
            .map(|layer| (layer.name.as_str(), layer.left, layer.image.width(), layer.text, layer.groups.clone()))
            .collect();
        let in_group = vec![TEXT_GROUP_NAME.to_owned()];
        assert_eq!(
            summary[2..],
            [
                ("첫 줄", 2, 2, true, in_group.clone()),
                ("래스터화된 텍스트", 2, 2, false, in_group),
                ("밖", 2, 2, true, Vec::new()),
            ]
        );
        assert_eq!(import_inpaint_layers(&document, 4, 3).unwrap(), import_inpaint_layers(&plain, 4, 3).unwrap());
    }

    /// A hand-made layer record: top, left, bottom, right and the channels' ids and lengths.
    type Crafted<'a> = ([i32; 4], &'a [(i16, u32)]);

    /// A 4×3 PSD around hand-made layer records followed by `channel_data`, the way a broken
    /// or hostile file can be.
    fn crafted(records: &[Crafted<'_>], channel_data: &[u8]) -> Vec<u8> {
        let mut info = Vec::new();
        push_u16(&mut info, records.len() as u16);
        for (bounds, channels) in records {
            bounds.iter().for_each(|value| push_i32(&mut info, *value));
            push_u16(&mut info, channels.len() as u16);
            for (id, length) in *channels {
                info.extend_from_slice(&id.to_be_bytes());
                push_u32(&mut info, *length);
            }
            info.extend_from_slice(b"8BIMnorm");
            info.extend_from_slice(&[255, 0, 0, 0]);
            let extra = [0, 0, 0, 0, 0, 0, 0, 0, 3, b'a', b'b', b'c'];
            push_u32(&mut info, extra.len() as u32);
            info.extend_from_slice(&extra);
        }
        info.extend_from_slice(channel_data);
        let mut bytes = write_psd(4, 3, &RgbaImage::new(4, 3), &[]);
        let layer_section = 26 + 4 + 4;
        bytes.truncate(layer_section);
        push_u32(&mut bytes, info.len() as u32 + 8);
        push_u32(&mut bytes, info.len() as u32);
        bytes.extend(info);
        push_u32(&mut bytes, 0);
        bytes
    }

    #[test]
    fn broken_layer_records_are_errors_before_anything_is_allocated() {
        let raw_header = [0u8, 0];
        let cases: [(&str, Vec<u8>); 6] = [
            // Used to overflow `right - left` (a panic).
            ("overflowing bounds", crafted(&[([0, i32::MIN, 1, i32::MAX], &[(0, 2)])], &raw_header)),
            // Used to overflow the image buffer's length (a panic).
            ("2³¹ square", crafted(&[([0, 0, i32::MAX, i32::MAX], &[(0, 2)])], &raw_header)),
            // Used to allocate 1.6GB before finding the data missing.
            ("20000 square", crafted(&[([0, 0, 20000, 20000], &[(0, 2)])], &raw_header)),
            ("RLE without rows", {
                let mut data = vec![0, 1];
                data.extend(std::iter::repeat_n(0, 2 * 20000));
                crafted(&[([0, 0, 20000, 20000], &[(0, data.len() as u32)])], &data)
            }),
            ("channel past the end", crafted(&[([0, 0, 2, 2], &[(0, 1000)])], &[0, 0, 1, 2, 3, 4])),
            ("RLE rows past the channel", crafted(&[([0, 0, 1, 2], &[(0, 6)])], &[0, 1, 0, 9, 1, 7])),
        ];
        for (case, bytes) in cases {
            let error = read_psd(&bytes).unwrap_err();
            assert!(error.contains("잘렸습니다"), "{case}: {error}");
        }
        // Raw and RLE data that fits reads.
        let fits = crafted(&[([0, 0, 1, 2], &[(0, 4), (1, 7)])], &[0, 0, 10, 20, 0, 1, 0, 3, 1, 30, 40]);
        let layer = &read_psd(&fits).unwrap().layers[0];
        assert_eq!(layer.image.as_raw(), &[10, 30, 0, 255, 20, 40, 0, 255]);
        let mut huge = write_psd(1, 1, &RgbaImage::new(1, 1), &[]);
        huge[14..22].copy_from_slice(&[0, 0x10, 0, 0, 0, 0, 0, 1]);
        assert!(read_psd(&huge).unwrap_err().contains("너무 큽니다"));
    }

    #[test]
    fn layers_past_the_canvas_keep_only_their_part_on_it() {
        let paint = RgbaImage::from_fn(5, 3, |x, y| image::Rgba([x as u8 * 10, y as u8 * 10, 7, 255]));
        let empty = RgbaImage::new(0, 0);
        let bytes = write_psd(
            4,
            3,
            &RgbaImage::new(4, 3),
            &[
                ExportLayer { left: -2, top: 1, ..ExportLayer::new("걸친 레이어", &paint, false) },
                ExportLayer { left: 10, top: 0, ..ExportLayer::new("밖", &paint, false) },
                ExportLayer::new("빈 레이어", &empty, false),
            ],
        );
        let document = read_psd(&bytes).unwrap();
        let layer = &document.layers[0];
        assert_eq!((layer.left, layer.top, layer.image.dimensions()), (0, 1, (3, 2)));
        assert_eq!(layer.image.get_pixel(0, 0).0, [20, 0, 7, 255]);
        assert_eq!(layer.image.get_pixel(2, 1).0, [40, 10, 7, 255]);
        assert_eq!(document.layers[1].image.dimensions(), (0, 3));
        assert_eq!(document.layers[2].image.dimensions(), (0, 0));
        let (merged, _) = import_inpaint_layers(&document, 4, 3).unwrap();
        assert_eq!(merged.get_pixel(1, 2).0, [30, 10, 7, 255]);
        assert_eq!(merged.get_pixel(3, 0)[3], 0);
    }

    #[test]
    fn rejects_non_rgb_documents() {
        let mut bytes = write_psd(1, 1, &RgbaImage::new(1, 1), &[]);
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes()); // grayscale mode
        assert!(read_psd(&bytes).unwrap_err().contains("8비트 RGB"));
    }
}
