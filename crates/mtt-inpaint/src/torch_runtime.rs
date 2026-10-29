//! The LibTorch runtime LaMa runs on: koharu's LibTorch builds with their C shim
//! (<https://github.com/koharu-rs/torch>), downloaded once into
//! `<data>/tools/libtorch/<release>-<asset>/`. macOS gets the MPS build, Windows and Linux
//! the CPU one.

use std::io::Read;
use std::path::{Path, PathBuf};

use mtt_core::paths::AppPaths;
use sha2::{Digest, Sha256};

/// The koharu-rs/torch release; its shim must match `mtt-torch`'s bindings.
pub const RELEASE: &str = "v2.13.0.7";

struct Asset {
    name: &'static str,
    sha256: &'static str,
}

/// This platform's archive, where one is published.
const ASSET: Option<Asset> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some(Asset {
        name: "aarch64-apple-darwin-metal",
        sha256: "cc13ec11bd014eb334595e6255ec450f8a3bbc6d8c90ff87fb04769066163754",
    })
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some(Asset {
        name: "x86_64-pc-windows-msvc-cpu",
        sha256: "db6917ea59ef4a3d5eff1d4f4232c364768296f3c98bad50713b93e3594da135",
    })
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some(Asset {
        name: "x86_64-unknown-linux-gnu-cpu",
        sha256: "467c182f60b15c8aae0db4e56e0b23652e6a0ef9d0daed3e10d7e96434ca5f20",
    })
} else {
    None
};

/// Where this platform's runtime goes; `None` where there is no build.
pub fn runtime_dir(paths: &AppPaths) -> Option<PathBuf> {
    ASSET.map(|asset| paths.tools_dir.join("libtorch").join(format!("{RELEASE}-{}", asset.name)))
}

pub fn installed(dir: &Path) -> bool {
    mtt_torch::LIBRARIES.iter().all(|name| dir.join(name).is_file())
}

/// Downloads, checks and unpacks the runtime unless it is already there.
pub fn download(paths: &AppPaths, progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let (Some(asset), Some(dir)) = (ASSET.as_ref(), runtime_dir(paths)) else {
        return Err("이 플랫폼용 LibTorch가 없습니다.".into());
    };
    if installed(&dir) {
        return Ok(());
    }
    let parent = dir.parent().expect("runtime dir has a parent");
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let archive = parent.join(format!("{RELEASE}-{}.tar.gz", asset.name));
    let url = format!("https://github.com/koharu-rs/torch/releases/download/{RELEASE}/{}.tar.gz", asset.name);
    crate::lama::download(&url, &archive, progress)?;
    let result = verify(&archive, asset.sha256).and_then(|()| unpack(&archive, &dir));
    let _ = std::fs::remove_file(&archive);
    result
}

fn verify(archive: &Path, expected: &str) -> Result<(), String> {
    let mut file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("LibTorch 파일이 손상되었습니다 (SHA-256 {actual}). 다시 받아 주세요."))
    }
}

/// The runtime's libraries from `archive`, flat into `dir` (via a `.partial` folder, so a
/// cut-off unpack is never taken for an install).
fn unpack(archive: &Path, dir: &Path) -> Result<(), String> {
    let partial = dir.with_extension("partial");
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(|e| format!("LibTorch 압축을 풀지 못했습니다: {e}"))? {
        let mut entry = entry.map_err(|e| format!("LibTorch 압축을 풀지 못했습니다: {e}"))?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        // Only the listed libraries, by file name: nothing lands outside the folder.
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        if entry.header().entry_type().is_file() && mtt_torch::LIBRARIES.contains(&name) {
            entry.unpack(partial.join(name)).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    if !installed(&partial) {
        let _ = std::fs::remove_dir_all(&partial);
        return Err("LibTorch 압축에 필요한 파일이 없습니다.".into());
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&partial, dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gzipped tar of `files`.
    fn archive(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("runtime.tar.gz");
        let encoder = flate2::write::GzEncoder::new(std::fs::File::create(&path).unwrap(), Default::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    #[test]
    fn unpacks_only_the_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let mut files: Vec<(String, &[u8])> =
            mtt_torch::LIBRARIES.iter().map(|name| (format!("./{name}"), b"lib".as_slice())).collect();
        files.push(("./README.md".into(), b"x"));
        files.push(("./include/torch.h".into(), b"x"));
        let files: Vec<(&str, &[u8])> = files.iter().map(|(name, data)| (name.as_str(), *data)).collect();
        let dir = temp.path().join("runtime");
        unpack(&archive(temp.path(), &files), &dir).unwrap();
        assert!(installed(&dir));
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        let mut expected: Vec<String> = mtt_torch::LIBRARIES.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn incomplete_archives_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("runtime");
        let archive = archive(temp.path(), &[(mtt_torch::LIBRARIES[0], b"lib")]);
        assert!(unpack(&archive, &dir).is_err());
        assert!(!dir.exists() && !dir.with_extension("partial").exists());
    }

    #[test]
    fn checksums_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("data");
        std::fs::write(&file, b"abc").unwrap();
        verify(&file, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap();
        assert!(verify(&file, "00").is_err());
    }
}
