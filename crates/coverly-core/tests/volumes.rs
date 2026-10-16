use std::fs;

use std::path::Path;

use coverly_core::volumes::{is_system_mount, is_system_mount_on, list_volumes, Volume};

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

/// What `/Volumes` holds on a Mac with Time Machine snapshots and the APFS
/// helper volumes mounted, next to two real cards.
const MACOS_MOUNTS: [&str; 12] = [
    "com.apple.TimeMachine.localsnapshots",
    "Recovery",
    "Preboot",
    "VM",
    "Update",
    "xarts",
    "iSCPreboot",
    "Hardware",
    ".timemachine",
    "com.apple.something-else",
    "ANBERNIC",
    "SD CARD",
];

#[test]
fn macos_system_mounts_are_not_connected_drives() {
    let user: Vec<&str> = MACOS_MOUNTS
        .into_iter()
        .filter(|n| !is_system_mount_on(true, n, &Path::new("/Volumes").join(n)))
        .collect();
    assert_eq!(user, ["ANBERNIC", "SD CARD"]);
    assert!(is_system_mount_on(
        true,
        "Data",
        Path::new("/System/Volumes/Data")
    ));
    assert!(is_system_mount_on(
        true,
        "Macintosh HD - Data",
        Path::new("/System/Volumes/Data")
    ));
    assert!(!is_system_mount_on(
        true,
        "Data",
        Path::new("/Volumes/Data")
    ));
    assert!(!is_system_mount_on(
        true,
        "Updates",
        Path::new("/Volumes/Updates")
    ));
    assert_eq!(
        is_system_mount("VM", Path::new("/Volumes/VM")),
        cfg!(target_os = "macos"),
        "is_system_mount applies the macOS names only on macOS"
    );
}

#[test]
fn other_systems_keep_drives_named_like_macos_helpers() {
    // A Linux or Windows drive the user labelled "Update" or "VM" is theirs.
    for name in [
        "Recovery",
        "Preboot",
        "VM",
        "Update",
        "xarts",
        "iSCPreboot",
        "Hardware",
    ] {
        assert!(
            !is_system_mount_on(false, name, &Path::new("/media/me").join(name)),
            "{name}"
        );
    }
    assert!(!is_system_mount_on(
        false,
        "Data",
        Path::new("/System/Volumes/Data")
    ));
    // Hidden names and com.apple.* stay hidden everywhere.
    assert!(is_system_mount_on(
        false,
        ".Trash-1000",
        Path::new("/media/me/.Trash-1000")
    ));
    assert!(is_system_mount_on(
        false,
        "com.apple.TimeMachine.localsnapshots",
        Path::new("/media/me/x")
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn list_volumes_skips_the_macos_helpers_on_macos() {
    let parent = tempfile::tempdir().unwrap();
    for name in MACOS_MOUNTS {
        fs::create_dir(parent.path().join(name)).unwrap();
    }
    let names: Vec<String> = list_volumes(&[parent.path().to_path_buf()])
        .into_iter()
        .map(|v| v.name)
        .collect();
    assert_eq!(names, ["ANBERNIC", "SD CARD"]);
}

#[cfg(target_os = "macos")]
#[test]
fn a_link_into_system_volumes_is_skipped() {
    let parent = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        "/System/Volumes/Data",
        parent.path().join("Macintosh HD - Data"),
    )
    .unwrap();
    fs::create_dir(parent.path().join("SD")).unwrap();
    let names: Vec<String> = list_volumes(&[parent.path().to_path_buf()])
        .into_iter()
        .map(|v| v.name)
        .collect();
    assert_eq!(names, ["SD"]);
}
