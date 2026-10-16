use std::path::PathBuf;

use coverly_core::jobs::{write_report_csv, ItemErrorKind, ItemResult, ItemStatus, JobReport};
use coverly_core::profiles::Media;

#[test]
fn job_report_csv_neutralises_formulas() {
    let report = JobReport {
        items: vec![
            ItemResult {
                folder: "GBA".into(),
                rel_path: PathBuf::from("=HYPERLINK(\"http://x\").gba"),
                media: Media::Cover,
                target: PathBuf::from("/sd/x.png"),
                status: ItemStatus::Filled {
                    source: "libretro".into(),
                    name: "Golden Sun (USA, Europe)".into(),
                },
            },
            ItemResult {
                folder: "GBA".into(),
                rel_path: PathBuf::from("b.gba"),
                media: Media::Cover,
                target: PathBuf::from("/sd/b.png"),
                status: ItemStatus::Failed {
                    kind: ItemErrorKind::Decode,
                    message: "-cmd".into(),
                },
            },
        ],
        ..JobReport::default()
    };
    let mut out = Vec::new();
    write_report_csv(&report, &mut out).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "folder,rel_path,media,status,source,name,error,message\n\
         GBA,\"'=HYPERLINK(\"\"http://x\"\").gba\",cover,filled,libretro,\"Golden Sun (USA, Europe)\",,\n\
         GBA,b.gba,cover,failed,,,decode,'-cmd\n"
    );
}

#[test]
fn error_kinds_use_their_wire_names() {
    for kind in [
        ItemErrorKind::NoArt,
        ItemErrorKind::Network,
        ItemErrorKind::Decode,
        ItemErrorKind::WriteDenied,
        ItemErrorKind::DiskFull,
        ItemErrorKind::NameInvalid,
        ItemErrorKind::OutsideRoot,
        ItemErrorKind::VolumeGone,
        ItemErrorKind::Io,
    ] {
        assert_eq!(
            serde_json::to_value(kind).unwrap().as_str(),
            Some(kind.as_str())
        );
    }
}
