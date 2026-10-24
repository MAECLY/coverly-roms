//! Zip backend of the archive reader (spec §4, §5.1, S2, S3). Fixtures are
//! synthetic.

use std::io::{Cursor, Read};

use coverly_core::archive::testing::zip_raw_entry;

/// The `zip` crate is built without its `lzma`, `xz`, `ppmd` and `zstd`
/// features, so it structurally cannot decode those methods: `archive` reads
/// them raw and decodes them with capped decoders (S2, S3). If this test
/// starts failing, a crate in the graph turned one of those features on
/// (`cargo tree -e features -i zip`, `scripts/check-zip-features.sh`).
#[test]
fn zip_crate_cannot_decode_lzma() {
    let payload = b"not really lzma, never decoded".to_vec();
    let bytes = zip_raw_entry(14, &payload);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid central directory");
    assert_eq!(archive.len(), 1);

    match archive.by_index(0) {
        Ok(_) => panic!("zip crate decodes LZMA: a forbidden zip feature is enabled"),
        Err(err) => {
            let text = err.to_string().to_ascii_lowercase();
            assert!(
                text.contains("compression")
                    && (text.contains("unsupported") || text.contains("not supported")),
                "unexpected error: {err}"
            );
        }
    }

    let mut raw = archive
        .by_index_raw(0)
        .expect("raw read of a method-14 entry");
    let mut out = Vec::new();
    raw.read_to_end(&mut out).unwrap();
    assert_eq!(out, payload);
}
