//! Minimal PSD support for the inpaint round trip (`inpaintPsd.ts`).
//!
//! Export writes an 8-bit RGB document with the source as a locked-name background layer,
//! the masked inpaint result, and an optional hidden translation layer. Import flattens
//! every visible non-background layer (groups, opacity, RLE or raw channels) into a new
//! result layer and a mask covering its painted pixels.

use image::{GrayImage, RgbaImage};

pub const BACKGROUND_LAYER_NAME: &str = "배경 (이름변경금지)";
pub const RESULT_LAYER_NAME: &str = "Inpaint Result";
pub const TRANSLATION_LAYER_NAME: &str = "번역 블록";

pub struct ExportLayer<'a> {
    pub name: &'a str,
    pub image: &'a RgbaImage,
    pub hidden: bool,
    /// `lsct` section type for group markers (tests); `None` for normal layers.
    pub section: Option<u32>,
}

impl<'a> ExportLayer<'a> {
    pub fn new(name: &'a str, image: &'a RgbaImage, hidden: bool) -> Self {
        Self { name, image, hidden, section: None }
    }
}

// ---------------------------------------------------------------------------------- write

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
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

/// `luni` additional layer info with the UTF-16 name.
fn unicode_name_block(name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut data = Vec::new();
    push_u32(&mut data, units.len() as u32);
    for unit in units {
        push_u16(&mut data, unit);
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let mut out = b"8BIMluni".to_vec();
    push_u32(&mut out, data.len() as u32);
    out.extend(data);
    out
}

/// Writes a PSD; `layers` are bottom to top. The composite is `composite` (opaque RGB).
pub fn write_psd(width: u32, height: u32, composite: &RgbaImage, layers: &[ExportLayer<'_>]) -> Vec<u8> {
    let pixels = (width * height) as usize;
    let mut out = Vec::with_capacity(pixels * 4 * (layers.len() + 1) + 1024);
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
    for layer in layers {
        push_i32(&mut layer_info, 0);
        push_i32(&mut layer_info, 0);
        push_i32(&mut layer_info, height as i32);
        push_i32(&mut layer_info, width as i32);
        push_u16(&mut layer_info, 4);
        // Transparency first, like Photoshop.
        for id in [-1i16, 0, 1, 2] {
            layer_info.extend_from_slice(&id.to_be_bytes());
            push_u32(&mut layer_info, 2 + pixels as u32);
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
        push_u32(&mut layer_info, extra.len() as u32);
        layer_info.extend(extra);
    }
    for layer in layers {
        for channel in [3usize, 0, 1, 2] {
            push_u16(&mut layer_info, 0);
            layer_info.extend(layer.image.as_raw().iter().skip(channel).step_by(4));
        }
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }
    push_u32(&mut out, layer_info.len() as u32 + 4 + 4);
    push_u32(&mut out, layer_info.len() as u32);
    out.extend(layer_info);
    push_u32(&mut out, 0); // global layer mask

    push_u16(&mut out, 0);
    for channel in 0..3 {
        out.extend(composite.as_raw().iter().skip(channel).step_by(4));
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

#[derive(Debug, Default, Clone)]
struct LayerRecord {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    channels: Vec<(i16, u32)>,
    opacity: u8,
    hidden: bool,
    name: String,
    /// `lsct` section type: 1/2 folder, 3 group end marker.
    section: u32,
}

#[derive(Debug, Clone)]
pub struct PsdLayer {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub image: RgbaImage,
    pub opacity: f32,
    pub hidden: bool,
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

fn read_channel(reader: &mut Reader<'_>, width: usize, height: usize, length: u32) -> Result<Vec<u8>, String> {
    let start = reader.pos;
    if length < 2 {
        reader.take(length as usize)?;
        return Ok(vec![0; width * height]);
    }
    let compression = reader.u16()?;
    let data = match compression {
        0 => reader.take(width * height)?.to_vec(),
        1 => {
            let counts: Vec<usize> = (0..height).map(|_| reader.u16().map(usize::from)).collect::<Result<_, _>>()?;
            let mut pixels = Vec::with_capacity(width * height);
            for count in counts {
                pixels.extend(unpack_bits(reader.take(count)?, width)?);
            }
            pixels
        }
        other => return Err(format!("지원하지 않는 PSD 압축 방식입니다 ({other}). 압축 없이 또는 RLE로 저장하세요.")),
    };
    reader.pos = start + length as usize;
    Ok(data)
}

fn parse_extra(reader: &mut Reader<'_>, record: &mut LayerRecord, length: usize) -> Result<(), String> {
    let end = reader.pos + length;
    let mask_len = reader.u32()? as usize;
    reader.take(mask_len)?;
    let ranges_len = reader.u32()? as usize;
    reader.take(ranges_len)?;
    let name_len = reader.u8()? as usize;
    let name = reader.take(name_len)?;
    record.name = String::from_utf8_lossy(name).into_owned();
    let padded = (1 + name_len).div_ceil(4) * 4;
    reader.take(padded - 1 - name_len)?;
    while reader.pos + 12 <= end {
        let signature = reader.take(4)?;
        if signature != b"8BIM" && signature != b"8B64" {
            break;
        }
        let key = reader.take(4)?.to_vec();
        let len = reader.u32()? as usize;
        let block = reader.take(len)?;
        match key.as_slice() {
            b"luni" if block.len() >= 4 => {
                let count = u32::from_be_bytes([block[0], block[1], block[2], block[3]]) as usize;
                let units: Vec<u16> =
                    block[4..].chunks_exact(2).take(count).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                record.name = String::from_utf16_lossy(&units);
            }
            b"lsct" | b"lsdk" if block.len() >= 4 => {
                record.section = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
            }
            _ => {}
        }
    }
    reader.pos = end;
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
        let mut record = LayerRecord {
            top: reader.i32()?,
            left: reader.i32()?,
            bottom: reader.i32()?,
            right: reader.i32()?,
            ..LayerRecord::default()
        };
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
    let mut images = Vec::with_capacity(count);
    for record in &records {
        let (w, h) = ((record.right - record.left).max(0) as usize, (record.bottom - record.top).max(0) as usize);
        let mut image = RgbaImage::from_pixel(w as u32, h as u32, image::Rgba([0, 0, 0, 255]));
        let mut has_alpha = false;
        for (id, length) in &record.channels {
            let channel = read_channel(&mut reader, w, h, *length)?;
            let index = match id {
                0 => 0,
                1 => 1,
                2 => 2,
                -1 => {
                    has_alpha = true;
                    3
                }
                _ => continue, // layer masks are ignored
            };
            for (pixel, value) in image.pixels_mut().zip(channel) {
                pixel[index] = value;
            }
        }
        if !has_alpha {
            image.pixels_mut().for_each(|p| p[3] = 255);
        }
        images.push(image);
    }

    // Fold groups (bottom-to-top order: end marker, children, folder record).
    let mut stack: Vec<Vec<PsdLayer>> = vec![Vec::new()];
    for (record, image) in records.into_iter().zip(images) {
        match record.section {
            3 => stack.push(Vec::new()),
            1 | 2 => {
                let children = if stack.len() > 1 { stack.pop().unwrap_or_default() } else { Vec::new() };
                let opacity = f32::from(record.opacity) / 255.0;
                let parent = stack.last_mut().expect("root frame");
                parent.extend(children.into_iter().map(|mut child| {
                    child.hidden |= record.hidden;
                    child.opacity *= opacity;
                    child
                }));
            }
            _ => stack.last_mut().expect("root frame").push(PsdLayer {
                name: record.name,
                left: record.left,
                top: record.top,
                image,
                opacity: f32::from(record.opacity) / 255.0,
                hidden: record.hidden,
            }),
        }
    }
    let layers = stack.into_iter().flatten().collect();
    Ok(PsdDocument { width, height, layers })
}

/// `importInpaintPsd`: merges visible non-background layers into (result, mask).
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
    let paint: Vec<&PsdLayer> =
        document.layers.iter().filter(|layer| !layer.hidden && layer.name.trim() != BACKGROUND_LAYER_NAME).collect();
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

    #[test]
    fn rejects_non_rgb_documents() {
        let mut bytes = write_psd(1, 1, &RgbaImage::new(1, 1), &[]);
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes()); // grayscale mode
        assert!(read_psd(&bytes).unwrap_err().contains("8비트 RGB"));
    }
}
