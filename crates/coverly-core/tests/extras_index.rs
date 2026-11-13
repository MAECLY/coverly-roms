//! Reading and writing `Extras/extras.json` (extras spec D2, D3, D5).

use coverly_core::extras::index::{
    read_index, serialize, IndexRead, UnreadableKind, MAX_INDEX_BYTES,
};
use coverly_core::extras::ExtrasFile;

const OK: &str = include_str!("fixtures/extras/ok.json");

fn extras_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn write_index(d: &tempfile::TempDir, bytes: impl AsRef<[u8]>) {
    std::fs::write(d.path().join("extras.json"), bytes).unwrap();
}

#[test]
fn absent_when_there_is_no_file() {
    let d = extras_dir();
    assert!(matches!(read_index(d.path()), IndexRead::Absent));
}

#[test]
fn serialize_is_pretty_sorted_and_round_trips_byte_for_byte() {
    // Written by serialize(): sorted keys, 2-space indent, trailing newline.
    let d = extras_dir();
    write_index(&d, OK);
    let IndexRead::Ok(f) = read_index(d.path()) else {
        panic!("ok.json must read as Ok")
    };
    assert_eq!(String::from_utf8(serialize(&f)).unwrap(), OK);
}

#[test]
fn serialize_sorts_records_by_path_and_keys_alphabetically() {
    let compact: serde_json::Value = serde_json::from_str(OK).unwrap();
    let mut f: ExtrasFile = serde_json::from_value(compact).unwrap();
    f.files.reverse();
    let text = String::from_utf8(serialize(&f)).unwrap();
    assert_eq!(text, OK);
    let first_keys: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("  \"") && l.contains(':'))
        .map(|l| l.trim().split('"').nth(1).unwrap())
        .collect();
    let mut sorted = first_keys.clone();
    sorted.sort();
    assert_eq!(first_keys, sorted);
    assert!(text.ends_with("}\n"));
}

#[test]
fn records_sort_by_nfc_path() {
    let mut f = ExtrasFile::empty();
    let rec = |path: &str| -> coverly_core::extras::ExtraRecord {
        serde_json::from_value(serde_json::json!({
            "path": path, "type": "audio", "provenance": "user",
            "origin": {"how": "found"}, "tags": [], "size": 1, "mtime_ns": 0,
            "added_at": "2026-11-13T00:00:00Z"
        }))
        .unwrap()
    };
    // NFD «Cancio\u{301}n» sorts as NFC «Canción», before «Cantar».
    f.files = vec![
        rec("Soundtrack/Cantar.flac"),
        rec("Soundtrack/Cancio\u{301}n.flac"),
    ];
    let v: serde_json::Value = serde_json::from_slice(&serialize(&f)).unwrap();
    // The on-disk spelling is kept.
    assert_eq!(v["files"][0]["path"], "Soundtrack/Cancio\u{301}n.flac");
    assert_eq!(v["files"][1]["path"], "Soundtrack/Cantar.flac");
}

#[test]
fn malformed_json_is_reported_and_salvaged() {
    let d = extras_dir();
    write_index(
        &d,
        r#"{"version":1,"files":[{"path":"Soundtrack/a.flac","type":"audio","provenance":"user","origin":{"how":"added"},"tags":["x"],"size":1,"mtime_ns":0,"added_at":"t"},{"nope":1},{"path":"../evil","type":"audio","provenance":"user","origin":{"how":"added"},"size":1,"mtime_ns":0,"added_at":"t"}]}"#,
    );
    let IndexRead::Malformed {
        salvaged,
        bad_records,
        unreadable_kind,
    } = read_index(d.path())
    else {
        panic!("expected Malformed")
    };
    let salvaged = salvaged.unwrap();
    assert_eq!(salvaged.files.len(), 1);
    assert_eq!(salvaged.files[0].tags, ["x"]);
    assert_eq!(bad_records, 2); // shape error + a path that is not plain components
    assert_eq!(unreadable_kind, None);
}

#[test]
fn record_paths_must_be_plain_relative_components() {
    for bad in [
        "",
        "/abs.flac",
        "Soundtrack//a.flac",
        "Soundtrack/./a.flac",
        "Soundtrack/../a.flac",
        "..",
        "Soundtrack\\a.flac",
        "C:/a.flac",
        "c:a.flac",
        "Soundtrack/",
        "a\u{0}b",
    ] {
        let d = extras_dir();
        let text = serde_json::json!({
            "version": 1,
            "files": [{
                "path": bad, "type": "audio", "provenance": "user",
                "origin": {"how": "found"}, "tags": [], "size": 1, "mtime_ns": 0,
                "added_at": "t"
            }]
        });
        write_index(&d, text.to_string());
        let IndexRead::Malformed {
            salvaged,
            bad_records,
            ..
        } = read_index(d.path())
        else {
            panic!("{bad:?} must be refused")
        };
        assert_eq!(bad_records, 1, "{bad:?}");
        assert!(salvaged.unwrap().files.is_empty(), "{bad:?}");
    }
}

#[test]
fn a_bad_top_level_shape_salvages_the_good_records() {
    let d = extras_dir();
    write_index(
        &d,
        r#"{"version":"one","categories":{"Manuals":{"sync":false}},"files":[{"path":"Manuals/m.pdf","type":"document","provenance":"official","origin":{"how":"organize"},"tags":[],"size":1,"mtime_ns":0,"added_at":"t"}]}"#,
    );
    let IndexRead::Malformed {
        salvaged,
        bad_records,
        ..
    } = read_index(d.path())
    else {
        panic!("expected Malformed")
    };
    let s = salvaged.unwrap();
    assert_eq!(bad_records, 0);
    assert_eq!(s.version, ExtrasFile::VERSION);
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.sync_for("Manuals"), Some(false));
}

#[test]
fn an_object_without_a_files_array_is_not_salvaged() {
    let d = extras_dir();
    write_index(&d, r#"{"version":1,"files":{"a":1}}"#);
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed { salvaged: None, .. }
    ));
    write_index(&d, r#"[1,2,3]"#);
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed { salvaged: None, .. }
    ));
}

#[test]
fn version_zero_or_missing_is_malformed() {
    let d = extras_dir();
    for text in [r#"{"version":0,"files":[]}"#, r#"{"files":[]}"#] {
        write_index(&d, text);
        assert!(
            matches!(read_index(d.path()), IndexRead::Malformed { .. }),
            "{text}"
        );
    }
}

#[test]
fn a_minimal_file_reads_as_ok() {
    let d = extras_dir();
    write_index(&d, r#"{"version":1}"#);
    let IndexRead::Ok(f) = read_index(d.path()) else {
        panic!("expected Ok")
    };
    assert_eq!(f, ExtrasFile::empty());
}

#[test]
fn a_utf8_bom_is_tolerated() {
    let d = extras_dir();
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(OK.as_bytes());
    write_index(&d, bytes);
    assert!(matches!(read_index(d.path()), IndexRead::Ok(_)));
}

#[test]
fn not_json_is_malformed_without_salvage() {
    let d = extras_dir();
    write_index(&d, b"\xff\xfe garbage");
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed {
            salvaged: None,
            unreadable_kind: None,
            ..
        }
    ));
}

#[test]
fn newer_version_is_view_only_with_what_is_understood() {
    let d = extras_dir();
    write_index(&d, r#"{"version":2,"files":[]}"#);
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Newer { version: 2, .. }
    ));
    write_index(
        &d,
        r#"{"version":7,"files":[{"path":"Videos/t.mp4","type":"video","provenance":"user","origin":{"how":"added"},"tags":["tráiler"],"size":3,"mtime_ns":0,"added_at":"t"},{"path":"Videos/x.glb","type":"model-3d","provenance":"user","origin":{"how":"added"},"size":3,"mtime_ns":0,"added_at":"t"}]}"#,
    );
    let IndexRead::Newer { version, partial } = read_index(d.path()) else {
        panic!("expected Newer")
    };
    assert_eq!(version, 7);
    assert_eq!(partial.files.len(), 1);
    assert_eq!(partial.files[0].tags, ["tráiler"]);
}

#[test]
fn oversize_file_is_malformed_and_never_parsed() {
    let d = extras_dir();
    let f = std::fs::File::create(d.path().join("extras.json")).unwrap();
    f.set_len(MAX_INDEX_BYTES + 1).unwrap();
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed {
            salvaged: None,
            unreadable_kind: Some(UnreadableKind::TooLarge),
            ..
        }
    ));
}

#[test]
fn a_folder_index_is_malformed() {
    let d = extras_dir();
    std::fs::create_dir(d.path().join("extras.json")).unwrap();
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed {
            salvaged: None,
            unreadable_kind: Some(UnreadableKind::Folder),
            ..
        }
    ));
}

#[cfg(unix)]
#[test]
fn a_symlinked_index_is_malformed_and_never_followed() {
    let d = extras_dir();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), OK).unwrap();
    std::os::unix::fs::symlink(outside.path(), d.path().join("extras.json")).unwrap();
    assert!(matches!(
        read_index(d.path()),
        IndexRead::Malformed {
            salvaged: None,
            unreadable_kind: Some(UnreadableKind::Symlink),
            ..
        }
    ));
}

#[test]
fn sync_values_survive_a_rewrite() {
    let mut f = ExtrasFile::empty();
    f.categories.entry("Manuals".into()).or_default().sync = Some(false);
    f.categories.entry("Videos".into()).or_default();
    let text = String::from_utf8(serialize(&f)).unwrap();
    assert!(text.contains("\"sync\": false"));
    assert!(text.contains("\"sync\": null"));
    let back: ExtrasFile = serde_json::from_str(&text).unwrap();
    assert_eq!(back.sync_for("Manuals"), Some(false));
    assert_eq!(back.sync_for("Videos"), None);
}

#[test]
fn unknown_fields_survive_read_and_serialize() {
    let d = extras_dir();
    write_index(&d, OK);
    let IndexRead::Ok(f) = read_index(d.path()) else {
        panic!()
    };
    let v: serde_json::Value = serde_json::from_slice(&serialize(&f)).unwrap();
    assert_eq!(v["future"]["kept"], true);
    assert_eq!(v["files"][0]["rating"], 5);
}

/// Every truncation and a fixed-seed byte-flip loop never panic.
#[test]
fn reading_never_panics_on_damaged_bytes() {
    let d = extras_dir();
    let bytes = OK.as_bytes();
    for n in 0..bytes.len() {
        write_index(&d, &bytes[..n]);
        let _ = read_index(d.path());
    }
    let mut seed: u64 = 0x5eed_c0de;
    for _ in 0..400 {
        let mut b = bytes.to_vec();
        for _ in 0..4 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let i = (seed >> 33) as usize % b.len();
            b[i] ^= (seed >> 17) as u8 | 1;
        }
        write_index(&d, &b);
        if let IndexRead::Ok(f)
        | IndexRead::Malformed {
            salvaged: Some(f), ..
        } = read_index(d.path())
        {
            let _ = serialize(&f);
        }
    }
}
