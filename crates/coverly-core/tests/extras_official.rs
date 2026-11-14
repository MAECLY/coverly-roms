//! Official extras told apart from their plaintext headers (extras spec
//! D6, §5): PS3 `.p3t` and theme/avatar PKGs, the Vita theme PKG, the PSP
//! `.ptf`, Xbox 360 STFS extras by content type and 3DS soundtrack CIAs.
//! No key is read and key material is never opened.

use coverly_core::consoles::testing::pkg;
use coverly_core::consoles::{self, Budgeted};
use coverly_core::extras::official::{sniff_bytes, sniff_path, OfficialExtra};
use coverly_core::extras::testing::{
    all_official, cia_titled, p3t, pkg_with_entries, pkg_with_type, stfs_extra,
};
use coverly_core::extras::{ExtraFormat, ExtrasCategory};
use coverly_core::identify::sniff::SliceAt;

const APP: u64 = 0x0004_0000_0012_3400;

fn sniff(name: &str, bytes: &[u8]) -> Option<OfficialExtra> {
    sniff_bytes(name, bytes)
}

#[test]
fn ps3_p3t_by_magic() {
    let x = sniff("Theme.p3t", &p3t("Tidewalker")).unwrap();
    assert_eq!(x.format, ExtraFormat::Ps3P3t);
    assert_eq!(x.category, ExtrasCategory::Themes);
    assert_eq!(x.display_name.as_deref(), Some("Tidewalker"));
    assert_eq!(x.title_id, None);
    // Without the magic it is no theme; without a readable name the stem.
    assert_eq!(sniff("Theme.p3t", &[0u8; 0x80]), None);
    let mut nameless = b"P3TF".to_vec();
    nameless.extend_from_slice(&[0u8; 0x40]);
    let x = sniff("Night Sky.P3T", &nameless).unwrap();
    assert_eq!(x.display_name.as_deref(), Some("Night Sky"));
    // The magic alone under another extension is not a theme.
    assert_eq!(sniff("Theme.bin", &p3t("Tidewalker")), None);
}

#[test]
fn ps3_pkg_theme_and_avatar_by_metadata_content_type() {
    let theme = pkg_with_type(1, 0x09, "UP0001-BLUS30001_00-THEME0000000001");
    let x = sniff("theme.pkg", &theme).unwrap();
    assert_eq!(x.format, ExtraFormat::Ps3ThemePkg);
    assert_eq!(x.category, ExtrasCategory::Themes);
    assert_eq!(x.title_id.as_deref(), Some("BLUS30001"));
    assert_eq!(x.content_type, Some(0x09));
    let avatar = pkg_with_type(1, 0x0D, "UP0001-BLUS30001_00-AVATAR000000001");
    let x = sniff("avatar.pkg", &avatar).unwrap();
    assert_eq!(x.format, ExtraFormat::Ps3AvatarPkg);
    assert_eq!(x.category, ExtrasCategory::Themes);
    // A game executable PKG (0x05) and a game data PKG (0x04) stay games.
    for ct in [0x04, 0x05, 0x15, 0x1F] {
        let other = pkg_with_type(1, ct, "UP0001-BLUS30001_00-0000000000000001");
        assert_eq!(sniff("game.pkg", &other), None, "{ct:#x}");
    }
    // A theme PKG without a readable title id is still a theme.
    let x = sniff("t.pkg", &pkg_with_type(1, 0x09, "garbage")).unwrap();
    assert_eq!(x.format, ExtraFormat::Ps3ThemePkg);
    assert_eq!(x.title_id, None);
}

#[test]
fn vita_pkg_theme_0x1f() {
    let vita = pkg_with_type(2, 0x1F, "EP0001-PCSB00001_00-THEME0000000001");
    let x = sniff("vita.pkg", &vita).unwrap();
    assert_eq!(x.format, ExtraFormat::VitaThemePkg);
    assert_eq!(x.category, ExtrasCategory::Themes);
    assert_eq!(x.title_id.as_deref(), Some("PCSB00001"));
    assert_eq!(x.content_type, Some(0x1F));
    // The PS3 theme values are not Vita themes, and 0x1F is no PS3 type.
    assert_eq!(
        sniff("v.pkg", &pkg_with_type(2, 0x09, "EP0001-PCSB00001_00-X")),
        None
    );
    assert_eq!(
        sniff("v.pkg", &pkg_with_type(2, 0x15, "EP0001-PCSB00001_00-X")),
        None
    );
}

#[test]
fn truncated_or_out_of_range_metadata_tables_are_none() {
    let cid = "UP0001-BLUS30001_00-THEME0000000001";
    // No metadata table at all (the console readers' PKG fixture).
    assert_eq!(sniff("t.pkg", &pkg(1, cid).bytes()), None);
    // More than 64 entries.
    let many: Vec<(u32, Vec<u8>)> = (0..65).map(|i| (100 + i, vec![0u8; 4])).collect();
    let mut b = pkg_with_entries(1, cid, &many);
    assert_eq!(sniff("t.pkg", &b), None);
    // The content type entry is past the first 4 KiB of the table.
    let mut far = vec![(7u32, vec![0u8; 4096])];
    far.push((2, 0x09u32.to_be_bytes().to_vec()));
    assert_eq!(sniff("t.pkg", &pkg_with_entries(1, cid, &far)), None);
    // An entry whose size runs past the file.
    let mut big = pkg_with_entries(1, cid, &[(2, 0x09u32.to_be_bytes().to_vec())]);
    big[0xC4..0xC8].copy_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    assert_eq!(sniff("t.pkg", &big), None);
    // A content type entry of the wrong size.
    let short = pkg_with_entries(1, cid, &[(2, vec![0x09])]);
    assert_eq!(sniff("t.pkg", &short), None);
    // A metadata offset past the end of the file.
    b = pkg_with_type(1, 0x09, cid);
    b[8..12].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
    assert_eq!(sniff("t.pkg", &b), None);
    // Truncated inside the table.
    let whole = pkg_with_type(1, 0x09, cid);
    assert_eq!(sniff("t.pkg", &whole[..0xC0 + 12]), None);
}

#[test]
fn psp_ptf_by_extension_only_when_no_magic_claims_it() {
    let x = sniff("Sunset.PTF", &[0x42; 0x80]).unwrap();
    assert_eq!(x.format, ExtraFormat::PspPtf);
    assert_eq!(x.category, ExtrasCategory::Themes);
    assert_eq!(x.display_name.as_deref(), Some("Sunset"));
    // Another extension is not a PTF.
    assert_eq!(sniff("Sunset.bin", &[0x42; 0x80]), None);
    // A PKG of a game, a 3DS image or an STFS game under `.ptf` is not a
    // theme: a console magic claims it first.
    let game_pkg = pkg_with_type(1, 0x05, "UP0001-BLUS30001_00-0000000000000001");
    assert_eq!(sniff("x.ptf", &game_pkg), None);
    assert_eq!(sniff("x.ptf", &stfs_extra(0x000D_0000, 0x5841_0889)), None);
    let mut ncsd = vec![0u8; 0x200];
    ncsd[0x100..0x104].copy_from_slice(b"NCSD");
    assert_eq!(sniff("x.ptf", &ncsd), None);
    // An empty file is nothing.
    assert_eq!(sniff("x.ptf", &[]), None);
}

#[test]
fn x360_stfs_extras_by_content_type() {
    use ExtraFormat::{X360Avatar, X360Gamerpic, X360Theme, X360Video};
    use ExtrasCategory::{Themes, Videos};
    for (ct, f, cat) in [
        (0x0003_0000, X360Theme, Themes),
        (0x0002_0000, X360Gamerpic, Themes),
        (0x0000_9000, X360Avatar, Themes),
        (0x0009_0000, X360Video, Videos),
        (0x000C_0000, X360Video, Videos),
        (0x0010_0000, X360Video, Videos),
        (0x0030_0000, X360Video, Videos),
        (0x0040_0000, X360Video, Videos),
    ] {
        let x = sniff("0123456789ABCDEF", &stfs_extra(ct, 0x4D53_0001))
            .unwrap_or_else(|| panic!("{ct:#010x}"));
        assert_eq!(x.format, f, "{ct:#010x}");
        assert_eq!(x.category, cat, "{ct:#010x}");
        assert_eq!(x.title_id.as_deref(), Some("4D530001"), "{ct:#010x}");
        assert_eq!(x.content_type, Some(ct));
        assert_eq!(x.display_name.as_deref(), Some("Test Arcade Game"));
    }
    // The licence store, games, updates, DLC and demos are never extras.
    for ct in [
        0x000F_0000,
        0x0000_7000,
        0x000D_0000,
        0x000B_0000,
        0x0000_0002,
        0x0008_0000,
    ] {
        assert_eq!(sniff("x", &stfs_extra(ct, 0x4D53_0001)), None, "{ct:#010x}");
    }
    // A header too short for the content type is nothing.
    assert_eq!(sniff("x", &stfs_extra(0x0003_0000, 1)[..0x300]), None);
}

#[test]
fn n3ds_soundtrack_cia_only_with_a_whole_word() {
    let plain = cia_titled(APP, None);
    let x = sniff("Zelda Original Soundtrack.cia", &plain).unwrap();
    assert_eq!(x.format, ExtraFormat::N3dsSoundtrackCia);
    assert_eq!(x.category, ExtrasCategory::Soundtrack);
    assert_eq!(x.title_id.as_deref(), Some("0004000000123400"));
    assert!(sniff("Pokémon OST (EUR).cia", &plain).is_some());
    // NFD «é» in the name still splits into the same words.
    assert!(sniff("Poke\u{301}mon OST.cia", &plain).is_some());
    assert!(sniff("Mario Sound Track.cia", &plain).is_some());
    assert!(sniff("Zelda banda SONORA.cia", &plain).is_some());
    assert!(sniff("ゼルダの伝説サウンドトラック.cia", &plain).is_some());
    // No whole word: a game.
    for name in [
        "Hostage.cia",
        "Video Poker.cia",
        "Soundtracker.cia",
        "Ghost.cia",
        "Trailer Park.cia",
    ] {
        assert_eq!(sniff(name, &plain), None, "{name}");
    }
    // The SMDH title counts too, and gives the display name.
    let titled = cia_titled(APP, Some("Kid Icarus Soundtrack"));
    let x = sniff("0004000000123400.cia", &titled).unwrap();
    assert_eq!(x.display_name.as_deref(), Some("Kid Icarus Soundtrack"));
    let game = cia_titled(APP, Some("Kid Icarus"));
    assert_eq!(sniff("0004000000123400.cia", &game), None);
    // Only an application: DLC, an update or a demo named so is not one.
    for high in [0x0004_008C_u64, 0x0004_000E, 0x0004_0002] {
        let other = cia_titled((high << 32) | 0x0012_3400, None);
        assert_eq!(sniff("OST.cia", &other), None, "{high:08X}");
    }
    // Not a CIA at all, or a soundtrack word under another extension.
    assert_eq!(sniff("OST.cia", &[0u8; 0x400]), None);
    assert_eq!(sniff("OST.3ds", &plain), None);
}

#[test]
fn key_material_is_never_opened() {
    let dir = tempfile::tempdir().unwrap();
    let theme = pkg_with_type(1, 0x09, "UP0001-BLUS30001_00-THEME0000000001");
    let keys: Vec<_> = ["act.dat", "x.rap", "prod.keys", "title.tik"]
        .into_iter()
        .map(|n| {
            let p = dir.path().join(n);
            std::fs::write(&p, &theme).unwrap();
            p
        })
        .collect();
    let ok = dir.path().join("theme.pkg");
    std::fs::write(&ok, &theme).unwrap();
    consoles::start_open_log();
    for k in &keys {
        assert_eq!(sniff_path(k), None, "{k:?}");
    }
    assert_eq!(sniff_path(&ok).unwrap().format, ExtraFormat::Ps3ThemePkg);
    let opened = consoles::take_open_log();
    for k in &keys {
        assert!(!opened.contains(k), "{opened:?}");
    }
    assert!(opened.contains(&ok), "{opened:?}");
    // Bytes in memory under a key name are refused too.
    assert_eq!(sniff("x.rap", &theme), None);
    // A folder or a missing file: nothing.
    assert_eq!(sniff_path(dir.path()), None);
    assert_eq!(sniff_path(&dir.path().join("missing.pkg")), None);
}

fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn sweep(name: &str, bytes: &[u8], lengths: impl Iterator<Item = usize>) {
    for n in lengths {
        let _ = sniff(name, &bytes[..n]);
    }
    let mut rng = 0x5eed_u64;
    for _ in 0..2000 {
        let mut b = bytes.to_vec();
        rng = xorshift(rng);
        let i = (rng as usize) % b.len();
        b[i] ^= (rng >> 32) as u8 | 1;
        let _ = sniff(name, &b);
    }
}

#[test]
fn no_panic_on_truncations_and_byte_flips() {
    for (name, bytes) in all_official() {
        assert!(sniff(name, &bytes).is_some(), "{name}");
        sweep(name, &bytes, 0..bytes.len());
    }
    // The CIA is over 1 MiB of mostly zeros: every length through the
    // header, ticket and TMD, then every 4 KiB, then the whole meta section.
    let cia = cia_titled(APP, Some("Kid Icarus Soundtrack"));
    assert!(sniff("a.cia", &cia).is_some());
    let meta = cia.len() - 0x3AC0;
    sweep(
        "a.cia",
        &cia,
        (0..0x4000)
            .chain((0x4000..meta).step_by(4096))
            .chain(meta..cia.len()),
    );
}

#[test]
fn a_vita_theme_pkg_is_no_longer_a_vita_game() {
    let probe = |bytes: &[u8]| {
        let mut src = SliceAt(bytes);
        consoles::identify(
            &mut Budgeted::new(&mut src, bytes.len() as u64),
            "pkg",
            "x.pkg",
        )
    };
    // A theme PKG carries a game's title id, so the game probe reads it
    // as a Vita game; the official sniff, which runs first, claims it.
    let theme = pkg_with_type(2, 0x1F, "EP0001-PCSB00001_00-THEME0000000001");
    assert_eq!(probe(&theme).unwrap().system, "psvita");
    assert_eq!(
        sniff("x.pkg", &theme).unwrap().format,
        ExtraFormat::VitaThemePkg
    );
    // A Vita game PKG (0x15) is no extra and the game probe is unchanged.
    let game = pkg_with_type(2, 0x15, "EP0001-PCSB00001_00-0000000000000000");
    assert_eq!(sniff("x.pkg", &game), None);
    let id = probe(&game).unwrap();
    assert_eq!(
        (id.system.as_str(), id.title_id.as_deref()),
        ("psvita", Some("PCSB00001"))
    );
}
