//! Synthetic zips for organise tests (no real ROM data).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

/// Writes a zip at `path` with `(name, bytes, deflate)` entries.
pub fn zip_file(path: &Path, entries: &[(&str, &[u8], bool)]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes, deflate) in entries {
        let method = if *deflate {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// Writes a zip at `path` with one ZipCrypto-encrypted entry.
pub fn zip_encrypted(path: &Path, name: &str, bytes: &[u8]) {
    use zip::unstable::write::FileOptionsExt;
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .with_deprecated_encryption(b"secret")
        .unwrap();
    zip.start_file(name, options).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}
