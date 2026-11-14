//! Music rip tags (extras spec D15): SPC ID666, NSF/NSFe, GBS, VGM/VGZ
//! GD3 and the PSF family `[TAG]` block, bounded and never panicking.

use coverly_core::extras::rips::{read_rip, read_rip_bytes, VGZ_MAX_OUTPUT};
use coverly_core::extras::testing::{
    all_rips, gbs, gzip, nsf, nsfe, psf, psf_raw, spc, spc_bytes, vgm, vgm_padded, vgz,
};
use coverly_core::extras::{ExtraFormat, RipTags};

fn tags(name: &str, bytes: &[u8]) -> RipTags {
    read_rip_bytes(name, bytes)
        .unwrap_or_else(|| panic!("{name}: no tags"))
        .1
}

#[test]
fn spc_id666_text_tags() {
    let (format, t) = read_rip_bytes(
        "a.spc",
        &spc("Boss", "Super Metroid", "Kenji Yamamoto", true),
    )
    .unwrap();
    assert_eq!(format, ExtraFormat::Spc);
    assert_eq!(t.game.as_deref(), Some("Super Metroid"));
    assert_eq!(t.title.as_deref(), Some("Boss"));
    assert_eq!(t.artist.as_deref(), Some("Kenji Yamamoto"));
    assert_eq!(t.systems, ["snes"]);
}

#[test]
fn spc_without_tags_gives_nothing() {
    let bytes = spc("Boss", "Super Metroid", "Kenji Yamamoto", false);
    assert_eq!(read_rip_bytes("a.spc", &bytes), None);
    // Tags present but every field empty: nothing either.
    assert_eq!(read_rip_bytes("a.spc", &spc("", "", "", true)), None);
    // Not an SPC at all.
    assert_eq!(read_rip_bytes("a.spc", &[0u8; 0x100]), None);
}

#[test]
fn nsf_and_nsfe_names() {
    let t = tags("a.nsf", &nsf("Mega Man 2", "Takashi Tateishi"));
    assert_eq!(t.game.as_deref(), Some("Mega Man 2"));
    assert_eq!(t.artist.as_deref(), Some("Takashi Tateishi"));
    assert_eq!(t.systems, ["nes", "famicom"]);
    let (format, t) = read_rip_bytes(
        "a.nsfe",
        &nsfe(&["Mega Man 2", "Takashi Tateishi", "1988 Capcom", "ripper"]),
    )
    .unwrap();
    assert_eq!(format, ExtraFormat::Nsf);
    assert_eq!(t.game.as_deref(), Some("Mega Man 2"));
    assert_eq!(t.artist.as_deref(), Some("Takashi Tateishi"));
    assert_eq!(t.systems, ["nes", "famicom"]);
    // An NSFe named `.nsf` is still read by its magic.
    assert!(read_rip_bytes("a.nsf", &nsfe(&["Kirby", "x"])).is_some());
    // No `auth` chunk: nothing.
    assert_eq!(read_rip_bytes("a.nsfe", &nsfe(&[])), None);
}

#[test]
fn nsfe_chunk_walk_is_bounded() {
    // 65 empty chunks before `auth`: past the 64-chunk cap, so not read.
    let mut b = b"NSFE".to_vec();
    for _ in 0..65 {
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(b"text");
    }
    b.extend_from_slice(&6u32.to_le_bytes());
    b.extend_from_slice(b"auth");
    b.extend_from_slice(b"Game\0\0");
    assert_eq!(read_rip_bytes("a.nsfe", &b), None);
    // A chunk claiming 4 GiB ends the walk, never allocates.
    let mut b = b"NSFE".to_vec();
    b.extend_from_slice(&u32::MAX.to_le_bytes());
    b.extend_from_slice(b"auth");
    b.extend_from_slice(b"Game\0");
    assert_eq!(read_rip_bytes("a.nsfe", &b), None);
}

#[test]
fn gbs_title() {
    let (format, t) = read_rip_bytes("a.gbs", &gbs("Tetris", "Hirokazu Tanaka")).unwrap();
    assert_eq!(format, ExtraFormat::Gbs);
    assert_eq!(t.game.as_deref(), Some("Tetris"));
    assert_eq!(t.artist.as_deref(), Some("Hirokazu Tanaka"));
    assert_eq!(t.systems, ["gb", "gbc"]);
}

#[test]
fn vgm_gd3_utf16_and_system_hint() {
    let (format, t) = read_rip_bytes(
        "a.vgm",
        &vgm(
            "Green Hill Zone",
            "Sonic the Hedgehog",
            "Sega Mega Drive / Genesis",
            "Masato Nakamura",
        ),
    )
    .unwrap();
    assert_eq!(format, ExtraFormat::Vgm);
    assert_eq!(t.title.as_deref(), Some("Green Hill Zone"));
    assert_eq!(t.game.as_deref(), Some("Sonic the Hedgehog"));
    assert_eq!(t.artist.as_deref(), Some("Masato Nakamura"));
    assert_eq!(t.systems, ["megadrive", "genesis"]);
    let sys = |s: &str| tags("a.vgm", &vgm("t", "g", s, "a")).systems;
    assert_eq!(sys("Sega Master System"), ["mastersystem"]);
    assert_eq!(sys("Sega Game Gear"), ["gamegear"]);
    assert_eq!(sys("NEC PC Engine / TurboGrafx-16"), ["pcengine"]);
    assert_eq!(sys("Nintendo Entertainment System"), ["nes", "famicom"]);
    assert!(sys("Arcade Machine").is_empty());
    assert!(sys("").is_empty());
    // Japanese text survives the UTF-16 decoding.
    let t = tags("a.vgm", &vgm("グリーンヒル", "ソニック", "", "中村"));
    assert_eq!(t.game.as_deref(), Some("ソニック"));
}

#[test]
fn vgz_is_gunzipped_with_a_cap() {
    let (format, t) =
        read_rip_bytes("a.vgz", &vgz("Green Hill", "Sonic", "Genesis", "M. N.")).unwrap();
    assert_eq!(format, ExtraFormat::Vgm);
    assert_eq!(t.game.as_deref(), Some("Sonic"));
    // A gzipped `.vgm` is read too.
    assert!(read_rip_bytes("a.vgm", &vgz("t", "Sonic", "", "")).is_some());
    // 17 MiB of output: past the cap, so the tags are skipped.
    let big = vgm_padded("t", "Sonic", "Genesis", "a", 17 * 1024 * 1024);
    assert!(big.len() as u64 > VGZ_MAX_OUTPUT);
    assert_eq!(read_rip_bytes("a.vgz", &gzip(&big)), None);
    // Just under it: read.
    let fits = vgm_padded("t", "Sonic", "Genesis", "a", 1024 * 1024);
    assert!(read_rip_bytes("a.vgz", &gzip(&fits)).is_some());
}

#[test]
fn psf_tag_block_utf8_and_shift_jis() {
    let (format, t) = read_rip_bytes(
        "a.psf",
        &psf(
            1,
            &[
                ("game", "Final Fantasy VII"),
                ("title", "Prelude"),
                ("artist", "Nobuo Uematsu"),
            ],
            true,
        ),
    )
    .unwrap();
    assert_eq!(format, ExtraFormat::Psf);
    assert_eq!(t.game.as_deref(), Some("Final Fantasy VII"));
    assert_eq!(t.title.as_deref(), Some("Prelude"));
    assert_eq!(t.artist.as_deref(), Some("Nobuo Uematsu"));
    assert_eq!(t.systems, ["psx"]);
    // `game=ファイナルファンタジー` in Shift-JIS, no `utf8=1`.
    let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("game=ファイナルファンタジー\n");
    let t = tags("a.minipsf", &psf_raw(1, &sjis));
    assert_eq!(t.game.as_deref(), Some("ファイナルファンタジー"));
    // Keys are case-insensitive and values trimmed.
    let t = tags("a.psf2", &psf_raw(2, b"  GAME = Okami \r\n"));
    assert_eq!(t.game.as_deref(), Some("Okami"));
    assert_eq!(t.systems, ["ps2"]);
    // No `[TAG]`: nothing.
    let mut b = psf(1, &[("game", "x")], true);
    b.truncate(b.len() - 19);
    assert_eq!(read_rip_bytes("a.psf", &b), None);
}

#[test]
fn psf_family_systems() {
    let sys = |name: &str, v: u8| tags(name, &psf(v, &[("game", "G")], true)).systems;
    assert_eq!(sys("a.psf", 2), ["ps2"]);
    assert_eq!(sys("a.gsf", 0x22), ["gba"]);
    assert_eq!(sys("a.miniusf", 0x21), ["n64"]);
    assert_eq!(sys("a.mini2sf", 0x24), ["nds"]);
    assert_eq!(sys("a.ssf", 0x11), ["saturn"]);
    assert_eq!(sys("a.dsf", 0x12), ["dreamcast"]);
    assert!(sys("a.qsf", 0x41).is_empty());
}

#[test]
fn psf_tag_block_is_capped() {
    let mut text = vec![b' '; 50_000];
    text.extend_from_slice(b"\ngame=Late\n");
    assert_eq!(read_rip_bytes("a.psf", &psf_raw(1, &text)), None);
}

#[test]
fn latin1_fallback_for_spc() {
    let t = tags("a.spc", &spc_bytes(b"Th\xe8me", b"Pok\xe9mon", b"", true));
    assert_eq!(t.game.as_deref(), Some("Pokémon"));
    assert_eq!(t.title.as_deref(), Some("Thème"));
    assert_eq!(t.artist, None);
    // Valid UTF-8 stays UTF-8.
    let t = tags("a.spc", &spc("t", "Pokémon", "", true));
    assert_eq!(t.game.as_deref(), Some("Pokémon"));
}

#[test]
fn other_extensions_are_never_read() {
    assert_eq!(read_rip_bytes("a.mp3", &spc("a", "b", "c", true)), None);
    assert_eq!(read_rip_bytes("a.rsn", &spc("a", "b", "c", true)), None);
    assert_eq!(read_rip_bytes("noext", &spc("a", "b", "c", true)), None);
}

fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

#[test]
fn no_panic_on_truncations_and_byte_flips() {
    for (name, bytes) in all_rips() {
        assert!(read_rip_bytes(name, &bytes).is_some(), "{name}");
        for n in 0..bytes.len() {
            let _ = read_rip_bytes(name, &bytes[..n]);
        }
        let mut rng = 0x5eed_u64;
        for _ in 0..2000 {
            let mut b = bytes.clone();
            rng = xorshift(rng);
            let i = (rng as usize) % b.len();
            b[i] ^= (rng >> 32) as u8 | 1;
            let _ = read_rip_bytes(name, &b);
        }
    }
}

#[test]
fn read_rip_reads_files_and_never_opens_key_material() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in all_rips() {
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read_rip(&path), read_rip_bytes(name, &bytes), "{name}");
    }
    // A big VGM with its GD3 far into the file is read by seeking.
    let far = vgm_padded("t", "Sonic", "Genesis", "a", 3 * 1024 * 1024);
    let path = dir.path().join("far.vgm");
    std::fs::write(&path, &far).unwrap();
    assert_eq!(read_rip(&path).unwrap().1.game.as_deref(), Some("Sonic"));
    // A folder or a missing file: nothing.
    assert_eq!(read_rip(dir.path()), None);
    assert_eq!(read_rip(&dir.path().join("missing.spc")), None);
    // A key file is refused by name before it is opened.
    let key = dir.path().join("prod.keys");
    std::fs::write(&key, b"x").unwrap();
    coverly_core::consoles::start_open_log();
    assert_eq!(read_rip(&key), None);
    let other = dir.path().join("a.mp3");
    std::fs::write(&other, b"x").unwrap();
    assert_eq!(read_rip(&other), None);
    let opened = coverly_core::consoles::take_open_log();
    assert!(!opened.contains(&key), "{opened:?}");
    assert!(!opened.contains(&other), "{opened:?}");
}

#[test]
fn nsfe_auth_after_a_large_data_chunk_is_read() {
    // Tools write INFO, DATA, then `auth`: a DATA chunk past 64 KiB is
    // skipped by its length, never read, so the tags still come through.
    let mut b = b"NSFE".to_vec();
    let mut push = |id: &[u8], data: &[u8]| {
        b.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(data);
    };
    push(b"INFO", &[0u8; 10]);
    push(b"DATA", &vec![0xEAu8; 200 * 1024]);
    push(b"auth", b"Kirby's Adventure\0Hirokazu Ando\0\0\0");
    push(b"NEND", &[]);
    let t = tags("a.nsfe", &b);
    assert_eq!(t.game.as_deref(), Some("Kirby's Adventure"));
    assert_eq!(t.artist.as_deref(), Some("Hirokazu Ando"));
    // An `auth` chunk itself past 64 KiB is never read.
    let mut b = b"NSFE".to_vec();
    let mut auth = b"Game\0".to_vec();
    auth.resize(64 * 1024 + 1, b'x');
    b.extend_from_slice(&u32::try_from(auth.len()).unwrap().to_le_bytes());
    b.extend_from_slice(b"auth");
    b.extend_from_slice(&auth);
    assert_eq!(read_rip_bytes("a.nsfe", &b), None);
}

#[test]
fn vgm_system_text_links_only_the_consoles_it_names() {
    let sys = |s: &str| tags("a.vgm", &vgm("t", "g", s, "a")).systems;
    // A Super Famicom is not a Famicom.
    assert!(!sys("Nintendo Super Famicom").contains(&"nes"));
    assert!(!sys("Super Nintendo Entertainment System").contains(&"nes"));
    assert_eq!(sys("Nintendo Family Computer"), ["nes", "famicom"]);
    assert_eq!(sys("Famicom Disk System"), ["fds", "nes", "famicom"]);
    // The colour handhelds and later models are linked too.
    assert_eq!(sys("SNK Neo Geo Pocket Color"), ["ngpc", "ngp"]);
    assert_eq!(sys("SNK Neo Geo Pocket"), ["ngp", "ngpc"]);
    assert_eq!(
        sys("Bandai WonderSwan Color"),
        ["wonderswancolor", "wonderswan"]
    );
    assert_eq!(sys("Bandai WonderSwan"), ["wonderswan", "wonderswancolor"]);
    assert_eq!(sys("MSX2"), ["msx2", "msx"]);
    assert_eq!(sys("MSX"), ["msx", "msx2"]);
    assert_eq!(sys("NEC PC Engine CD-ROM²"), ["pcenginecd", "pcengine"]);
    assert_eq!(sys("Sega Mega Drive / Genesis"), ["megadrive", "genesis"]);
}

#[test]
fn vgz_compressed_input_is_capped_too() {
    // 18 MiB of empty stored deflate blocks before the real data: no
    // output for all that input, so the input is capped as well.
    let small = gzip(&vgm("t", "Sonic", "Genesis", "a"));
    assert!(read_rip_bytes("a.vgz", &small).is_some());
    let mut padded = small[..10].to_vec();
    for _ in 0..(18 * 1024 * 1024 / 5) {
        padded.extend_from_slice(&[0x00, 0x00, 0x00, 0xFF, 0xFF]);
    }
    padded.extend_from_slice(&small[10..]);
    assert_eq!(read_rip_bytes("a.vgz", &padded), None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("padded.vgz");
    std::fs::write(&path, &padded).unwrap();
    assert_eq!(read_rip(&path), None);
}
