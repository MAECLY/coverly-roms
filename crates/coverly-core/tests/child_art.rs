//! Part C D11: the art an update, DLC or demo carries in the clear is read
//! into its own slots beside it, journaled and undoable; keys are never
//! opened, Switch children give nothing (their NCAs are encrypted) and an
//! existing slot file is never overwritten.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use coverly_core::art::children::{
    art_json_path, child_slot_path, icon_preview, read_art_json, read_child_art, write_child_slot,
    ChildMedia, ChildOrigin, SkipWhy,
};
use coverly_core::consoles::testing::{
    cia, nsp, png_8x8, ps4_app, put_file, vita_sfo, wiiu_folder,
};
use coverly_core::consoles::{start_open_log, take_open_log};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::write::{undo_job, Writer};
use tempfile::TempDir;

/// The open log is one per process: the tests of this file take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `root` joined with the `/`-separated `rel`, by component.
fn at(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

/// Every file under `dir`, relative, as components (never slash text).
fn tree(dir: &Path) -> Vec<Vec<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<Vec<String>>) {
        let Ok(read) = fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap()
                        .components()
                        .filter_map(|c| match c {
                            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                            _ => None,
                        })
                        .collect(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn is_png(path: &Path) -> bool {
    fs::read(path).is_ok_and(|b| {
        image::guess_format(&b).ok() == Some(image::ImageFormat::Png)
            && image::load_from_memory(&b).is_ok()
    })
}

struct Lib {
    _tmp: TempDir,
    root: PathBuf,
    data: PathBuf,
}

fn lib() -> Lib {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("lib");
    let data = tmp.path().join("data");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&data).unwrap();
    // Canonical, so `/var` and `/private/var` never disagree.
    let root = fs::canonicalize(root).unwrap();
    Lib {
        _tmp: tmp,
        root,
        data,
    }
}

fn put(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    pin_mtime(path, 1);
}

/// A clear CIA DLC (title id high `0004008C`) with a meta SMDH.
fn cia_dlc(path: &Path) {
    put(path, &cia(0x0004_008C_0005_5D00, false, true).bytes());
}

/// A Vita app zip with `sce_sys/icon0.png` (synthetic bytes).
fn vita_zip(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("eboot.bin", vec![0u8; 256]),
        ("sce_sys/param.sfo", vita_sfo("PCSE00001")),
        ("sce_sys/icon0.png", png_8x8()),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    pin_mtime(path, 1);
}

fn run(lib: &Lib, children: &[&Path]) -> (coverly_core::art::children::ChildArtReport, String) {
    let mut writer = Writer::begin(&lib.root, &lib.data).unwrap();
    let report = read_child_art(children, &mut writer).unwrap();
    let job = writer.finish().unwrap();
    (report, job)
}

fn skipped(report: &coverly_core::art::children::ChildArtReport, child: &Path) -> Vec<SkipWhy> {
    report
        .skipped
        .iter()
        .filter(|(p, _)| p == child)
        .map(|(_, why)| *why)
        .collect()
}

#[test]
fn a_cia_dlc_with_a_meta_smdh_gets_its_icon() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/3DS/Game/DLC/Pack.cia");
    cia_dlc(&child);
    let (report, _) = run(&lib, &[&child]);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    assert!(is_png(&icon), "{report:?}");
    assert_eq!(report.written, [icon]);
    let art = read_art_json(&child);
    assert_eq!(art.icon.map(|s| s.origin), Some(ChildOrigin::Package));
    // A CIA is a file: it has no banner in the clear.
    assert_eq!(art.banner, None);
    assert!(!child_slot_path(&child, ChildMedia::Banner)
        .unwrap()
        .exists());
}

#[test]
fn a_vita_zip_child_gets_icon0() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/PSVITA/Game/DLC/Pack.zip");
    vita_zip(&child);
    let (report, _) = run(&lib, &[&child]);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    assert!(is_png(&icon), "{report:?}");
    assert_eq!(
        read_art_json(&child).icon.map(|s| s.origin),
        Some(ChildOrigin::Package)
    );
}

#[test]
fn a_folder_ps4_update_gets_icon_and_banner() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/PS4/Game/Updates/CUSA00900-patch");
    ps4_app(
        &child,
        "CUSA00900",
        "gp",
        "UP9000-CUSA00900_00-BLOODBORNE000000",
    );
    put_file(&child, "sce_sys/pic1.png", &png_8x8());
    let (report, _) = run(&lib, &[&child]);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    let banner = child_slot_path(&child, ChildMedia::Banner).unwrap();
    assert!(is_png(&icon), "{report:?}");
    assert!(is_png(&banner), "{report:?}");
    assert_eq!(report.written, [icon, banner]);
    let art = read_art_json(&child);
    assert_eq!(art.icon.map(|s| s.origin), Some(ChildOrigin::Package));
    assert_eq!(art.banner.map(|s| s.origin), Some(ChildOrigin::Package));
}

#[test]
fn switch_children_get_nothing() {
    let _g = serial();
    let lib = lib();
    let child = at(
        &lib.root,
        "ROMS/SWITCH/Game/DLC/Pack [0100000000011001][v0].nsp",
    );
    let nsp = nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
    put(&child, &nsp.bytes());
    let (report, _) = run(&lib, &[&child]);
    assert!(report.written.is_empty());
    assert_eq!(skipped(&report, &child), [SkipWhy::NoArtInClear]);
    assert!(!at(&lib.root, "ROMS/SWITCH/Game/DLC/.media").exists());
}

#[test]
fn keys_are_never_opened() {
    let _g = serial();
    let lib = lib();
    let dlc = at(&lib.root, "ROMS/PS3/Game/DLC");
    let tik = dlc.join("Pack.tik");
    let rap = dlc.join("EP0000-NPEB00000_00-PACK000000000000.rap");
    put(&tik, &[0xEE; 0x350]);
    put(&rap, &[0x11; 16]);
    start_open_log();
    let (report, _) = run(&lib, &[&tik, &rap]);
    let opened = take_open_log();
    assert!(!opened.contains(&tik), "{opened:?}");
    assert!(!opened.contains(&rap), "{opened:?}");
    assert!(report.written.is_empty());
    assert_eq!(skipped(&report, &tik), [SkipWhy::KeyMaterial]);
    assert_eq!(skipped(&report, &rap), [SkipWhy::KeyMaterial]);
    assert!(!dlc.join(".media").exists());
}

#[test]
fn an_existing_icon_is_kept() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/3DS/Game/DLC/Pack.cia");
    cia_dlc(&child);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    put(&icon, b"the user's own icon");
    start_open_log();
    let (report, _) = run(&lib, &[&child]);
    // A filled slot is not read for: the package is never opened.
    let opened = take_open_log();
    assert!(!opened.contains(&child), "{opened:?}");
    assert_eq!(fs::read(&icon).unwrap(), b"the user's own icon");
    assert_eq!(skipped(&report, &child), [SkipWhy::Exists]);
    assert!(report.written.is_empty());
    // Nothing written: no origin claimed for the user's file.
    assert!(!art_json_path(&child).exists());

    // A case-insensitive twin counts as the same slot.
    let lib = self::lib();
    let child = at(&lib.root, "ROMS/3DS/Game/DLC/Pack.cia");
    cia_dlc(&child);
    let twin = at(&lib.root, "ROMS/3DS/Game/DLC/.media/PACK-ICON.PNG");
    put(&twin, b"twin");
    let (report, _) = run(&lib, &[&child]);
    assert_eq!(fs::read(&twin).unwrap(), b"twin");
    assert_eq!(skipped(&report, &child), [SkipWhy::Exists]);
}

/// Only the empty slot of a folder child is read for: with the icon slot
/// filled, the banner is written and `sce_sys/icon0.png` is never opened;
/// with both filled, nothing inside the child is opened at all.
#[test]
fn only_empty_slots_are_read_for() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/PS4/Game/Updates/CUSA00900-patch");
    ps4_app(
        &child,
        "CUSA00900",
        "gp",
        "UP9000-CUSA00900_00-BLOODBORNE000000",
    );
    put_file(&child, "sce_sys/pic1.png", &png_8x8());
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    let banner = child_slot_path(&child, ChildMedia::Banner).unwrap();
    put(&icon, b"mine");
    start_open_log();
    let (report, _) = run(&lib, &[&child]);
    let opened = take_open_log();
    assert!(
        !opened.contains(&at(&child, "sce_sys/icon0.png")),
        "{opened:?}"
    );
    assert_eq!(report.written, [banner], "{report:?}");
    assert_eq!(fs::read(&icon).unwrap(), b"mine");
    let art = read_art_json(&child);
    assert_eq!(art.icon, None);
    assert_eq!(art.banner.map(|s| s.origin), Some(ChildOrigin::Package));

    start_open_log();
    let (report, _) = run(&lib, &[&child]);
    let opened = take_open_log();
    assert!(!opened.iter().any(|p| p.starts_with(&child)), "{opened:?}");
    assert!(report.written.is_empty());
    assert_eq!(skipped(&report, &child), [SkipWhy::Exists]);
}

#[test]
fn undo_removes_what_it_wrote() {
    let _g = serial();
    let lib = lib();
    let fresh = at(&lib.root, "ROMS/3DS/Game/DLC/Pack.cia");
    cia_dlc(&fresh);
    let before_fresh = tree(&lib.root);
    let (_, job) = run(&lib, &[&fresh]);
    assert!(child_slot_path(&fresh, ChildMedia::Icon).unwrap().is_file());
    undo_job(&lib.data, &job).unwrap();
    assert_eq!(tree(&lib.root), before_fresh);
    // `.media/` was created by the job and is empty again: gone.
    assert!(!at(&lib.root, "ROMS/3DS/Game/DLC/.media").exists());

    // `.media/` already there with an older origin file: kept, the
    // origin file restored byte for byte.
    let other = at(&lib.root, "ROMS/3DS/Other/DLC/Pack.cia");
    cia_dlc(&other);
    let old_json = br#"{ "banner": { "origin": "user", "at": 3 } }"#;
    put(&art_json_path(&other), old_json);
    let banner = child_slot_path(&other, ChildMedia::Banner).unwrap();
    put(&banner, b"banner by hand");
    let before = tree(&lib.root);
    let (report, job) = run(&lib, &[&other]);
    assert_eq!(
        report.written,
        [child_slot_path(&other, ChildMedia::Icon).unwrap()]
    );
    let art = read_art_json(&other);
    assert_eq!(art.icon.map(|s| s.origin), Some(ChildOrigin::Package));
    assert_eq!(art.banner.map(|s| s.origin), Some(ChildOrigin::User));
    undo_job(&lib.data, &job).unwrap();
    assert_eq!(tree(&lib.root), before);
    assert_eq!(fs::read(art_json_path(&other)).unwrap(), old_json);
    assert_eq!(fs::read(&banner).unwrap(), b"banner by hand");
    assert!(at(&lib.root, "ROMS/3DS/Other/DLC/.media").is_dir());
}

#[test]
fn a_child_outside_the_library_is_refused() {
    let _g = serial();
    let lib = lib();
    let elsewhere = TempDir::new().unwrap();
    let child = at(elsewhere.path(), "Game/DLC/Pack.cia");
    cia_dlc(&child);
    let (report, _) = run(&lib, &[&child]);
    assert!(report.written.is_empty());
    assert_eq!(skipped(&report, &child), [SkipWhy::Refused]);
    assert!(!at(elsewhere.path(), "Game/DLC/.media").exists());
}

fn tga(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 120, 200, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Tga).unwrap();
    out.into_inner()
}

/// A Wii U update folder: icon from `meta/iconTex.tga`, and its 1280×720
/// `meta/bootTvTex.tga` (wider than an icon may be) scaled down to fit
/// the icon side.
#[test]
fn a_wiiu_folder_update_gets_a_scaled_banner() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/WIIU/Game/Updates/Game (v32)");
    wiiu_folder(&child, "0005000E10101D00", "WUP-P-ARKE", "Test Game");
    put_file(&child, "meta/iconTex.tga", &tga(128, 128));
    put_file(&child, "meta/bootTvTex.tga", &tga(1280, 720));
    let (report, _) = run(&lib, &[&child]);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    let banner = child_slot_path(&child, ChildMedia::Banner).unwrap();
    assert_eq!(report.written, [icon.clone(), banner.clone()], "{report:?}");
    let img = image::load_from_memory(&fs::read(&banner).unwrap()).unwrap();
    assert_eq!((img.width(), img.height()), (1024, 576));
    assert!(is_png(&icon));
}

/// A slot path over 259 UTF-16 units: nothing read, nothing written.
#[test]
fn a_slot_path_too_long_is_skipped() {
    let _g = serial();
    let lib = lib();
    let units = lib.root.to_string_lossy().encode_utf16().count();
    let pad = "p".repeat(270usize.saturating_sub(units + 40).max(1));
    let child = at(&lib.root, &format!("ROMS/3DS/{pad}/DLC/Pack.cia"));
    if fs::create_dir_all(child.parent().unwrap()).is_err() {
        return; // a file system that refuses the long path itself
    }
    cia_dlc(&child);
    start_open_log();
    let (report, _) = run(&lib, &[&child]);
    let opened = take_open_log();
    assert!(!opened.contains(&child), "{opened:?}");
    assert!(report.written.is_empty());
    assert_eq!(skipped(&report, &child), [SkipWhy::PathTooLong]);
}

/// Part C D15: Organizar previews the icon a pile's update or DLC carries
/// in the clear, from the source file, without writing anything.
#[test]
fn an_icon_preview_reads_the_package_and_writes_nothing() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "Downloads/Pack.cia");
    cia_dlc(&child);
    let before = tree(&lib.root);
    let png = icon_preview(&child).expect("the CIA's meta SMDH icon");
    assert_eq!(
        image::guess_format(&png).ok(),
        Some(image::ImageFormat::Png)
    );
    assert_eq!(tree(&lib.root), before);
    assert!(tree(&lib.data).is_empty());
}

#[test]
fn an_icon_preview_of_a_switch_child_or_a_key_is_none() {
    let _g = serial();
    let lib = lib();
    let nsp_path = at(&lib.root, "Downloads/Pack [0100000000011001][v0].nsp");
    let nsp = nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
    put(&nsp_path, &nsp.bytes());
    assert_eq!(icon_preview(&nsp_path), None);
    let tik = at(&lib.root, "Downloads/Pack.tik");
    put(&tik, &[0xEE; 0x350]);
    start_open_log();
    assert_eq!(icon_preview(&tik), None);
    let opened = take_open_log();
    assert!(!opened.contains(&tik), "{opened:?}");
}

/// «Buscar arte» (D11): a picked image fills the slot, stamped with the
/// service it came from, in a job undone like any manual write.
#[test]
fn a_picked_image_fills_the_slot_with_its_origin() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/PS4/Game/DLC/Pack.pkg");
    put(&child, b"synthetic package");
    let before = tree(&lib.root);
    let mut writer = Writer::begin(&lib.root, &lib.data).unwrap();
    start_open_log();
    let why = write_child_slot(
        &child,
        ChildMedia::Icon,
        &png_8x8(),
        ChildOrigin::Screenscraper,
        &mut writer,
    )
    .unwrap();
    // The package itself is never opened for a picked image.
    assert!(!take_open_log().contains(&child));
    let job = writer.finish().unwrap();
    assert_eq!(why, None);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    assert!(is_png(&icon));
    let art = read_art_json(&child);
    assert_eq!(art.icon.map(|s| s.origin), Some(ChildOrigin::Screenscraper));
    assert_eq!(art.banner, None);
    undo_job(&lib.data, &job).unwrap();
    assert_eq!(tree(&lib.root), before);
}

/// An explicit pick replaces the slot's file only with the usual backup:
/// undo gives the old icon and origin back byte for byte.
#[test]
fn a_picked_image_backs_up_the_slot_it_replaces() {
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/3DS/Game/DLC/Pack.cia");
    cia_dlc(&child);
    let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
    put(&icon, b"the package icon");
    let old_json = br#"{ "icon": { "origin": "package", "at": 3 } }"#;
    put(&art_json_path(&child), old_json);
    let mut writer = Writer::begin(&lib.root, &lib.data).unwrap();
    let why = write_child_slot(
        &child,
        ChildMedia::Icon,
        &png_8x8(),
        ChildOrigin::Steamgriddb,
        &mut writer,
    )
    .unwrap();
    let job = writer.finish().unwrap();
    assert_eq!(why, None);
    assert!(is_png(&icon));
    assert_eq!(
        read_art_json(&child).icon.map(|s| s.origin),
        Some(ChildOrigin::Steamgriddb)
    );
    undo_job(&lib.data, &job).unwrap();
    assert_eq!(fs::read(&icon).unwrap(), b"the package icon");
    assert_eq!(fs::read(art_json_path(&child)).unwrap(), old_json);
}

/// A key or licence file never gets art, picked or not.
#[test]
fn a_picked_image_is_never_written_for_a_key_file() {
    let _g = serial();
    let lib = lib();
    let key = at(&lib.root, "ROMS/PS3/Game/DLC/Pack.rap");
    put(&key, b"not a real licence");
    let before = tree(&lib.root);
    let mut writer = Writer::begin(&lib.root, &lib.data).unwrap();
    let why = write_child_slot(
        &key,
        ChildMedia::Icon,
        &png_8x8(),
        ChildOrigin::Screenscraper,
        &mut writer,
    )
    .unwrap();
    writer.discard().unwrap();
    assert_eq!(why, Some(SkipWhy::KeyMaterial));
    assert_eq!(tree(&lib.root), before);
}

/// The panel's write is a job of its own: none is left behind when
/// nothing was written.
#[test]
fn a_picked_image_is_its_own_job_or_none() {
    use coverly_core::fill::{write_child_image, ManualOptions};
    let _g = serial();
    let lib = lib();
    let child = at(&lib.root, "ROMS/PS4/Game/DLC/Pack.pkg");
    put(&child, b"synthetic package");
    let opts = ManualOptions::default();
    let (job, why) = write_child_image(
        &lib.root,
        &child,
        ChildMedia::Icon,
        &png_8x8(),
        ChildOrigin::Steamgriddb,
        &lib.data,
        &opts,
    )
    .unwrap();
    assert_eq!(why, None);
    let job = job.expect("a job");
    assert!(child_slot_path(&child, ChildMedia::Icon).unwrap().is_file());
    undo_job(&lib.data, &job).unwrap();

    let key = at(&lib.root, "ROMS/PS4/Game/DLC/Pack.rap");
    put(&key, b"not a real licence");
    let (job, why) = write_child_image(
        &lib.root,
        &key,
        ChildMedia::Icon,
        &png_8x8(),
        ChildOrigin::Steamgriddb,
        &lib.data,
        &opts,
    )
    .unwrap();
    assert_eq!((job, why), (None, Some(SkipWhy::KeyMaterial)));
}

/// A folder child whose `meta/` is a symlink to a folder outside the
/// library: neither its icon nor its banner is read through the link, so
/// no image from outside the library lands in its slots.
#[cfg(unix)]
#[test]
fn art_is_never_read_through_a_linked_folder_inside_a_child() {
    let _g = serial();
    let lib = lib();
    let elsewhere = TempDir::new().unwrap();
    let real = elsewhere.path().join("app");
    wiiu_folder(&real, "0005000E10101D00", "WUP-P-ARKE", "Test Game");
    put_file(&real, "meta/iconTex.tga", &tga(128, 128));
    put_file(&real, "meta/bootTvTex.tga", &tga(1280, 720));
    let child = at(&lib.root, "ROMS/WIIU/Game/Updates/Game (v32)");
    wiiu_folder(&child, "0005000E10101D00", "WUP-P-ARKE", "Test Game");
    fs::remove_dir_all(child.join("meta")).unwrap();
    std::os::unix::fs::symlink(real.join("meta"), child.join("meta")).unwrap();
    let (report, _) = run(&lib, &[&child]);
    assert!(report.written.is_empty(), "{report:?}");
    for media in ChildMedia::ALL {
        assert!(!child_slot_path(&child, media).unwrap().exists());
    }
    assert_eq!(icon_preview(&child), None);
}
