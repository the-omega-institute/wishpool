//! Unpack an upload into `path → bytes`, refusing archives that are too
//! large, too many files, or paths that leave the archive.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

use crate::{LatexError, LatexResult};

/// Limits on what an upload may expand to.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

pub type Files = BTreeMap<String, Vec<u8>>;

fn clean_path(raw: &str) -> Option<String> {
    let raw = raw.replace('\\', "/");
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p => parts.push(p),
        }
    }
    if parts.is_empty() || raw.starts_with('/') || parts.iter().any(|p| p.starts_with("__MACOSX")) {
        return None;
    }
    Some(parts.join("/"))
}

/// A POSIX tar has `ustar` at offset 257 of its first header.
fn is_tar(data: &[u8]) -> bool {
    data.get(257..262) == Some(b"ustar")
}

/// The name of a lone source file: the upload's name without `.gz`, or
/// `main.tex` when that is not a `.tex` name (arXiv names e-prints by id).
fn single_name(upload_name: &str) -> String {
    let name = upload_name.trim_end_matches(".gz");
    match clean_path(name) {
        Some(n) if n.to_ascii_lowercase().ends_with(".tex") => n,
        _ => "main.tex".into(),
    }
}

/// Unpack `bytes`: a zip, a tar (optionally gzipped), or a single file
/// (optionally gzipped).
pub fn unpack(bytes: &[u8], upload_name: &str, limits: Limits) -> LatexResult<Files> {
    let mut files = Files::new();
    let mut total: u64 = 0;
    let mut add = |files: &mut Files, path: String, data: Vec<u8>| -> LatexResult<()> {
        total += data.len() as u64;
        if total > limits.max_total_bytes {
            return Err(LatexError::Archive(format!(
                "expands beyond {} bytes",
                limits.max_total_bytes
            )));
        }
        if files.len() >= limits.max_files {
            return Err(LatexError::Archive(format!(
                "more than {} files",
                limits.max_files
            )));
        }
        files.insert(path, data);
        Ok(())
    };
    if bytes.starts_with(b"PK\x03\x04") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| LatexError::Archive(e.to_string()))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| LatexError::Archive(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let Some(path) = clean_path(entry.name()) else {
                continue;
            };
            let mut data = Vec::new();
            (&mut entry)
                .take(limits.max_total_bytes + 1)
                .read_to_end(&mut data)
                .map_err(|e| LatexError::Archive(e.to_string()))?;
            add(&mut files, path, data)?;
        }
    } else {
        // arXiv serves a single-file source as a bare gzip of the `.tex`, and
        // a multi-file source as a gzipped tar; plain tars are accepted too.
        let inflated;
        let data: &[u8] = if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(bytes)
                .take(limits.max_total_bytes + 1)
                .read_to_end(&mut out)
                .map_err(|e| LatexError::Archive(e.to_string()))?;
            if out.len() as u64 > limits.max_total_bytes {
                return Err(LatexError::Archive(format!(
                    "expands beyond {} bytes",
                    limits.max_total_bytes
                )));
            }
            inflated = out;
            &inflated
        } else {
            bytes
        };
        if is_tar(data) {
            let mut archive = tar::Archive::new(Cursor::new(data));
            for entry in archive
                .entries()
                .map_err(|e| LatexError::Archive(e.to_string()))?
            {
                let mut entry = entry.map_err(|e| LatexError::Archive(e.to_string()))?;
                if !entry.header().entry_type().is_file() {
                    continue;
                }
                let raw = entry
                    .path()
                    .map_err(|e| LatexError::Archive(e.to_string()))?
                    .to_string_lossy()
                    .into_owned();
                let Some(path) = clean_path(&raw) else {
                    continue;
                };
                let mut data = Vec::new();
                (&mut entry)
                    .take(limits.max_total_bytes + 1)
                    .read_to_end(&mut data)
                    .map_err(|e| LatexError::Archive(e.to_string()))?;
                add(&mut files, path, data)?;
            }
        } else {
            add(&mut files, single_name(upload_name), data.to_vec())?;
        }
    }
    if files.is_empty() {
        return Err(LatexError::Archive("the upload contains no files".into()));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn single_file_and_zip() {
        let files = unpack(b"\\documentclass{article}", "paper.tex", Limits::default()).unwrap();
        assert!(files.contains_key("paper.tex"));

        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("src/main.tex", options).unwrap();
            writer.write_all(b"hello").unwrap();
            writer.start_file("../evil.tex", options).unwrap();
            writer.write_all(b"no").unwrap();
            writer.start_file("__MACOSX/._main.tex", options).unwrap();
            writer.write_all(b"junk").unwrap();
            writer.finish().unwrap();
        }
        let files = unpack(buffer.get_ref(), "upload.zip", Limits::default()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["src/main.tex"]);
    }

    #[test]
    fn bare_gzip_and_plain_tar() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"\\documentclass{article}").unwrap();
        let gz = gz.finish().unwrap();
        let files = unpack(&gz, "2609.33421", Limits::default()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["main.tex"]);
        let files = unpack(&gz, "paper.tex.gz", Limits::default()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["paper.tex"]);

        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_ustar();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "a/main.tex", &b"hello"[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let files = unpack(&tar_bytes, "src.tar", Limits::default()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["a/main.tex"]);
    }

    #[test]
    fn tar_gz_and_limits() {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            for (name, body) in [("main.tex", b"x".repeat(10)), ("fig.tex", b"y".repeat(10))] {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, body.as_slice())
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar_bytes).unwrap();
        let gz = gz.finish().unwrap();
        assert_eq!(
            unpack(&gz, "src.tar.gz", Limits::default()).unwrap().len(),
            2
        );
        let tight = Limits {
            max_files: 1,
            max_total_bytes: 1_000,
        };
        assert!(unpack(&gz, "src.tar.gz", tight).is_err());
        let tiny = Limits {
            max_files: 10,
            max_total_bytes: 15,
        };
        assert!(unpack(&gz, "src.tar.gz", tiny).is_err());
    }
}
