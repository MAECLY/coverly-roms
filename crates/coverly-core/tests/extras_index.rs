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

// ---- Listing and reconcile (D1, D4) ----

mod listing {
    use std::path::{Component, Path};

    use coverly_core::dedupe::testing::pin_mtime;
    use coverly_core::extras::index::{list, IndexState, ListedFile, Listing, Match, LIST_CAP};
    use coverly_core::extras::{CategoryRef, ExtrasCategory, OriginHow, Provenance};
    use serde_json::json;

    const BASE_S: i64 = 1_700_000_000;

    /// The mtime `pin_mtime(_, n)` gives, in nanoseconds.
    fn mtime_of(n: u64) -> i64 {
        (BASE_S + n as i64) * 1_000_000_000
    }

    /// Writes `bytes` at `rel` (`/`-separated) under `dir` and pins its mtime.
    fn put(dir: &Path, rel: &str, bytes: &[u8], n: u64) {
        let path = rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        pin_mtime(&path, n);
    }

    fn record(path: &str, size: u64, n: u64, tags: &[&str]) -> serde_json::Value {
        json!({
            "path": path, "type": "audio", "provenance": "official",
            "origin": {"how": "organize", "format": "spc"},
            "tags": tags, "size": size, "mtime_ns": mtime_of(n),
            "added_at": "2026-11-13T00:00:00Z"
        })
    }

    fn index(dir: &Path, records: Vec<serde_json::Value>) {
        let v = json!({"version": 1, "categories": {}, "files": records});
        std::fs::write(dir.join("extras.json"), serde_json::to_vec(&v).unwrap()).unwrap();
    }

    /// The `/`-joined path as plain components.
    fn parts(p: &str) -> Vec<String> {
        Path::new(
            &p.split('/')
                .fold(std::path::PathBuf::new(), |b, c| b.join(c)),
        )
        .components()
        .map(|c| match c {
            Component::Normal(n) => n.to_string_lossy().into_owned(),
            other => panic!("not a plain component: {other:?}"),
        })
        .collect()
    }

    fn file<'a>(l: &'a Listing, rel: &str) -> &'a ListedFile {
        let want = parts(rel);
        l.files
            .iter()
            .find(|f| parts(&f.path) == want)
            .unwrap_or_else(|| panic!("{rel} not listed in {:?}", l.files))
    }

    fn paths(l: &Listing) -> Vec<Vec<String>> {
        l.files.iter().map(|f| parts(&f.path)).collect()
    }

    fn case_insensitive(dir: &Path) -> bool {
        std::fs::write(dir.join("probe-a"), b"").unwrap();
        let ci = dir.join("PROBE-A").exists();
        std::fs::remove_file(dir.join("probe-a")).unwrap();
        ci
    }

    #[test]
    fn same_path_keeps_tags() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/a.flac", b"abcd", 1);
        index(d.path(), vec![record("Soundtrack/a.flac", 4, 1, &["boss"])]);
        let l = list(d.path());
        assert_eq!(l.index, IndexState::Ok);
        let f = file(&l, "Soundtrack/a.flac");
        assert_eq!(f.matched, Match::Path);
        assert_eq!(f.record.tags, ["boss"]);
        assert_eq!(f.record.provenance, Provenance::Official);
        assert_eq!(f.category, CategoryRef::Fixed(ExtrasCategory::Soundtrack));
        assert_eq!(f.name, "a.flac");
        assert_eq!((f.size, f.mtime_ns), (4, mtime_of(1)));
        assert!(l.missing.is_empty());
        assert!(!l.capped);
    }

    #[test]
    fn nfd_on_disk_matches_nfc_in_json_and_keeps_disk_spelling() {
        let d = tempfile::tempdir().unwrap();
        let nfd = "Cancio\u{301}n.flac";
        put(d.path(), &format!("Soundtrack/{nfd}"), b"abcd", 1);
        index(
            d.path(),
            vec![record("Soundtrack/Canci\u{f3}n.flac", 4, 9, &["boss"])],
        );
        let l = list(d.path());
        assert_eq!(l.files.len(), 1);
        let f = &l.files[0];
        assert_eq!(f.matched, Match::Path);
        assert_eq!(f.record.tags, ["boss"]);
        // The disk spelling is kept (APFS keeps the bytes it was given;
        // HFS+ stores NFD whatever it was given).
        let on_disk: Vec<String> = std::fs::read_dir(d.path().join("Soundtrack"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            parts(&f.path),
            ["Soundtrack".to_string(), on_disk[0].clone()]
        );
        assert_eq!(f.record.path, f.path);
        assert!(l.missing.is_empty());
    }

    #[test]
    fn renamed_in_finder_matches_by_size_and_mtime() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/new.flac", b"abcd", 7);
        index(
            d.path(),
            vec![record("Soundtrack/old.flac", 4, 7, &["boss"])],
        );
        let l = list(d.path());
        let f = file(&l, "Soundtrack/new.flac");
        assert_eq!(f.matched, Match::Moved);
        assert_eq!(f.record.tags, ["boss"]);
        assert_eq!(f.record.provenance, Provenance::Official);
        assert_eq!(parts(&f.record.path), parts("Soundtrack/new.flac"));
        assert!(l.missing.is_empty());
    }

    #[test]
    fn moved_across_categories_matches_by_name_and_size_when_mtime_changed() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Videos/a.flac", b"abcd", 30);
        index(d.path(), vec![record("Soundtrack/a.flac", 4, 1, &["boss"])]);
        let l = list(d.path());
        let f = file(&l, "Videos/a.flac");
        assert_eq!(f.matched, Match::Moved);
        assert_eq!(f.record.tags, ["boss"]);
        assert_eq!(f.category, CategoryRef::Fixed(ExtrasCategory::Videos));
        assert_eq!((f.record.size, f.record.mtime_ns), (4, mtime_of(30)));
        assert!(l.missing.is_empty());
    }

    #[test]
    fn identical_twins_never_merge() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/x.flac", b"abcd", 7);
        put(d.path(), "Soundtrack/y.flac", b"abcd", 7);
        index(
            d.path(),
            vec![record("Soundtrack/gone.flac", 4, 7, &["boss"])],
        );
        let l = list(d.path());
        assert_eq!(l.files.len(), 2);
        for f in &l.files {
            assert_eq!(f.matched, Match::Found);
            assert!(f.record.tags.is_empty());
        }
        assert_eq!(l.missing.len(), 1);
        assert_eq!(l.missing[0].tags, ["boss"]);
    }

    #[test]
    fn two_records_never_share_one_file() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/x.flac", b"abcd", 7);
        index(
            d.path(),
            vec![
                record("Soundtrack/old1.flac", 4, 7, &["one"]),
                record("Soundtrack/old2.flac", 4, 7, &["two"]),
            ],
        );
        let l = list(d.path());
        assert_eq!(file(&l, "Soundtrack/x.flac").matched, Match::Found);
        assert_eq!(l.missing.len(), 2);
    }

    #[test]
    fn unmatched_records_are_kept_as_missing_with_tags() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/a.flac", b"abcd", 1);
        index(
            d.path(),
            vec![
                record("Soundtrack/a.flac", 4, 1, &[]),
                record("Manuals/lost.pdf", 99, 3, &["tema del jefe final"]),
            ],
        );
        let l = list(d.path());
        assert_eq!(l.files.len(), 1);
        assert_eq!(l.missing.len(), 1);
        assert_eq!(parts(&l.missing[0].path), parts("Manuals/lost.pdf"));
        assert_eq!(l.missing[0].tags, ["tema del jefe final"]);
        // The base keeps every record for the next write.
        assert_eq!(l.base.as_ref().unwrap().files.len(), 2);
    }

    #[test]
    fn unrecorded_files_are_found_user_records() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Artwork/front.png", b"png!", 2);
        let l = list(d.path());
        assert_eq!(l.index, IndexState::Absent);
        assert!(l.base.is_none());
        let f = file(&l, "Artwork/front.png");
        assert_eq!(f.matched, Match::Found);
        assert_eq!(f.record.provenance, Provenance::User);
        assert_eq!(f.record.origin.how, OriginHow::Found);
        assert_eq!(f.record.kind, coverly_core::extras::ExtraType::Image);
        assert!(f.record.tags.is_empty());
        assert_eq!((f.record.size, f.record.mtime_ns), (4, mtime_of(2)));
        assert_eq!(f.record.added_at, "2023-11-14T22:13:22Z");
    }

    #[test]
    fn two_spellings_of_a_fixed_category_are_one_category() {
        let d = tempfile::tempdir().unwrap();
        if case_insensitive(d.path()) {
            return; // one spelling is all a case-insensitive disk can hold
        }
        put(d.path(), "Soundtrack/a.flac", b"a", 1);
        put(d.path(), "soundtrack/b.flac", b"b", 2);
        let l = list(d.path());
        let st = CategoryRef::Fixed(ExtrasCategory::Soundtrack);
        let cats: Vec<_> = l.categories.iter().filter(|c| c.category == st).collect();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].count, 2);
        assert_eq!(cats[0].spellings, ["Soundtrack", "soundtrack"]);
        assert!(l.files.iter().all(|f| f.category == st));
    }

    #[test]
    fn index_and_backups_at_the_root_are_not_files_but_lookalikes_are() {
        let d = tempfile::tempdir().unwrap();
        index(d.path(), vec![]);
        put(d.path(), "extras.broken-20261113-120000.json", b"{", 1);
        put(d.path(), "extras.broken-20261113-120000-2.json", b"{", 1);
        put(d.path(), "Guides/extras-notes.json", b"{}", 2);
        put(d.path(), "Guides/extras.json", b"{}", 3);
        let l = list(d.path());
        assert_eq!(
            paths(&l),
            [
                parts("Guides/extras-notes.json"),
                parts("Guides/extras.json")
            ]
        );
    }

    #[test]
    fn hidden_and_junk_are_never_listed_whatever_include_hidden_says() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/.DS_Store", b"x", 1);
        put(d.path(), "Soundtrack/._a.flac", b"x", 1);
        put(d.path(), "Soundtrack/.secret", b"x", 1);
        put(d.path(), ".hidden/inside.flac", b"x", 1);
        put(d.path(), "Soundtrack/Thumbs.db", b"x", 1);
        put(d.path(), "Soundtrack/a.flac", b"x", 1);
        let l = list(d.path());
        assert_eq!(paths(&l), [parts("Soundtrack/a.flac")]);
        assert_eq!(l.categories.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_listed_as_links_and_never_followed() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        put(outside.path(), "secret.flac", b"abcd", 1);
        std::os::unix::fs::symlink(outside.path(), d.path().join("Linked")).unwrap();
        std::fs::create_dir(d.path().join("Soundtrack")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.flac"),
            d.path().join("Soundtrack").join("link.flac"),
        )
        .unwrap();
        let l = list(d.path());
        let linked = l
            .categories
            .iter()
            .find(|c| c.category == CategoryRef::User("Linked".into()))
            .expect("the linked category is listed");
        assert!(linked.link);
        assert_eq!(linked.count, 0);
        assert_eq!(paths(&l), [parts("Soundtrack/link.flac")]);
        assert!(l.files[0].link);
    }

    #[test]
    fn files_directly_in_extras_are_root_category() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "loose.txt", b"hi", 1);
        let l = list(d.path());
        let f = file(&l, "loose.txt");
        assert_eq!(f.category, CategoryRef::Root);
        let root = l
            .categories
            .iter()
            .find(|c| c.category == CategoryRef::Root)
            .unwrap();
        assert_eq!(root.count, 1);
    }

    #[test]
    fn deeper_files_keep_their_sub_path() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Artwork/Scans/front.png", b"png", 1);
        put(d.path(), "Artwork/a/b/c/too-deep.png", b"png", 1);
        let l = list(d.path());
        let f = file(&l, "Artwork/Scans/front.png");
        assert_eq!(f.category, CategoryRef::Fixed(ExtrasCategory::Artwork));
        assert_eq!(f.name, "front.png");
        // Depth 4 below Extras/ at most.
        assert_eq!(l.files.len(), 1);
    }

    #[test]
    fn the_listing_stops_at_ten_thousand_files_and_says_so() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("Soundtrack");
        std::fs::create_dir(&dir).unwrap();
        for i in 0..=LIST_CAP {
            std::fs::write(dir.join(format!("{i:05}.flac")), b"").unwrap();
        }
        // A record whose file is past the cap is not reported missing.
        index(
            d.path(),
            vec![record(
                &format!("Soundtrack/{LIST_CAP:05}.flac"),
                0,
                1,
                &["x"],
            )],
        );
        let l = list(d.path());
        assert_eq!(l.files.len(), LIST_CAP);
        assert!(l.capped);
        assert!(l.missing.is_empty());
    }

    #[test]
    fn listing_never_writes() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/a.flac", b"abcd", 1);
        put(d.path(), "Soundtrack/new.flac", b"abcd", 7);
        let before_mtime = std::fs::metadata(d.path()).unwrap().modified().unwrap();
        let names = |p: &Path| {
            let mut v: Vec<_> = walk(p);
            v.sort();
            v
        };
        let before = names(d.path());
        let _ = list(d.path());
        assert!(!d.path().join("extras.json").exists());
        assert_eq!(names(d.path()), before);
        assert_eq!(
            std::fs::metadata(d.path()).unwrap().modified().unwrap(),
            before_mtime
        );
        // With an index that reconcile changes, still nothing written.
        index(
            d.path(),
            vec![record("Soundtrack/old.flac", 4, 7, &["boss"])],
        );
        let json = std::fs::read(d.path().join("extras.json")).unwrap();
        let _ = list(d.path());
        assert_eq!(std::fs::read(d.path().join("extras.json")).unwrap(), json);
    }

    fn walk(p: &Path) -> Vec<(std::path::PathBuf, u64)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(p).unwrap() {
            let e = e.unwrap();
            let m = e.metadata().unwrap();
            if m.is_dir() {
                out.extend(walk(&e.path()));
            }
            out.push((e.path(), m.len()));
        }
        out
    }

    #[test]
    fn user_categories_sort_after_fixed_alphabetically() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "zeta/a.flac", b"a", 1);
        put(d.path(), "Videos/a.mp4", b"a", 1);
        put(d.path(), "Remixes/a.flac", b"a", 1);
        put(d.path(), "alpha/a.flac", b"a", 1);
        put(d.path(), "Soundtrack/a.flac", b"a", 1);
        std::fs::create_dir(d.path().join("Empty")).unwrap();
        put(d.path(), "loose.txt", b"a", 1);
        let l = list(d.path());
        let order: Vec<CategoryRef> = l.categories.iter().map(|c| c.category.clone()).collect();
        assert_eq!(
            order,
            [
                CategoryRef::Fixed(ExtrasCategory::Soundtrack),
                CategoryRef::Fixed(ExtrasCategory::Videos),
                CategoryRef::User("alpha".into()),
                CategoryRef::User("Empty".into()),
                CategoryRef::User("Remixes".into()),
                CategoryRef::User("zeta".into()),
                CategoryRef::Root,
            ]
        );
        let empty = &l.categories[3];
        assert_eq!(empty.count, 0);
    }

    #[test]
    fn a_missing_extras_folder_is_an_empty_listing() {
        let d = tempfile::tempdir().unwrap();
        let l = list(&d.path().join("Extras"));
        assert_eq!(l.index, IndexState::Absent);
        assert!(l.files.is_empty() && l.categories.is_empty() && l.missing.is_empty());
    }

    #[test]
    fn a_malformed_index_lists_with_what_was_salvaged() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/a.flac", b"abcd", 1);
        let v = json!({"version": 1, "files": [record("Soundtrack/a.flac", 4, 1, &["boss"]), {"nope": 1}]});
        std::fs::write(
            d.path().join("extras.json"),
            serde_json::to_vec(&v).unwrap(),
        )
        .unwrap();
        let l = list(d.path());
        assert_eq!(
            l.index,
            IndexState::Malformed {
                bad_records: 1,
                unreadable: None
            }
        );
        assert_eq!(file(&l, "Soundtrack/a.flac").record.tags, ["boss"]);
    }

    #[test]
    fn a_newer_index_lists_view_only_with_its_records() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "Soundtrack/a.flac", b"abcd", 1);
        let v = json!({"version": 2, "files": [record("Soundtrack/a.flac", 4, 1, &["boss"])]});
        std::fs::write(
            d.path().join("extras.json"),
            serde_json::to_vec(&v).unwrap(),
        )
        .unwrap();
        let l = list(d.path());
        assert_eq!(l.index, IndexState::Newer { version: 2 });
        assert_eq!(file(&l, "Soundtrack/a.flac").record.tags, ["boss"]);
    }
}
