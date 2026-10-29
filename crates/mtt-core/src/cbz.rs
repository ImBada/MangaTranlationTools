//! Comic book archives (`.cbz`) of rendered pages.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

/// A CBZ of `pages` (PNG bytes in reading order), named `0001.png`, `0002.png`, … so every
/// reader keeps the order. PNGs are stored as they are; compressing them again gains nothing.
pub fn cbz_bytes(pages: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let width = pages.len().to_string().len().max(4);
    for (index, page) in pages.iter().enumerate() {
        writer.start_file(format!("{:0width$}.png", index + 1), options).map_err(|error| error.to_string())?;
        writer.write_all(page).map_err(|error| error.to_string())?;
    }
    writer.finish().map(Cursor::into_inner).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn pages_are_numbered_in_order() {
        let bytes = cbz_bytes(&[b"one".to_vec(), b"two".to_vec()]).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> =
            (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned()).collect();
        assert_eq!(names, ["0001.png", "0002.png"]);
        let mut second = String::new();
        archive.by_name("0002.png").unwrap().read_to_string(&mut second).unwrap();
        assert_eq!(second, "two");
    }
}
