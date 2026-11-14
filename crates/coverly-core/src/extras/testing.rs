//! Synthetic byte builders for extras tests.

use std::io::Write;

/// `text` into a fixed field of `len` bytes, NUL padded.
fn field(out: &mut [u8], offset: usize, len: usize, text: &[u8]) {
    let n = text.len().min(len);
    out[offset..offset + n].copy_from_slice(&text[..n]);
}

/// An SPC header (0x100 bytes) with ID666 text tags: `song` at 0x2E,
/// `game` at 0x4E, `artist` at 0xB1; byte 0x23 is 26 when `tags_present`,
/// 27 otherwise. Strings are written as given (bytes, not re-encoded).
pub fn spc_bytes(song: &[u8], game: &[u8], artist: &[u8], tags_present: bool) -> Vec<u8> {
    let mut b = vec![0u8; 0x100];
    field(&mut b, 0, 33, b"SNES-SPC700 Sound File Data v0.30");
    b[0x21] = 26;
    b[0x22] = 26;
    b[0x23] = if tags_present { 26 } else { 27 };
    b[0x24] = 30;
    field(&mut b, 0x2E, 32, song);
    field(&mut b, 0x4E, 32, game);
    field(&mut b, 0xB1, 32, artist);
    b
}

/// [`spc_bytes`] from UTF-8 text.
pub fn spc(song: &str, game: &str, artist: &str, tags_present: bool) -> Vec<u8> {
    spc_bytes(
        song.as_bytes(),
        game.as_bytes(),
        artist.as_bytes(),
        tags_present,
    )
}

/// An NSF header (0x80 bytes): `name` at 0x0E, `artist` at 0x2E.
pub fn nsf(name: &str, artist: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x80];
    field(&mut b, 0, 5, b"NESM\x1A");
    b[5] = 1;
    b[6] = 3;
    b[7] = 1;
    field(&mut b, 0x0E, 32, name.as_bytes());
    field(&mut b, 0x2E, 32, artist.as_bytes());
    b
}

/// An NSFe file: `NSFE`, an `INFO` chunk, an `auth` chunk holding `auth`
/// as NUL-terminated strings (game, artist, copyright, ripper), `NEND`.
pub fn nsfe(auth: &[&str]) -> Vec<u8> {
    let mut b = b"NSFE".to_vec();
    let chunk = |b: &mut Vec<u8>, id: &[u8; 4], data: &[u8]| {
        b.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(data);
    };
    chunk(&mut b, b"INFO", &[0u8; 10]);
    let mut data = Vec::new();
    for s in auth {
        data.extend_from_slice(s.as_bytes());
        data.push(0);
    }
    chunk(&mut b, b"auth", &data);
    chunk(&mut b, b"NEND", &[]);
    b
}

/// A GBS header (0x70 bytes): `title` at 0x10, `author` at 0x30.
pub fn gbs(title: &str, author: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x70];
    field(&mut b, 0, 3, b"GBS");
    b[3] = 1;
    b[4] = 1;
    b[5] = 1;
    field(&mut b, 0x10, 32, title.as_bytes());
    field(&mut b, 0x30, 32, author.as_bytes());
    b
}

/// A VGM file: a 0x40-byte header, a few data bytes and a GD3 block
/// (UTF-16LE: track, track JP, game, game JP, system, system JP, author,
/// author JP, date, ripper, notes) whose offset sits at 0x14.
pub fn vgm(track: &str, game: &str, system: &str, author: &str) -> Vec<u8> {
    vgm_padded(track, game, system, author, 16)
}

/// [`vgm`] with `data` bytes of sound data before the GD3 block.
pub fn vgm_padded(track: &str, game: &str, system: &str, author: &str, data: usize) -> Vec<u8> {
    let mut b = vec![0u8; 0x40];
    field(&mut b, 0, 4, b"Vgm ");
    b[8..12].copy_from_slice(&0x0000_0150_u32.to_le_bytes());
    b.extend(std::iter::repeat_n(0x66u8, data));
    let gd3_at = b.len();
    let mut strings = Vec::new();
    for s in [track, "", game, "", system, "", author, "", "", "", ""] {
        for u in s.encode_utf16() {
            strings.extend_from_slice(&u.to_le_bytes());
        }
        strings.extend_from_slice(&[0, 0]);
    }
    b.extend_from_slice(b"Gd3 ");
    b.extend_from_slice(&0x0000_0100_u32.to_le_bytes());
    b.extend_from_slice(&u32::try_from(strings.len()).unwrap().to_le_bytes());
    b.extend_from_slice(&strings);
    let eof = u32::try_from(b.len() - 4).unwrap();
    b[4..8].copy_from_slice(&eof.to_le_bytes());
    let rel = u32::try_from(gd3_at - 0x14).unwrap();
    b[0x14..0x18].copy_from_slice(&rel.to_le_bytes());
    b
}

/// `bytes` gzip-compressed, as a `.vgz` is.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

/// A `.vgz`: [`vgm`] gzip-compressed.
pub fn vgz(track: &str, game: &str, system: &str, author: &str) -> Vec<u8> {
    gzip(&vgm(track, game, system, author))
}

/// A PSF-family file: `PSF`, `version`, reserved and program sizes,
/// a few reserved and program bytes, then `[TAG]` with `key=value`
/// lines (`utf8=1` first when `utf8`). Values that are not UTF-8 text
/// go through [`psf_raw`].
pub fn psf(version: u8, tags: &[(&str, &str)], utf8: bool) -> Vec<u8> {
    let mut text = Vec::new();
    if utf8 {
        text.extend_from_slice(b"utf8=1\n");
    }
    for (k, v) in tags {
        text.extend_from_slice(format!("{k}={v}\n").as_bytes());
    }
    psf_raw(version, &text)
}

/// A PSF-family file whose tag block is `tag_text` as given.
pub fn psf_raw(version: u8, tag_text: &[u8]) -> Vec<u8> {
    let reserved = 8u32;
    let program = 24u32;
    let mut b = b"PSF".to_vec();
    b.push(version);
    b.extend_from_slice(&reserved.to_le_bytes());
    b.extend_from_slice(&program.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend(std::iter::repeat_n(0xAAu8, (reserved + program) as usize));
    b.extend_from_slice(b"[TAG]");
    b.extend_from_slice(tag_text);
    b
}

/// One file of every rip format with tags, named by its extension.
pub fn all_rips() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "a.spc",
            spc("Boss", "Super Metroid", "Kenji Yamamoto", true),
        ),
        ("a.nsf", nsf("Mega Man 2", "Takashi Tateishi")),
        (
            "a.nsfe",
            nsfe(&["Mega Man 2", "Takashi Tateishi", "1988 Capcom", "x"]),
        ),
        ("a.gbs", gbs("Tetris", "Hirokazu Tanaka")),
        (
            "a.vgm",
            vgm(
                "Green Hill",
                "Sonic",
                "Sega Mega Drive / Genesis",
                "Masato Nakamura",
            ),
        ),
        (
            "a.vgz",
            vgz(
                "Green Hill",
                "Sonic",
                "Sega Mega Drive / Genesis",
                "Masato Nakamura",
            ),
        ),
        (
            "a.psf",
            psf(
                1,
                &[
                    ("game", "Final Fantasy VII"),
                    ("title", "Prelude"),
                    ("artist", "Nobuo Uematsu"),
                ],
                true,
            ),
        ),
        (
            "a.minigsf",
            psf(
                0x22,
                &[("game", "Golden Sun"), ("artist", "Motoi Sakuraba")],
                false,
            ),
        ),
    ]
}

/// A PS3 theme `.p3t`: the `P3TF` magic, a version, then a plaintext
/// theme XML whose `infotable` names the theme `name`.
pub fn p3t(name: &str) -> Vec<u8> {
    let mut b = b"P3TF".to_vec();
    b.extend_from_slice(&0x0101_0000u32.to_be_bytes());
    b.extend_from_slice(&[0u8; 0x38]);
    b.extend_from_slice(
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><theme format-version=\"1.1\">\
             <infotable><name>{name}</name><author>Test</author></infotable></theme>"
        )
        .as_bytes(),
    );
    b.extend_from_slice(&[0xE7; 0x100]);
    b
}

/// The offset [`pkg_with_type`] puts its metadata table at (psdevwiki
/// «PKG files»: retail packages put it right after the 0xC0 header).
pub const PKG_METADATA_AT: usize = 0xC0;

/// A PS3/Vita PKG (`7F PKG`, revision, `kind` at 0x06, metadata offset
/// at 0x08, metadata count at 0x0C, content id at 0x30) with a plaintext
/// metadata table of three entries: DRM type (id 1), content type (id 2,
/// `ctype`) and package flags (id 3); then filler for the body.
pub fn pkg_with_type(kind: u16, ctype: u32, content_id: &str) -> Vec<u8> {
    pkg_with_entries(
        kind,
        content_id,
        &[
            (1, 3u32.to_be_bytes().to_vec()),
            (2, ctype.to_be_bytes().to_vec()),
            (3, 0x0E_u32.to_be_bytes().to_vec()),
        ],
    )
}

/// A [`pkg_with_type`] whose metadata table holds `entries` as given
/// (`(id, data)`), for the bounds tests.
pub fn pkg_with_entries(kind: u16, content_id: &str, entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0u8; PKG_METADATA_AT];
    b[..4].copy_from_slice(&[0x7F, b'P', b'K', b'G']);
    b[4..6].copy_from_slice(&0x8000u16.to_be_bytes());
    b[6..8].copy_from_slice(&kind.to_be_bytes());
    b[8..12].copy_from_slice(&u32::try_from(PKG_METADATA_AT).unwrap().to_be_bytes());
    b[12..16].copy_from_slice(&u32::try_from(entries.len()).unwrap().to_be_bytes());
    let n = content_id.len().min(0x24);
    b[0x30..0x30 + n].copy_from_slice(&content_id.as_bytes()[..n]);
    let mut table = Vec::new();
    for (id, data) in entries {
        table.extend_from_slice(&id.to_be_bytes());
        table.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        table.extend_from_slice(data);
    }
    b[0x10..0x14].copy_from_slice(&u32::try_from(table.len()).unwrap().to_be_bytes());
    b.extend_from_slice(&table);
    b.extend_from_slice(&[0xE7; 0x200]);
    b
}

/// An Xbox 360 STFS `LIVE` package of content type `ctype` for title id
/// `tid` (display name «Test Arcade Game»), no thumbnail.
pub fn stfs_extra(ctype: u32, tid: u32) -> Vec<u8> {
    crate::consoles::testing::stfs(b"LIVE", tid, ctype, &[])
}

/// A CIA (clear content 0) of title id `tid` whose meta section carries
/// an SMDH titled `title`, or no meta section when `title` is `None`.
pub fn cia_titled(tid: u64, title: Option<&str>) -> Vec<u8> {
    let mut s = crate::consoles::testing::cia(tid, false, title.is_some());
    if let Some(title) = title {
        let at = s.len - crate::consoles::ctr::SMDH_LEN as u64;
        s.put(at, &crate::consoles::testing::smdh(title));
    }
    s.bytes()
}

/// Every official extra fixture, named as the console stores it.
pub fn all_official() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("Theme.p3t", p3t("Tidewalker")),
        (
            "theme.pkg",
            pkg_with_type(1, 0x09, "UP0001-BLUS30001_00-THEME0000000001"),
        ),
        (
            "avatar.pkg",
            pkg_with_type(1, 0x0D, "UP0001-BLUS30001_00-AVATAR000000001"),
        ),
        (
            "vita.pkg",
            pkg_with_type(2, 0x1F, "EP0001-PCSB00001_00-THEME0000000001"),
        ),
        ("Theme.PTF", vec![0x42; 0x80]),
        (
            "0123456789ABCDEF0123456789ABCDEF01234567",
            stfs_extra(0x0003_0000, 0x4D53_0001),
        ),
    ]
}
