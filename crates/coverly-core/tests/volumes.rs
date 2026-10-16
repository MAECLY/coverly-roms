use std::fs;

use coverly_core::volumes::{list_volumes, Volume};

#[test]
fn lists_child_folders_of_the_mount_parents() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::create_dir(a.path().join("SD CARD")).unwrap();
    fs::create_dir(a.path().join(".Spotlight")).unwrap();
    fs::write(a.path().join("file"), b"x").unwrap();
    fs::create_dir(b.path().join("ANBERNIC")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/", a.path().join("Macintosh HD")).unwrap();
    let missing = a.path().join("missing");
    let got = list_volumes(&[
        a.path().to_path_buf(),
        b.path().to_path_buf(),
        missing,
        a.path().to_path_buf(),
    ]);
    let names: Vec<&str> = got.iter().map(|v: &Volume| v.name.as_str()).collect();
    assert_eq!(names, ["ANBERNIC", "SD CARD"]);
    assert!(got[1].path.ends_with("SD CARD"));
}

#[test]
fn a_volume_serialises_as_path_and_name() {
    let v = Volume {
        path: "/Volumes/SD".into(),
        name: "SD".into(),
    };
    assert_eq!(
        serde_json::to_value(v).unwrap(),
        serde_json::json!({ "path": "/Volumes/SD", "name": "SD" })
    );
}
