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

/// A channel PackBits-packed row by row: the rows' byte counts and the packed rows.
type Packed = (Vec<u32>, Vec<u8>);

/// One image's channels, PackBits per row (`channels` picks byte offsets within RGBA
/// pixels).
fn packed_channels(image: &RgbaImage, channels: &[usize]) -> Vec<Packed> {
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
                counts.push((data.len() - start) as u32);
            }
            (counts, data)
        })
        .collect()
}

/// Photoshop's largest PSD side; bigger documents are written as PSB (the large document
/// format, up to `MAX_SIDE`).
pub const PSD_MAX_SIDE: u32 = 30_000;

/// The file format's widths: a PSB stores section and channel lengths in 8 bytes instead
/// of 4 and RLE row byte counts in 4 instead of 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Format {
    psb: bool,
}

impl Format {
    /// PSB when a side is over Photoshop's PSD limit, a packed row is too long for a 16-bit
    /// count or the channels are too much for 32-bit lengths (half of it, leaving room for
    /// the records), so nothing is ever cut to fit.
    fn for_document<'a>(width: u32, height: u32, channels: impl Iterator<Item = &'a Packed> + Clone) -> Self {
        let longest_row = channels.clone().flat_map(|(counts, _)| counts).max().copied().unwrap_or(0);
        let bytes: u64 = channels.map(|(counts, data)| (counts.len() * 4 + data.len()) as u64).sum();
        let psb =
            width.max(height) > PSD_MAX_SIDE || longest_row > u32::from(u16::MAX) || bytes > u64::from(u32::MAX / 2);
        Self { psb }
    }

    fn count_size(self) -> usize {
        if self.psb { 4 } else { 2 }
    }

    fn length_size(self) -> usize {
        if self.psb { 8 } else { 4 }
    }

    fn push_length(self, out: &mut Vec<u8>, length: usize) {
        if self.psb {
            out.extend_from_slice(&(length as u64).to_be_bytes());
        } else {
            push_u32(out, u32::try_from(length).expect("`for_document` picks PSB for longer data"));
        }
    }

    fn push_count(self, out: &mut Vec<u8>, count: u32) {
        if self.psb {
            push_u32(out, count);
        } else {
            push_u16(out, u16::try_from(count).expect("`for_document` picks PSB for longer rows"));
        }
    }
}

/// Writes a PSD, or a PSB when Photoshop needs one (see `Format::for_document`; check with
/// [`is_psb`]); `layers` are bottom to top. The composite is `composite` (opaque RGB).
/// Channels are RLE compressed: result and translation layers are mostly transparent.
/// Group records can have an empty (0×0) image.
pub fn write_psd(width: u32, height: u32, composite: &RgbaImage, layers: &[ExportLayer<'_>]) -> Vec<u8> {
    let layer_channels: Vec<Vec<Packed>> =
        layers.iter().map(|layer| packed_channels(layer.image, &[3, 0, 1, 2])).collect();
    let composite_channels = packed_channels(composite, &[0, 1, 2]);
    let format = Format::for_document(width, height, layer_channels.iter().flatten().chain(&composite_channels));
    let mut out = Vec::with_capacity(1024);
    out.extend_from_slice(b"8BPS");
    push_u16(&mut out, if format.psb { 2 } else { 1 });
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
            format.push_length(&mut layer_info, 2 + counts.len() * format.count_size() + data.len());
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
            counts.iter().for_each(|count| format.push_count(&mut layer_info, *count));
            layer_info.extend_from_slice(data);
        }
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }
    format.push_length(&mut out, layer_info.len() + format.length_size() + 4);
    format.push_length(&mut out, layer_info.len());
    out.extend(layer_info);
    push_u32(&mut out, 0); // global layer mask

    // The composite: all row counts first, then all rows.
    push_u16(&mut out, 1);
    for (counts, _) in &composite_channels {
        counts.iter().for_each(|count| format.push_count(&mut out, *count));
    }
    for (_, data) in &composite_channels {
        out.extend_from_slice(data);
    }
    out
}

/// Whether `write_psd` wrote a PSB (save it as `.psb`).
pub fn is_psb(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS\0\x02")
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

    /// A section or channel length (8 bytes in a PSB).
    fn length(&mut self, format: Format) -> Result<usize, String> {
        let bytes = self.take(format.length_size())?;
        let length = bytes.iter().fold(0u64, |length, byte| length << 8 | u64::from(*byte));
        usize::try_from(length).map_err(|_| "PSD 파일이 잘렸습니다.".to_owned())
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
    channels: Vec<(i16, usize)>,
    /// Blend mode key; a group's from its `lsct` when there.
    blend: [u8; 4],
    opacity: u8,
    /// Clipped to the layer below.
    clipping: bool,
    hidden: bool,
    name: String,
    /// `lsct` section type: 1/2 folder, 3 group end marker.
    section: u32,
    /// Carries type tool data (`TySh`).
    text: bool,
    /// The enabled pixel mask.
    mask: Option<MaskInfo>,
    /// Settings the import leaves out, for [`import_warning`].
    notes: Vec<String>,
}

/// A layer's pixel mask as its record describes it.
#[derive(Debug, Clone, Copy)]
struct MaskInfo {
    /// Its channel: -2, or -3 (the pixel and vector masks combined) when there is one.
    channel: i16,
    bounds: Bounds,
    /// The value outside `bounds`.
    default: u8,
    /// Photoshop's mask density: 255 masks fully, 0 not at all.
    density: u8,
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
    /// What [`import_inpaint_layers`] takes as a normal layer instead (blend modes, vector
    /// masks, mask feather, clipping to a group).
    pub notes: Vec<String>,
}

#[derive(Debug)]
pub struct PsdDocument {
    pub width: u32,
    pub height: u32,
    /// Leaf layers bottom to top with layer masks, clipping and group visibility, opacity
    /// and masks folded in.
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

/// The RLE row byte counts in `table` (2 or 4 bytes each).
fn row_counts(table: &[u8], format: Format) -> impl Iterator<Item = u64> + '_ {
    table.chunks_exact(format.count_size()).map(|count| count.iter().fold(0, |sum, byte| sum << 8 | u64::from(*byte)))
}

/// Checks that a channel's `data` holds what a `bounds`-sized layer needs, before anything
/// is allocated for it: raw rows, or RLE row byte counts and the rows they count.
fn check_channel(data: &[u8], bounds: Bounds, format: Format) -> Result<(), String> {
    if data.len() < 2 {
        return Ok(());
    }
    let (width, height) = (bounds.width(), bounds.height());
    let available = data.len() as u64 - 2;
    match u16::from_be_bytes([data[0], data[1]]) {
        0 if width * height <= available => Ok(()),
        1 => {
            let table = height * format.count_size() as u64;
            if table > available {
                return Err(TRUNCATED_LAYER.into());
            }
            let rows: u64 = row_counts(&data[2..2 + table as usize], format).sum();
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
    format: Format,
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
    let counts = &data[2..2 + height * format.count_size()];
    let mut offset = 2 + counts.len();
    for (row, count) in row_counts(counts, format).enumerate().take(skip_y + rows) {
        let count = count as usize;
        let packed = &data[offset..offset + count];
        offset += count;
        if row >= skip_y {
            let pixels = unpack_bits(packed, skip_x + width)?;
            pixels[skip_x..].iter().enumerate().for_each(|(x, value)| put(x, row - skip_y, *value));
        }
    }
    Ok(())
}

/// A layer mask decoded where it's on the canvas.
struct Mask {
    /// Where `values` are; `default` everywhere else.
    area: Bounds,
    values: Vec<u8>,
    default: u8,
}

impl Mask {
    fn value(&self, x: i64, y: i64) -> u8 {
        let Mask { area, .. } = self;
        if x < area.left || y < area.top || x >= area.right || y >= area.bottom {
            return self.default;
        }
        self.values[((y - area.top) * (area.right - area.left) + x - area.left) as usize]
    }
}

/// `a * b` with 255 as 1.
fn multiply(a: u8, b: u8) -> u8 {
    ((u32::from(a) * u32::from(b) + 127) / 255) as u8
}

/// A decoded record: where its pixels are on the canvas, the pixels, and its mask.
type Decoded = (Bounds, RgbaImage, Option<Mask>);

/// The record's color and transparency channels where the layer is on `canvas`, and its
/// mask there (which has its own bounds).
fn decode_layer(
    record: &LayerRecord,
    channels: &[(i16, &[u8])],
    canvas: Bounds,
    format: Format,
) -> Result<Decoded, String> {
    let pixel_channels = channels.iter().filter(|(id, _)| (-1..=2).contains(id));
    let mask =
        record.mask.and_then(|mask| channels.iter().find(|(id, _)| *id == mask.channel).map(|(_, data)| (mask, *data)));
    for (_, data) in pixel_channels.clone() {
        check_channel(data, record.bounds, format)?;
    }
    if let Some((mask, data)) = mask {
        check_channel(data, mask.bounds, format)?;
    }
    let area = record.bounds.intersect(canvas);
    // At most the canvas, which fits u32.
    let width = area.width() as u32;
    let mut image = RgbaImage::from_pixel(width, area.height() as u32, image::Rgba([0, 0, 0, 255]));
    for (id, data) in pixel_channels {
        let channel = if *id == -1 { 3 } else { *id as usize };
        let pixels = image.as_mut();
        decode_channel(data, record.bounds, area, format, |x, y, value| {
            pixels[(y * width as usize + x) * 4 + channel] = value;
        })?;
    }
    let mask = mask
        .map(|(mask, data)| {
            let mask_area = mask.bounds.intersect(canvas);
            let mask_width = mask_area.width() as usize;
            let mut values = vec![0; mask_width * mask_area.height() as usize];
            decode_channel(data, mask.bounds, mask_area, format, |x, y, value| values[y * mask_width + x] = value)?;
            let density = |value: u8| 255 - multiply(mask.density, 255 - value);
            values.iter_mut().for_each(|value| *value = density(*value));
            Ok::<_, String>(Mask { area: mask_area, values, default: density(mask.default) })
        })
        .transpose()?;
    Ok((area, image, mask))
}

/// Hides what `mask` hides of `layer`.
fn apply_mask(layer: &mut PsdLayer, mask: &Mask) {
    let (left, top) = (i64::from(layer.left), i64::from(layer.top));
    for (x, y, pixel) in layer.image.enumerate_pixels_mut() {
        pixel[3] = multiply(pixel[3], mask.value(left + i64::from(x), top + i64::from(y)));
    }
}

/// Paints `layer` (clipped) onto `base`, the layer below it, the way Photoshop composites a
/// clipping group: only where the base has pixels, keeping the base's transparency, so the
/// base's opacity then applies to both. `layer` is left transparent.
fn merge_clipped(base: &mut PsdLayer, layer: &mut PsdLayer) {
    let (dx, dy) = (i64::from(layer.left) - i64::from(base.left), i64::from(layer.top) - i64::from(base.top));
    let (width, height) = (i64::from(base.image.width()), i64::from(base.image.height()));
    for (x, y, pixel) in layer.image.enumerate_pixels_mut() {
        let (bx, by) = (i64::from(x) + dx, i64::from(y) + dy);
        if bx >= 0 && by >= 0 && bx < width && by < height {
            let target = base.image.get_pixel_mut(bx as u32, by as u32);
            let amount = f32::from(pixel[3]) / 255.0 * layer.opacity;
            for channel in 0..3 {
                let mixed = f32::from(pixel[channel]) * amount + f32::from(target[channel]) * (1.0 - amount);
                target[channel] = mixed.round() as u8;
            }
        }
        pixel[3] = 0;
    }
}

/// Keeps `layer` (clipped) to the pixels of `base`, a layer the import leaves out (the
/// background or text), so it can't be merged into it; it also takes the base's opacity
/// and visibility.
fn clip_to(layer: &mut PsdLayer, base: &PsdLayer) {
    let (dx, dy) = (i64::from(layer.left) - i64::from(base.left), i64::from(layer.top) - i64::from(base.top));
    let (width, height) = (i64::from(base.image.width()), i64::from(base.image.height()));
    for (x, y, pixel) in layer.image.enumerate_pixels_mut() {
        let (bx, by) = (i64::from(x) + dx, i64::from(y) + dy);
        let inside = bx >= 0 && by >= 0 && bx < width && by < height;
        pixel[3] = if inside { multiply(pixel[3], base.image.get_pixel(bx as u32, by as u32)[3]) } else { 0 };
    }
    layer.opacity *= base.opacity;
    layer.hidden |= base.hidden;
}

/// Photoshop's (Korean) name of a blend mode key.
fn blend_name(key: &[u8; 4]) -> String {
    let name = match key {
        b"pass" => "통과",
        b"norm" => "표준",
        b"diss" => "디졸브",
        b"dark" => "어둡게 하기",
        b"mul " => "곱하기",
        b"idiv" => "색상 번",
        b"lbrn" => "선형 번",
        b"dkCl" => "어두운 색상",
        b"lite" => "밝게 하기",
        b"scrn" => "스크린",
        b"div " => "색상 닷지",
        b"lddg" => "선형 닷지(추가)",
        b"lgCl" => "밝은 색상",
        b"over" => "오버레이",
        b"sLit" => "소프트 라이트",
        b"hLit" => "하드 라이트",
        b"vLit" => "선명한 라이트",
        b"lLit" => "선형 라이트",
        b"pLit" => "핀 라이트",
        b"hMix" => "하드 혼합",
        b"diff" => "차이",
        b"smud" => "제외",
        b"fsub" => "빼기",
        b"fdiv" => "나누기",
        b"hue " => "색조",
        b"sat " => "채도",
        b"colr" => "색상",
        b"lum " => "광도",
        _ => return String::from_utf8_lossy(key).trim().to_owned(),
    };
    name.to_owned()
}

fn add_note(notes: &mut Vec<String>, note: String) {
    if !notes.contains(&note) {
        notes.push(note);
    }
}

/// The layer mask data of a record (after its length). `has_real` says whether the record
/// has a -3 channel (the pixel and vector masks combined): only then does the data describe
/// it, before the mask parameters (as psd-tools reads Photoshop's files). A disabled mask is
/// `None`.
fn parse_mask(data: &[u8], has_real: bool, notes: &mut Vec<String>) -> Result<Option<MaskInfo>, String> {
    if data.len() < 18 {
        return Ok(None);
    }
    let mut reader = Reader { data, pos: 0 };
    let bounds = Bounds::read(&mut reader)?;
    let default = reader.u8()?;
    let flags = reader.u8()?;
    let mut mask = MaskInfo { channel: -2, bounds, default, density: 255 };
    // Bit 1: disabled.
    let mut enabled = flags & 2 == 0;
    if has_real && data.len() >= 36 {
        let real_flags = reader.u8()?;
        let real_default = reader.u8()?;
        let real_bounds = Bounds::read(&mut reader)?;
        // It counts only with bit 4 set, as psd-tools composites Photoshop's files.
        if real_flags & 16 != 0 {
            mask = MaskInfo { channel: -3, bounds: real_bounds, default: real_default, density: 255 };
            enabled = real_flags & 2 == 0;
        }
    }
    // Bit 4: density and feather follow.
    if flags & 16 != 0 {
        let parameters = reader.u8()?;
        if parameters & 1 != 0 {
            mask.density = reader.u8()?;
        }
        if parameters & 2 != 0 && f64::from_bits(u64::from(reader.u32()?) << 32 | u64::from(reader.u32()?)) > 0.0 {
            add_note(notes, "마스크 페더".into());
        }
    }
    Ok(enabled.then_some(mask))
}

/// Additional layer info whose length takes 8 bytes in a PSB.
const PSB_LONG_KEYS: [&[u8; 4]; 13] = [
    b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2", b"FEid", b"FXid", b"PxSD",
];

fn parse_extra(reader: &mut Reader<'_>, record: &mut LayerRecord, length: usize, format: Format) -> Result<(), String> {
    let mut extra = Reader { data: reader.take(length)?, pos: 0 };
    let mask_len = extra.u32()? as usize;
    let has_real = record.channels.iter().any(|(id, _)| *id == -3);
    record.mask = parse_mask(extra.take(mask_len)?, has_real, &mut record.notes)?;
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
        let long = format.psb && PSB_LONG_KEYS.iter().any(|long| long.as_slice() == key);
        let len = if long { extra.length(format)? } else { extra.u32()? as usize };
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
                if block.len() >= 12 && &block[4..8] == b"8BIM" {
                    record.blend.copy_from_slice(&block[8..12]);
                }
            }
            b"TySh" => record.text = true,
            // The real (-3) mask has the vector mask in it.
            b"vmsk" | b"vsms" if record.mask.is_none_or(|mask| mask.channel != -3) => {
                add_note(&mut record.notes, "벡터 마스크".into());
            }
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
    let format = match reader.u16()? {
        1 => Format { psb: false },
        2 => Format { psb: true },
        version => return Err(format!("지원하지 않는 PSD 버전입니다 ({version}).")),
    };
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
    let layer_mask_len = reader.length(format)?;
    if layer_mask_len == 0 {
        return Ok(PsdDocument { width, height, layers: Vec::new() });
    }
    let layer_info_len = reader.length(format)?;
    if layer_info_len == 0 {
        return Ok(PsdDocument { width, height, layers: Vec::new() });
    }
    let count = reader.i16()?.unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let mut record = LayerRecord { bounds: Bounds::read(&mut reader)?, ..LayerRecord::default() };
        let channel_count = reader.u16()?;
        for _ in 0..channel_count {
            record.channels.push((reader.i16()?, reader.length(format)?));
        }
        reader.take(4)?; // blend mode signature
        record.blend.copy_from_slice(reader.take(4)?);
        record.opacity = reader.u8()?;
        record.clipping = reader.u8()? != 0;
        let flags = reader.u8()?;
        record.hidden = flags & 2 != 0;
        reader.u8()?;
        let extra_len = reader.u32()? as usize;
        parse_extra(&mut reader, &mut record, extra_len, format)?;
        records.push(record);
    }
    let canvas = Bounds { top: 0, left: 0, bottom: i64::from(height), right: i64::from(width) };
    let mut decoded = Vec::with_capacity(count);
    for record in &records {
        // Every channel's bytes must be in the file before any of the layer is allocated.
        let channels = record
            .channels
            .iter()
            .map(|(id, length)| Ok((*id, reader.take(*length)?)))
            .collect::<Result<Vec<_>, String>>()?;
        decoded.push(decode_layer(record, &channels, canvas, format)?);
    }

    // Fold groups (bottom-to-top order: end marker, children, folder record). A clipped
    // layer clips to the closest unclipped one below it in its group.
    let mut stack: Vec<(Vec<PsdLayer>, ClipBase)> = vec![(Vec::new(), ClipBase::None)];
    for (record, (area, image, mask)) in records.into_iter().zip(decoded) {
        match record.section {
            3 => stack.push((Vec::new(), ClipBase::None)),
            1 | 2 => {
                let (children, _) = if stack.len() > 1 { stack.pop().unwrap_or_default() } else { Default::default() };
                let opacity = f32::from(record.opacity) / 255.0;
                let blend = (!matches!(&record.blend, b"pass" | b"norm"))
                    .then(|| format!("그룹 혼합 모드 {}", blend_name(&record.blend)));
                let (parent, base) = stack.last_mut().expect("root frame");
                parent.extend(children.into_iter().map(|mut child| {
                    if let Some(mask) = &mask {
                        apply_mask(&mut child, mask);
                    }
                    child.hidden |= record.hidden;
                    child.opacity *= opacity;
                    child.groups.push(record.name.clone());
                    record.notes.iter().chain(&blend).for_each(|note| add_note(&mut child.notes, note.clone()));
                    child
                }));
                if !record.clipping {
                    *base = ClipBase::Group;
                }
            }
            _ => {
                let (frame, base) = stack.last_mut().expect("root frame");
                let mut layer = PsdLayer {
                    name: record.name,
                    // Inside the canvas, whose sides fit i32.
                    left: area.left as i32,
                    top: area.top as i32,
                    image,
                    opacity: f32::from(record.opacity) / 255.0,
                    hidden: record.hidden,
                    text: record.text,
                    groups: Vec::new(),
                    notes: record.notes,
                };
                if let Some(mask) = &mask {
                    apply_mask(&mut layer, mask);
                }
                if &record.blend != b"norm" {
                    add_note(&mut layer.notes, format!("혼합 모드 {}", blend_name(&record.blend)));
                }
                match (record.clipping, *base) {
                    // Hidden, it adds nothing to its base.
                    (true, ClipBase::Layer(_)) if layer.hidden => {}
                    (true, ClipBase::Layer(index)) => {
                        let base = &mut frame[index];
                        if base.text || base.name.trim() == BACKGROUND_LAYER_NAME {
                            clip_to(&mut layer, base);
                        } else {
                            merge_clipped(base, &mut layer);
                        }
                    }
                    (true, ClipBase::Group) => add_note(&mut layer.notes, "그룹에 건 클리핑 마스크".into()),
                    (true, ClipBase::None) => {}
                    (false, _) => *base = ClipBase::Layer(frame.len()),
                }
                frame.push(layer);
            }
        }
    }
    let layers = stack.into_iter().flat_map(|(layers, _)| layers).collect();
    Ok(PsdDocument { width, height, layers })
}

/// What clipped layers in a group clip to: the last unclipped layer (its index), or a
/// group, which the import doesn't clip to.
#[derive(Debug, Default, Clone, Copy)]
enum ClipBase {
    #[default]
    None,
    Layer(usize),
    Group,
}

/// Layers that make up the inpaint result: visible, not the background, not text.
fn is_paint_layer(layer: &PsdLayer) -> bool {
    !layer.hidden
        && !layer.text
        && layer.name.trim() != BACKGROUND_LAYER_NAME
        && !layer.groups.iter().any(|group| group.trim() == TEXT_GROUP_NAME)
}

/// What `import_inpaint_layers` takes differently from Photoshop, as a status message: the
/// visible layers whose blend modes, vector masks and the like it leaves out (they come in
/// as normal layers). `None` when there are none.
pub fn import_warning(document: &PsdDocument) -> Option<String> {
    const SHOWN: usize = 3;
    let layers: Vec<String> = document
        .layers
        .iter()
        .filter(|layer| is_paint_layer(layer) && !layer.notes.is_empty())
        .map(|layer| format!("{}({})", layer.name.trim(), layer.notes.join(", ")))
        .collect();
    let mut list = layers.iter().take(SHOWN).cloned().collect::<Vec<_>>().join(", ");
    if layers.len() > SHOWN {
        list.push_str(&format!(" 외 {}개", layers.len() - SHOWN));
    }
    (!layers.is_empty())
        .then(|| format!("적용하지 못한 PSD 레이어 설정이 있어 Photoshop과 다르게 보일 수 있습니다: {list}"))
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
        with_layer_info(4, 3, info)
    }

    /// A `width`×`height` PSD with `info` as its layer info (and no composite).
    fn with_layer_info(width: u32, height: u32, info: Vec<u8>) -> Vec<u8> {
        let mut bytes = write_psd(width, height, &RgbaImage::new(width, height), &[]);
        let layer_section = 26 + 4 + 4;
        bytes.truncate(layer_section);
        push_u32(&mut bytes, info.len() as u32 + 8);
        push_u32(&mut bytes, info.len() as u32);
        bytes.extend(info);
        push_u32(&mut bytes, 0);
        bytes
    }

    /// A hand-made layer with raw channels and what Photoshop sets on it.
    struct TestLayer {
        name: &'static str,
        bounds: [i32; 4],
        channels: Vec<(i16, Vec<u8>)>,
        blend: &'static [u8; 4],
        opacity: u8,
        clipping: bool,
        /// Layer mask data, after its length.
        mask: Vec<u8>,
        section: Option<u32>,
        /// More additional layer info blocks.
        extra: Vec<u8>,
    }

    impl TestLayer {
        /// An opaque `color` layer over `bounds` (top, left, bottom, right).
        fn filled(name: &'static str, bounds: [i32; 4], color: [u8; 3]) -> Self {
            let pixels = ((bounds[2] - bounds[0]) * (bounds[3] - bounds[1])) as usize;
            let mut channels: Vec<(i16, Vec<u8>)> =
                color.iter().enumerate().map(|(id, value)| (id as i16, vec![*value; pixels])).collect();
            channels.push((-1, vec![255; pixels]));
            let (blend, mask, extra) = (b"norm", Vec::new(), Vec::new());
            Self { name, bounds, channels, blend, opacity: 255, clipping: false, mask, section: None, extra }
        }

        /// A group record (`section` 1) or the end of one (3).
        fn group(name: &'static str, section: u32) -> Self {
            let mut group = Self::filled(name, [0; 4], [0; 3]);
            group.blend = b"pass";
            group.section = Some(section);
            group
        }

        /// With a -2 mask over `bounds` holding `values`, `default` outside them.
        fn masked(mut self, bounds: [i32; 4], values: Vec<u8>, default: u8, flags: u8) -> Self {
            self.mask = mask_data(bounds, default, flags);
            self.channels.push((-2, values));
            self
        }
    }

    /// Layer mask data without a real (-3) mask: bounds, default and flags, padded to 20.
    fn mask_data(bounds: [i32; 4], default: u8, flags: u8) -> Vec<u8> {
        let mut data = Vec::new();
        bounds.iter().for_each(|value| push_i32(&mut data, *value));
        data.extend_from_slice(&[default, flags, 0, 0]);
        data
    }

    fn layered(width: u32, height: u32, layers: &[TestLayer]) -> Vec<u8> {
        let mut info = Vec::new();
        push_u16(&mut info, layers.len() as u16);
        for layer in layers {
            layer.bounds.iter().for_each(|value| push_i32(&mut info, *value));
            push_u16(&mut info, layer.channels.len() as u16);
            for (id, plane) in &layer.channels {
                info.extend_from_slice(&id.to_be_bytes());
                push_u32(&mut info, plane.len() as u32 + 2);
            }
            info.extend_from_slice(b"8BIM");
            info.extend_from_slice(layer.blend);
            info.extend_from_slice(&[layer.opacity, u8::from(layer.clipping), 0, 0]);
            let mut extra = Vec::new();
            push_u32(&mut extra, layer.mask.len() as u32);
            extra.extend_from_slice(&layer.mask);
            push_u32(&mut extra, 0);
            extra.extend(pascal_name(layer.name));
            extra.extend(unicode_name_block(layer.name));
            if let Some(section) = layer.section {
                extra.extend(layer_info_block(b"lsct", section.to_be_bytes().to_vec()));
            }
            extra.extend_from_slice(&layer.extra);
            push_u32(&mut info, extra.len() as u32);
            info.extend(extra);
        }
        for (_, plane) in layers.iter().flat_map(|layer| &layer.channels) {
            info.extend_from_slice(&[0, 0]);
            info.extend_from_slice(plane);
        }
        with_layer_info(width, height, info)
    }

    #[test]
    fn layer_masks_hide_what_they_hide_in_photoshop() {
        let real = {
            // A -2 mask that shows the pixel and a real (-3) one that hides it.
            let mut layer = TestLayer::filled("실제 마스크", [0, 2, 1, 3], [0, 0, 255]);
            layer.channels.extend([(-2, vec![255]), (-3, vec![0])]);
            layer.mask = mask_data([0, 2, 1, 3], 255, 0);
            layer.mask.truncate(18);
            layer.mask.extend_from_slice(&[16, 255]);
            [0, 2, 1, 3].iter().for_each(|value| push_i32(&mut layer.mask, *value));
            layer
        };
        let mut density = TestLayer::filled("밀도", [1, 2, 2, 3], [255, 255, 0]);
        density.channels.push((-2, vec![0]));
        density.mask = mask_data([1, 2, 2, 3], 0, 16);
        density.mask[18..20].copy_from_slice(&[1, 128]);
        let bytes = layered(
            4,
            2,
            &[
                TestLayer::filled(BACKGROUND_LAYER_NAME, [0, 0, 2, 4], [255; 3]),
                // Shown at x = 0 by the mask, hidden at x = 1 by it and past it by its default.
                TestLayer::filled(RESULT_LAYER_NAME, [0, 0, 2, 4], [255, 0, 0]).masked(
                    [0, 0, 2, 2],
                    vec![255, 0, 255, 0],
                    0,
                    0,
                ),
                TestLayer::filled("꺼진 마스크", [1, 3, 2, 4], [0, 255, 0]).masked([1, 3, 2, 4], vec![0], 0, 2),
                real,
                density,
            ],
        );
        let document = read_psd(&bytes).unwrap();
        let (merged, mask) = import_inpaint_layers(&document, 4, 2).unwrap();
        let alpha: Vec<u8> = merged.pixels().map(|pixel| pixel[3]).collect();
        assert_eq!(alpha, [255, 0, 0, 0, 255, 0, 127, 255]);
        assert_eq!(merged.get_pixel(0, 1).0, [255, 0, 0, 255]);
        assert_eq!(merged.get_pixel(3, 1).0, [0, 255, 0, 255]);
        assert_eq!(mask.as_raw(), &[255, 0, 0, 0, 255, 0, 255, 255]);
        assert_eq!(import_warning(&document), None);
    }

    #[test]
    fn clipped_layers_stay_on_their_base_and_left_out_settings_are_reported() {
        let mut base = TestLayer::filled("기준", [0, 0, 1, 2], [255, 0, 0]);
        base.opacity = 204;
        let clipped = TestLayer { clipping: true, ..TestLayer::filled("클리핑", [0, 0, 1, 4], [0, 0, 255]) };
        let group =
            TestLayer { blend: b"mul ", ..TestLayer::group("그룹", 1) }.masked([0, 2, 1, 4], vec![0, 255], 255, 0);
        let mut vector = TestLayer::filled("벡터", [0, 1, 1, 2], [0; 3]);
        vector.channels[3].1 = vec![0];
        vector.extra = layer_info_block(b"vmsk", vec![0; 8]);
        let bytes = layered(
            4,
            1,
            &[
                base,
                clipped,
                TestLayer::group(GROUP_END_NAME, 3),
                TestLayer::filled("그룹 안", [0, 2, 1, 4], [0, 255, 0]),
                group,
                TestLayer { clipping: true, ..TestLayer::filled("그룹 위 클리핑", [0, 3, 1, 4], [255; 3]) },
                TestLayer { blend: b"mul ", ..TestLayer::filled("곱하기", [0, 0, 1, 1], [255, 255, 0]) },
                vector,
            ],
        );
        let document = read_psd(&bytes).unwrap();
        // The clipped layer is painted onto its base, inside the base's pixels.
        assert_eq!(document.layers[0].image.as_raw(), &[0, 0, 255, 255, 0, 0, 255, 255]);
        assert!(document.layers[1].image.pixels().all(|pixel| pixel[3] == 0));
        let (merged, _) = import_inpaint_layers(&document, 4, 1).unwrap();
        assert_eq!(merged.get_pixel(0, 0).0, [255, 255, 0, 255]);
        // As Photoshop (psd-tools) composites it: the base's opacity over the clipping group.
        assert_eq!(merged.get_pixel(1, 0).0, [0, 0, 255, 204]);
        assert_eq!(merged.get_pixel(2, 0)[3], 0, "the group's mask hides it");
        assert_eq!(merged.get_pixel(3, 0).0, [255, 255, 255, 255], "not clipped to the group");
        let warning = import_warning(&document).unwrap();
        for part in [
            "그룹 안(그룹 혼합 모드 곱하기)",
            "그룹 위 클리핑(그룹에 건 클리핑 마스크)",
            "곱하기(혼합 모드 곱하기)",
            "외 1개",
        ] {
            assert!(warning.contains(part), "{part}: {warning}");
        }
        assert!(document.layers.iter().any(|layer| layer.name == "벡터" && layer.notes == ["벡터 마스크"]));
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
    fn pages_over_30000_px_are_written_and_read_as_psb() {
        let (width, height) = (2, PSD_MAX_SIDE + 1);
        let source = RgbaImage::from_fn(width, height, |x, y| image::Rgba([x as u8, (y % 251) as u8, 3, 255]));
        let mut result = RgbaImage::new(width, height);
        result.put_pixel(1, height - 1, image::Rgba([200, 100, 50, 255]));
        let layers = [
            ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
            ExportLayer::new(RESULT_LAYER_NAME, &result, false),
        ];
        let bytes = write_psd(width, height, &source, &layers);
        assert!(is_psb(&bytes));
        let document = read_psd(&bytes).unwrap();
        assert_eq!((document.width, document.height), (width, height));
        assert_eq!(document.layers[0].image, source);
        let (merged, mask) = import_inpaint_layers(&document, width, height).unwrap();
        assert_eq!(merged.get_pixel(1, height - 1).0, [200, 100, 50, 255]);
        assert_eq!(mask.pixels().filter(|p| p[0] > 0).count(), 1);
        // Up to Photoshop's limit it stays a PSD.
        let tall = RgbaImage::new(1, PSD_MAX_SIDE);
        assert!(!is_psb(&write_psd(1, PSD_MAX_SIDE, &tall, &[ExportLayer::new(RESULT_LAYER_NAME, &tall, false)])));
    }

    #[test]
    fn rows_too_long_for_16_bit_counts_switch_to_psb() {
        // 70000 bytes of noise pack to more than 65535, which a PSD row count can't hold
        // (they used to be cut to 16 bits).
        let mut state = 1u32;
        let noise = RgbaImage::from_fn(70_000, 1, |_, _| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let value = (state >> 16) as u8;
            image::Rgba([value, value ^ 0x55, value ^ 0xAA, 255])
        });
        let bytes = write_psd(4, 3, &RgbaImage::new(4, 3), &[ExportLayer::new("넓은 레이어", &noise, false)]);
        assert!(is_psb(&bytes));
        let layer = &read_psd(&bytes).unwrap().layers[0];
        assert_eq!(layer.image.dimensions(), (4, 1));
        assert_eq!(layer.image.as_raw()[..], noise.as_raw()[..16]);
    }

    #[test]
    fn rejects_non_rgb_documents() {
        let mut bytes = write_psd(1, 1, &RgbaImage::new(1, 1), &[]);
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes()); // grayscale mode
        assert!(read_psd(&bytes).unwrap_err().contains("8비트 RGB"));
    }
}
