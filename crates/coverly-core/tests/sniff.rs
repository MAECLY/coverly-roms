//! Header magic per system (spec §6.3). Fixtures hold only the bytes a
//! check reads; everything else is zero.

use coverly_core::identify::sniff::fixtures as fx;
use std::io::{self, Read, Write};

use coverly_core::identify::sniff::{
    sniff, ContainerMagic, FileAt, NotARom, ReadAt, SliceAt, Sniff, StreamAt, PREFIX,
};

fn run(bytes: &[u8], ext: &str) -> Sniff {
    sniff(&mut SliceAt(bytes), bytes.len() as u64, ext)
}

fn best(s: &Sniff) -> Option<(String, f32)> {
    s.hits
        .iter()
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
        .map(|e| (e.system.clone(), e.weight))
}

#[test]
fn nes_ines_and_unif() {
    assert_eq!(best(&run(&fx::nes(), "nes")), Some(("nes".into(), 0.95)));
    assert_eq!(best(&run(&fx::unif(), "unf")), Some(("nes".into(), 0.95)));
    let mut near = fx::nes();
    near[3] = 0x1B;
    assert_eq!(best(&run(&near, "nes")), None);
}

#[test]
fn fds_header_and_headerless() {
    assert_eq!(
        best(&run(&fx::fds(true), "fds")),
        Some(("fds".into(), 0.95))
    );
    assert_eq!(
        best(&run(&fx::fds(false), "fds")),
        Some(("fds".into(), 0.95))
    );
}

#[test]
fn n64_in_every_byte_order() {
    for order in [fx::N64Order::Z64, fx::N64Order::V64, fx::N64Order::N64] {
        assert_eq!(
            best(&run(&fx::n64(order), "bin")),
            Some(("n64".into(), 0.95))
        );
    }
}

#[test]
fn lynx_and_7800() {
    assert_eq!(best(&run(&fx::lynx(), "lnx")), Some(("lynx".into(), 0.95)));
    assert_eq!(
        best(&run(&fx::a7800(), "a78")),
        Some(("a7800".into(), 0.95))
    );
}

#[test]
fn web_pages_and_text_are_not_roms() {
    let html = fx::html_page();
    let s = run(&html, "zip");
    assert_eq!(s.not_a_rom, Some(NotARom::WebPage));
    assert!(s.hits.is_empty());
    assert_eq!(
        run(b"# Notes\n\nSome markdown text.\n", "md").not_a_rom,
        Some(NotARom::Text)
    );
    assert_eq!(run(b"{\"a\":1}", "bin").not_a_rom, Some(NotARom::WebPage));
    assert_eq!(
        run(&[0u8; 1024], "bin").not_a_rom,
        None,
        "zeros are not text"
    );
}

#[test]
fn containers_are_recognised_not_read() {
    assert_eq!(
        run(b"PK\x03\x04rest", "zip").container,
        Some(ContainerMagic::Zip)
    );
    assert_eq!(
        run(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4], "7z").container,
        Some(ContainerMagic::SevenZip)
    );
    assert_eq!(
        run(b"MComprHD\0\0\0\x7c", "chd").container,
        Some(ContainerMagic::Chd)
    );
    assert_eq!(
        run(b"Rar!\x1a\x07\x01\x00", "rar").container,
        Some(ContainerMagic::Rar5)
    );
    // Raw code that starts like a weak magic (`60 EA` = 6502 RTS NOP) is
    // not taken for a container, so header identification still runs.
    let mut code = vec![0x60, 0xEA, 0x28, 0x00, 0x1E, 0x0B];
    code.resize(4096, 0xEA);
    assert_eq!(run(&code, "a26").container, None);
}

#[test]
fn empty_and_short_inputs_never_panic() {
    for len in [0usize, 1, 3, 16, 0x103, 0x14F] {
        let s = run(&vec![0u8; len], "gb");
        assert!(s.hits.is_empty(), "len {len}");
    }
}

#[test]
fn text_native_game_formats_are_not_rejected_as_text() {
    // PICO-8 carts, ScummVM launchers, DOSBox configs and BASIC listings
    // are games that happen to be text.
    let p8 = b"pico-8 cartridge // http://www.pico-8.com\nversion 41\n__lua__\nprint(1)\n";
    assert_eq!(run(p8, "p8").not_a_rom, None);
    assert_eq!(run(b"monkey\n", "scummvm").not_a_rom, None);
    assert_eq!(run(b"[sdl]\nfullscreen=true\n", "conf").not_a_rom, None);
    assert_eq!(run(b"10 PRINT \"HI\"\n20 GOTO 10\n", "bas").not_a_rom, None);
    // A web page saved under such a name is still a failed download.
    assert_eq!(
        run(&fx::html_page(), "p8").not_a_rom,
        Some(NotARom::WebPage)
    );
    // The same text under a ROM extension stays "not a ROM".
    assert_eq!(run(p8, "gba").not_a_rom, Some(NotARom::Text));
}

/// Serves zeros for any offset and counts the bytes handed out.
struct Endless {
    served: usize,
}

impl ReadAt for Endless {
    fn read_at(&mut self, _offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        buf.fill(0);
        self.served += buf.len();
        Ok(buf.len())
    }
}

#[test]
fn a_huge_file_is_read_bounded() {
    let mut src = Endless { served: 0 };
    let s = sniff(&mut src, 64 * 1024 * 1024 * 1024, "bin");
    assert_eq!(s.read, PREFIX);
    // The prefix plus a few small targeted reads, never the whole file.
    assert!(src.served <= 2 * PREFIX, "served {}", src.served);
}

#[test]
fn file_at_reads_at_offsets_and_short_at_the_end() {
    let mut f = tempfile::tempfile().unwrap();
    f.write_all(b"0123456789").unwrap();
    let mut src = FileAt(f);
    let mut buf = [0u8; 4];
    assert_eq!(src.read_at(3, &mut buf).unwrap(), 4);
    assert_eq!(&buf, b"3456");
    assert_eq!(src.read_at(8, &mut buf).unwrap(), 2);
    assert_eq!(&buf[..2], b"89");
    assert_eq!(src.read_at(0, &mut buf).unwrap(), 4, "seeks back");
    assert_eq!(&buf, b"0123");
    assert_eq!(src.read_at(100, &mut buf).unwrap(), 0);
}

#[test]
fn stream_at_rereads_earlier_bytes_and_stops_at_the_cap() {
    let data: Vec<u8> = (0..100u8).collect();
    let mut src = StreamAt::new(&data[..], 50);
    let mut buf = [0u8; 20];
    assert_eq!(src.read_at(40, &mut buf).unwrap(), 10, "cut at the cap");
    assert_eq!(&buf[..10], &data[40..50]);
    assert_eq!(src.read_at(60, &mut buf).unwrap(), 0, "beyond the cap");
    assert_eq!(src.read_at(0, &mut buf).unwrap(), 20, "kept, not re-read");
    assert_eq!(&buf[..], &data[..20]);
}

#[test]
fn stream_at_over_an_endless_stream_does_not_hang() {
    let mut src = StreamAt::new(io::repeat(0xAB), 1024 * 1024);
    let mut buf = vec![0u8; 16];
    assert_eq!(src.read_at(u64::MAX - 4, &mut buf).unwrap(), 0);
    assert_eq!(src.read_at(1024 * 1024 - 8, &mut buf).unwrap(), 8);
}

/// Fails with `Interrupted` before every chunk, like a signal-heavy pipe.
struct Flaky<R> {
    inner: R,
    interrupt: bool,
}

impl<R: Read> Read for Flaky<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.interrupt = !self.interrupt;
        if self.interrupt {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.inner.read(buf)
    }
}

#[test]
fn stream_at_retries_interrupted_reads() {
    let nes = fx::nes();
    let flaky = Flaky {
        inner: &nes[..],
        interrupt: false,
    };
    let s = sniff(&mut StreamAt::new(flaky, 1 << 20), nes.len() as u64, "nes");
    assert_eq!(best(&s), Some(("nes".into(), 0.95)));
}

#[test]
fn game_boy_by_cgb_flag() {
    assert_eq!(best(&run(&fx::gb(0x00), "gb")), Some(("gb".into(), 0.95)));
    let gbc_only = run(&fx::gb(0xC0), "gb");
    assert_eq!(best(&gbc_only), Some(("gbc".into(), 0.95)));
    assert_eq!(
        gbc_only.excluded,
        ["gb"],
        "a GBC-only cart never goes to GB"
    );
    let dual = run(&fx::gb(0x80), "gb");
    assert_eq!(best(&dual), Some(("gbc".into(), 0.85)));
    assert!(dual.excluded.is_empty());
}

#[test]
fn game_boy_needs_the_header_checksum() {
    let mut bad = fx::gb(0x00);
    bad[0x14D] ^= 0xFF;
    assert_eq!(best(&run(&bad, "gb")), None);
    let mut no_logo = fx::gb(0x00);
    no_logo[0x104] = 0;
    assert_eq!(best(&run(&no_logo, "gb")), None);
}

#[test]
fn gba_by_fixed_byte_and_complement() {
    assert_eq!(best(&run(&fx::gba(), "gba")), Some(("gba".into(), 0.95)));
    let mut bad = fx::gba();
    bad[0xBD] ^= 0x01;
    assert_eq!(best(&run(&bad, "gba")), None, "homebrew without gbafix");
    let mut no_fixed = fx::gba();
    no_fixed[0xB2] = 0;
    assert_eq!(best(&run(&no_fixed, "gba")), None);
}

#[test]
fn nds_by_two_crcs_and_never_as_gba() {
    let nds = fx::nds();
    assert_eq!(best(&run(&nds, "nds")), Some(("nds".into(), 0.95)));
    // An NDS image that also looks like a GBA header stays NDS (the edit is
    // covered by the header CRC, so the fixture recomputes it).
    let hybrid = fx::nds_with(with_gba_area);
    assert!(
        gba_area_is_valid(&hybrid),
        "the hybrid must pass the GBA check on its own"
    );
    assert_eq!(best(&run(&hybrid, "nds")).unwrap().0, "nds");
    assert_eq!(run(&hybrid, "nds").hits.len(), 1, "no GBA evidence too");
    let mut bad = fx::nds();
    bad[0x15E] ^= 0xFF;
    assert_eq!(best(&run(&bad, "nds")), None);
}

/// Gives `b` a GBA-valid fixed byte and complement (a 16-bit signature).
fn with_gba_area(b: &mut [u8]) {
    b[0xB2] = 0x96;
    b[0xBD] = b[0xA0..=0xBC]
        .iter()
        .fold(0u8, |c, v| c.wrapping_sub(*v))
        .wrapping_sub(0x19);
}

/// The fixed byte and complement alone would pass the GBA check.
fn gba_area_is_valid(b: &[u8]) -> bool {
    let complement = b[0xA0..=0xBC]
        .iter()
        .fold(0u8, |c, v| c.wrapping_sub(*v))
        .wrapping_sub(0x19);
    b[0xB2] == 0x96 && b[0xBD] == complement
}

#[test]
fn weak_gba_signature_never_beats_a_stronger_magic() {
    // The GBA check is one fixed byte plus a one-byte complement; a file
    // that carries a 4+ byte magic (or the GB logo and checksum) and
    // happens to match it by chance keeps its own system.
    let cases: Vec<(Vec<u8>, &str, &str)> = vec![
        (fx::nes(), "nes", "nes"),
        (fx::n64(fx::N64Order::Z64), "z64", "n64"),
        (fx::lynx(), "lnx", "lynx"),
        (fx::a7800(), "a78", "a7800"),
        (fx::gb(0x00), "gb", "gb"),
    ];
    for (mut bytes, ext, want) in cases {
        if bytes.len() < 0x150 {
            bytes.resize(0x150, 0);
        }
        let gb_sum_before = bytes[0x14D];
        with_gba_area(&mut bytes);
        assert_eq!(bytes[0x14D], gb_sum_before);
        assert!(gba_area_is_valid(&bytes), "{want}: fixture must look GBA");
        let s = run(&bytes, ext);
        assert_eq!(best(&s).map(|h| h.0), Some(want.to_string()), "{want}");
        assert_eq!(s.hits.len(), 1, "{want}: only its own evidence");
    }
}

#[test]
fn mega_drive_and_interleaved_smd() {
    assert_eq!(
        best(&run(&fx::md(), "md")),
        Some(("megadrive".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::smd_interleaved(), "smd")),
        Some(("megadrive".into(), 0.90))
    );
}

#[test]
fn sega_cd_32x_and_pico_never_come_out_as_mega_drive() {
    assert_eq!(
        best(&run(&fx::segacd_iso(false), "iso")).unwrap().0,
        "segacd"
    );
    assert_eq!(
        best(&run(&fx::segacd_iso(true), "bin")).unwrap().0,
        "segacd"
    );
    assert_eq!(
        best(&run(&fx::sega32x(), "32x")),
        Some(("sega32x".into(), 0.95))
    );
    // A Pico cart is Sega Pico now (spec §8.1), never Mega Drive.
    let pico = run(&fx::pico(), "md");
    assert!(pico
        .hits
        .iter()
        .any(|h| h.system == "pico" && h.weight >= 0.95));
    assert!(pico.hits.iter().all(|h| h.system != "megadrive"));
    assert_eq!(pico.unsupported, None);
}

#[test]
fn pico_console_field_variants() {
    // The 16-byte console field at 0x100 is matched as the cores match it:
    // Genesis Plus GX (`strstr(consoletype, "SEGA PICO")`, core/loadrom.c
    // at f27655b5) and PicoDrive (`strstr(buff, " PICO ")`, pico/media.c at
    // 26ecb2b6), not only as a prefix.
    for field in [
        &b"SEGA PICO       "[..],
        b" SEGA PICO      ",
        b"SEGATOYS PICO   ",
    ] {
        let mut b = fx::pico();
        b[0x100..0x110].copy_from_slice(field);
        let s = run(&b, "md");
        assert_eq!(
            best(&s),
            Some(("pico".into(), 0.95)),
            "{}",
            String::from_utf8_lossy(field)
        );
        assert!(s.hits.iter().all(|h| h.system != "megadrive"));
    }
    // `PICO` outside the console field, or glued to other letters, is not
    // a Pico: a Mega Drive cart stays Mega Drive.
    let mut md = fx::pico();
    md[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    md[0x120..0x12A].copy_from_slice(b"SEGA PICO ");
    assert_eq!(best(&run(&md, "md")).unwrap().0, "megadrive");
    let mut glued = fx::pico();
    glued[0x100..0x110].copy_from_slice(b"SEGA PICOLAND   ");
    assert_eq!(best(&run(&glued, "md")).unwrap().0, "pico");
    glued[0x100..0x110].copy_from_slice(b"SEGA EPICO      ");
    assert_eq!(best(&run(&glued, "md")).unwrap().0, "megadrive");
}

#[test]
fn master_system_and_game_gear() {
    // TMR SEGA at any of the three offsets → SMS family.
    for offset in [0x7FF0usize, 0x3FF0, 0x1FF0] {
        let sms = run(&fx::sms_gg(offset, 0x4C), "sms");
        assert_eq!(
            best(&sms),
            Some(("mastersystem".into(), 0.90)),
            "{offset:#x}"
        );
    }
    // The extension decides even against the region code.
    let gg_with_sms_region = run(&fx::sms_gg(0x7FF0, 0x4C), "gg");
    assert_eq!(best(&gg_with_sms_region), Some(("gamegear".into(), 0.90)));
    // Without a deciding extension the region is only a tiebreak.
    let gg_region = run(&fx::sms_gg(0x7FF0, 0x6C), "bin");
    assert_eq!(best(&gg_region), Some(("gamegear".into(), 0.70)));
}

#[test]
fn snes_copier_header_is_not_mistaken_for_an_smd_dump() {
    // Super Wild Card header: 0xAA 0xBB at 8–9 too (type 0x04 at 10), and
    // a 512 KiB SNES ROM + 512 also satisfies `len % 16384 == 512`.
    let mut swc = vec![0u8; 512 * 1024 + 512];
    swc[8] = 0xAA;
    swc[9] = 0xBB;
    swc[10] = 0x04;
    let s = run(&swc, "smc");
    assert!(
        s.hits.iter().all(|e| e.system != "megadrive"),
        "{:?}",
        s.hits
    );
}

#[test]
fn sega_near_misses() {
    // `SEGA` one byte too far.
    let mut md = vec![0u8; 128 * 1024];
    md[0x102..0x106].copy_from_slice(b"SEGA");
    assert_eq!(best(&run(&md, "md")), None);
    // ` SEGA` at 0x101 is still a Mega Drive header.
    let mut md101 = vec![0u8; 128 * 1024];
    md101[0x101..0x105].copy_from_slice(b"SEGA");
    assert_eq!(best(&run(&md101, "bin")), Some(("megadrive".into(), 0.90)));
    // 32X by its `MARS` security code under a plain `SEGA` header.
    let mut mars = fx::md();
    mars[0x3C0..0x3CF].copy_from_slice(b"MARS CHECK MODE");
    assert_eq!(best(&run(&mars, "bin")), Some(("sega32x".into(), 0.95)));
    // SMD header with the wrong length.
    let mut smd = fx::smd_interleaved();
    smd.push(0);
    assert_eq!(best(&run(&smd, "smd")), None);
    // `TMR SEGA` off by one.
    let mut tmr = vec![0u8; 32 * 1024];
    tmr[0x7FF1..0x7FF9].copy_from_slice(b"TMR SEGA");
    assert_eq!(best(&run(&tmr, "sms")), None);
}

#[test]
fn snes_lorom_hirom_exhirom_with_and_without_copier_header() {
    use fx::SnesMap::*;
    for (map, copier) in [
        (Lo, false),
        (Lo, true),
        (Hi, false),
        (Hi, true),
        (ExHi, false),
    ] {
        let rom = fx::snes(map, copier);
        assert_eq!(
            best(&run(&rom, "sfc")),
            Some(("snes".into(), 0.95)),
            "{map:?} copier={copier}"
        );
    }
}

#[test]
fn snes_with_a_broken_checksum_but_valid_map_and_title_is_weaker() {
    let mut rom = fx::snes(fx::SnesMap::Lo, false);
    rom[0x7FC0 + 0x1C] ^= 0xFF;
    assert_eq!(best(&run(&rom, "sfc")), Some(("snes".into(), 0.85)));
}

#[test]
fn neo_geo_pocket_and_colour() {
    assert_eq!(
        best(&run(&fx::ngp(false), "ngp")),
        Some(("ngp".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::ngp(true), "ngc")),
        Some(("ngpc".into(), 0.90))
    );
}

#[test]
fn wonderswan_footer_only_for_its_extensions() {
    assert_eq!(
        best(&run(&fx::ws(false), "ws")),
        Some(("wonderswan".into(), 0.85))
    );
    assert_eq!(
        best(&run(&fx::ws(true), "wsc")),
        Some(("wonderswancolor".into(), 0.85))
    );
    assert_eq!(
        best(&run(&fx::ws(false), "bin")),
        None,
        "0xEA alone is too common"
    );
}

#[test]
fn wonderswan_real_footer_layout() {
    // Last 16 bytes of real dumps (`Final Fantasy (J).wsc`, a mono one):
    // the far jump opens the 16-byte footer, the colour flag is byte 7.
    for (tail, system) in [
        ("ea000000400001010100060204006b72", "wonderswancolor"),
        ("ea00000090000000008006010400a8b5", "wonderswan"),
    ] {
        let mut rom = vec![0u8; 128 * 1024];
        let n = rom.len();
        for (i, b) in (0..tail.len()).step_by(2).enumerate() {
            rom[n - 16 + i] = u8::from_str_radix(&tail[b..b + 2], 16).unwrap();
        }
        assert_eq!(
            best(&run(&rom, "wsc")),
            Some((system.into(), 0.85)),
            "{tail}"
        );
    }
    // 0xEA ten bytes from the end (the developer id slot) is not a footer.
    let mut late = vec![0u8; 128 * 1024];
    let n = late.len();
    late[n - 10] = 0xEA;
    assert_eq!(best(&run(&late, "ws")), None);
}

#[test]
fn snes_spc7110_and_sdd1_map_modes() {
    // Momotarou Dentetsu Happy / Super Power League 4 (SPC7110, 0x3A) and
    // ExLoROM / S-DD1 (0x22).
    for mode in [0x3Au8, 0x22] {
        let mut rom = fx::snes(fx::SnesMap::Hi, false);
        rom[0xFFC0 + 0x15] = mode;
        assert_eq!(
            best(&run(&rom, "sfc")),
            Some(("snes".into(), 0.95)),
            "{mode:#x}"
        );
    }
}

#[test]
fn snes_title_overflowing_the_map_byte_still_matches_by_checksum() {
    // `Contra III` / `Super Formation Soccer` dumps renamed by old tools:
    // a 22-byte title overwrites the map mode, the checksum pair and the
    // ROM size byte (0x17) stay valid.
    let mut rom = fx::snes(fx::SnesMap::Lo, false);
    rom[0x7FC0 + 0x15] = b'S';
    rom[0x7FC0 + 0x17] = 0x0A;
    assert_eq!(best(&run(&rom, "sfc")), Some(("snes".into(), 0.95)));
    // Without a plausible ROM size the bare 16-bit pair is not enough.
    rom[0x7FC0 + 0x17] = 0x00;
    assert_eq!(best(&run(&rom, "sfc")), None);
}

#[test]
fn a_gba_rom_with_its_logo_is_not_taken_for_snes() {
    // `Lizzie McGuire` (GBA): code at 0x7FC0 reads as map 0x20 and a
    // 0xFFFF/0x0000 checksum pair; `It's Mr Pants`: as map + ASCII title.
    let mut lizzie = fx::gba();
    lizzie.resize(0x10_0000, 0);
    lizzie[0x7FC0 + 0x15] = 0x20;
    lizzie[0x7FC0 + 0x1C..0x7FC0 + 0x1E].copy_from_slice(&[0xFF, 0xFF]);
    assert_eq!(best(&run(&lizzie, "gba")), Some(("gba".into(), 0.95)));
    let mut pants = fx::gba();
    pants.resize(0x10_0000, 0);
    pants[0x7FC0..0x7FD5].copy_from_slice(b"SOME TEXT IN THE DATA");
    pants[0x7FC0 + 0x15] = 0x31;
    assert_eq!(best(&run(&pants, "gba")), Some(("gba".into(), 0.95)));
}

#[test]
fn snes_blank_checksum_pair_needs_a_title() {
    // 0xFFFF filler next to zeros is a coincidence, not a checksum.
    let mut blank = vec![0u8; 0x8_0000];
    blank[0x7FC0 + 0x15] = 0x20;
    blank[0x7FC0 + 0x1C..0x7FC0 + 0x1E].copy_from_slice(&[0xFF, 0xFF]);
    assert_eq!(best(&run(&blank, "sfc")), None);
    blank[0x7FC0..0x7FD5].copy_from_slice(b"HOMEBREW             ");
    assert_eq!(best(&run(&blank, "sfc")), Some(("snes".into(), 0.85)));
}

#[test]
fn playstation_by_system_cnf() {
    for raw in [false, true] {
        let ps1 = run(&fx::psx_iso(raw, false), if raw { "bin" } else { "iso" });
        assert_eq!(best(&ps1), Some(("psx".into(), 0.95)), "raw={raw}");
        let ps2 = run(&fx::psx_iso(raw, true), "iso");
        assert!(
            ps2.hits.iter().all(|h| h.system != "psx"),
            "PS2 is never psx"
        );
        assert!(ps2
            .hits
            .iter()
            .any(|h| h.system == "ps2" && h.weight >= 0.95 && h.detail == "SYSTEM.CNF BOOT2"));
        assert!(ps2.unsupported.is_none());
        assert_eq!(ps2.serial.as_deref(), Some("SLUS-20001"));
    }
}

#[test]
fn playstation_without_system_cnf_is_family_only() {
    let mut iso = fx::psx_iso(false, false);
    fx::drop_system_cnf(&mut iso);
    assert_eq!(best(&run(&iso, "iso")), Some(("psx".into(), 0.80)));
}

#[test]
fn saturn_dreamcast_psp() {
    assert_eq!(
        best(&run(&fx::saturn(), "iso")),
        Some(("saturn".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::dreamcast_track(), "bin")),
        Some(("dreamcast".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::psp_iso(), "iso")),
        Some(("psp".into(), 0.85))
    );
    assert_eq!(best(&run(&fx::cso(), "cso")), Some(("psp".into(), 0.85)));
}

#[test]
fn random_bytes_never_reach_high_confidence() {
    for seed in 1..=400u64 {
        let bytes = fx::xorshift_bytes(seed, 64 * 1024 + (seed as usize % 7) * 512);
        for ext in ["bin", "gb", "gba", "sfc", "md", "iso", "ws"] {
            let s = run(&bytes, ext);
            assert!(
                s.hits.iter().all(|e| e.weight < 0.95),
                "seed {seed} ext {ext}: {:?}",
                s.hits
            );
        }
    }
}

#[test]
fn psp_by_the_system_id_of_a_real_umd() {
    // Real UMD images carry `PSP GAME` as the ISO 9660 system id and the
    // game code as the volume id.
    let iso = fx::psp_iso();
    assert_eq!(&iso[0x8008..0x8010], b"PSP GAME");
    assert_ne!(&iso[0x8028..0x8030], b"PSP GAME");
    assert_eq!(best(&run(&iso, "iso")), Some(("psp".into(), 0.85)));
}

#[test]
fn playstation_disc_larger_than_a_cd_is_ps2() {
    // No readable SYSTEM.CNF, but no CD holds this much: a PS2 DVD.
    let mut iso = fx::psx_iso(false, false);
    fx::drop_system_cnf(&mut iso);
    let s = sniff(&mut SliceAt(&iso), 4 * 1024 * 1024 * 1024, "iso");
    assert_eq!(best(&s), Some(("ps2".into(), 0.80)), "{:?}", s.hits);
    assert!(s.unsupported.is_none());
}

#[test]
fn cso_larger_than_a_umd_is_not_psp() {
    // PCSX2 also reads CSO: an image bigger than a dual-layer UMD is a DVD.
    let mut cso = fx::cso();
    cso[8..16].copy_from_slice(&(4u64 * 1024 * 1024 * 1024).to_le_bytes());
    assert_eq!(best(&run(&cso, "cso")), None);
}

#[test]
fn disc_rows_carry_serials() {
    use coverly_core::disc::testing as dt;
    // The head-magic verdicts are today's; the serial comes from `disc::`.
    let saturn = run(&dt::saturn_track("T-5704H   "), "bin");
    assert_eq!(best(&saturn), Some(("saturn".into(), 0.90)));
    assert_eq!(saturn.serial.as_deref(), Some("T-5704H"));
    let dc = run(&dt::dreamcast_track03("T-40214N  ", "1/1"), "bin");
    assert_eq!(best(&dc), Some(("dreamcast".into(), 0.90)));
    assert_eq!(dc.serial.as_deref(), Some("T-40214N"));
    let segacd = run(&dt::segacd_track(), "bin");
    assert_eq!(best(&segacd), Some(("segacd".into(), 0.95)));
    assert_eq!(segacd.serial.as_deref(), Some("MK-4407"));
    for raw in [false, true] {
        let ps1 = run(&fx::psx_iso(raw, false), "bin");
        assert_eq!(ps1.serial.as_deref(), Some("SLUS-00001"), "raw={raw}");
        let ps2 = run(&fx::psx_iso(raw, true), "iso");
        assert_eq!(ps2.serial.as_deref(), Some("SLUS-20001"), "raw={raw}");
    }
    // The bare magic fixtures have no product number; carts have no serial.
    assert_eq!(run(&fx::saturn(), "iso").serial, None);
    assert_eq!(run(&fx::md(), "md").serial, None);
    let mut iso = fx::psx_iso(false, false);
    fx::drop_system_cnf(&mut iso);
    assert_eq!(run(&iso, "iso").serial, None);
}

#[test]
fn text_native_game_files_are_not_text_roms() {
    // D26: Intel HEX (Arduboy), LowRes NX source, Lua and `.game` files
    // are text by nature.
    let hex = b":100000000C9435000C945D000C945D000C945D0024\n";
    assert_eq!(run(hex, "hex").not_a_rom, None);
    let nx = b"PRINT \"HELLO\"\nWAIT VBL\nGOTO 1\n";
    assert_eq!(run(nx, "nx").not_a_rom, None);
    assert_eq!(run(b"function love.load()\nend\n", "lua").not_a_rom, None);
    assert_eq!(run(b"[game]\nname=Test\n", "game").not_a_rom, None);
    // The same bytes under a ROM extension are still text.
    assert_eq!(run(hex, "gba").not_a_rom, Some(NotARom::Text));
    // An HTML error page is never a game file.
    assert_eq!(
        run(&fx::html_page(), "hex").not_a_rom,
        Some(NotARom::WebPage)
    );
}

#[test]
fn a_ps2_volume_inside_a_cso_is_ps2() {
    use coverly_core::disc::testing as dt;
    // The CISO reader reaches the PVD: BOOT2 says PS2, never PSP.
    let s = run(&dt::cso_of(&dt::ps2_iso()), "cso");
    assert_eq!(best(&s), Some(("ps2".into(), 0.95)), "{:?}", s.hits);
    assert!(s.hits.iter().all(|h| h.system != "psp"));
    assert_eq!(s.serial.as_deref(), Some("SLUS-20062"));
    // A compressed UMD stays PSP.
    let umd = run(&dt::cso_of(&fx::psp_iso()), "cso");
    assert_eq!(best(&umd), Some(("psp".into(), 0.85)));
}

#[test]
fn vectrex_gce_header() {
    // The Executive ROM only boots a cartridge that opens with its
    // `g GCE` copyright line (spec §8.1).
    for ext in ["vec", "bin", "gam"] {
        let s = run(&fx::vectrex(), ext);
        assert_eq!(best(&s), Some(("vectrex".into(), 0.95)), ".{ext}");
    }
    // The same cartridge without the magic says nothing.
    let mut bare = fx::vectrex();
    bare[..5].copy_from_slice(&[0; 5]);
    assert!(run(&bare, "vec").hits.is_empty());
}

#[test]
fn colecovision_test_bytes_are_weak() {
    // 16 bits: they support a folder or extension, never Auto alone.
    for swapped in [false, true] {
        for ext in ["col", "rom", "bin"] {
            let s = run(&fx::colecovision(swapped), ext);
            assert_eq!(
                best(&s),
                Some(("colecovision".into(), 0.60)),
                "swapped={swapped} .{ext}"
            );
        }
    }
    // Under any other extension the two bytes mean nothing.
    assert!(run(&fx::colecovision(false), "sms").hits.is_empty());
}

#[test]
fn jaguar_abs_and_cof() {
    assert_eq!(
        best(&run(&fx::jaguar_abs(), "abs")),
        Some(("jaguar".into(), 0.50))
    );
    assert_eq!(
        best(&run(&fx::jaguar_cof(), "cof")),
        Some(("jaguar".into(), 0.50))
    );
    // Each magic only under its own extension.
    assert!(run(&fx::jaguar_abs(), "cof").hits.is_empty());
    assert!(run(&fx::jaguar_cof(), "bin").hits.is_empty());
}

#[test]
fn vectrex_is_decisive() {
    assert!(coverly_core::identify::sniff::DECISIVE.contains(&"vectrex"));
}

#[test]
fn sufami_magic() {
    // Snes9x `is_SufamiTurbo_Cart`: `BANDAI SFC-ADX` at 0, and not the
    // `SFC-ADX BACKUP` line of the BIOS at 0x10.
    for ext in ["st", "sfc", "smc", "bin"] {
        assert_eq!(
            best(&run(&fx::sufami(false), ext)),
            Some(("sufami".into(), 0.95)),
            ".{ext}"
        );
    }
    let bios = run(&fx::sufami(true), "st");
    assert!(bios.hits.iter().all(|h| h.system != "sufami"));
    let mut near = fx::sufami(false);
    near[0] = b'b';
    assert!(run(&near, "st").hits.iter().all(|h| h.system != "sufami"));
}

#[test]
fn pico_and_sufami_are_decisive() {
    for id in ["pico", "sufami"] {
        assert!(
            coverly_core::identify::sniff::DECISIVE.contains(&id),
            "{id}"
        );
    }
}

#[test]
fn tzx_magic() {
    // TZX format 1.20: `ZXTape!` + 0x1A at 0. Amstrad CPC tapes (`.cdt`)
    // are TZX files too, so the extension picks the machine.
    assert_eq!(
        best(&run(&fx::tzx(), "tzx")),
        Some(("zxspectrum".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::tzx(), "cdt")),
        Some(("amstradcpc".into(), 0.90))
    );
    let mut near = fx::tzx();
    near[7] = 0x1B;
    assert!(run(&near, "tzx").hits.is_empty());
}

#[test]
fn cpc_dsk_magic_is_a_review() {
    // CPCEMU DSK: the same header opens CPC and Spectrum +3 disks, so it
    // names both at 0.50, Amstrad CPC first (the likelier one).
    for extended in [false, true] {
        let s = run(&fx::cpc_dsk(extended), "dsk");
        let hits: Vec<(&str, f32, &str)> = s
            .hits
            .iter()
            .map(|h| (h.system.as_str(), h.weight, h.detail.as_str()))
            .collect();
        assert_eq!(
            hits,
            [
                ("amstradcpc", 0.50, "CPC DSK"),
                ("zxspectrum", 0.50, "CPC DSK")
            ],
            "extended {extended}"
        );
    }
    // Only under `.dsk` (spec §8.1).
    assert!(run(&fx::cpc_dsk(false), "bin").hits.is_empty());
}

#[test]
fn uef_magic() {
    // UEF: `UEF File!` + NUL at 0. A gzip `.uef` is a package (D25): its
    // decoded head is read through the archive layer (organize_inspect).
    assert_eq!(
        best(&run(&fx::uef(), "uef")),
        Some(("bbcmicro".into(), 0.95))
    );
    let mut near = fx::uef();
    near[9] = b'?';
    assert!(run(&near, "uef").hits.is_empty());
}

#[test]
fn cpc_sna_magic() {
    // A CPCEMU snapshot opens with `MV - SNA`; a Spectrum `.sna` has no
    // magic (a 27-byte register dump), so the header decides `.sna`.
    assert_eq!(
        best(&run(&fx::cpc_sna(), "sna")),
        Some(("amstradcpc".into(), 0.90))
    );
    // Only under `.sna`.
    assert!(run(&fx::cpc_sna(), "bin").hits.is_empty());
    let mut near = fx::cpc_sna();
    near[7] = b'X';
    assert!(run(&near, "sna").hits.is_empty());
}

#[test]
fn nhd_magic() {
    // T98-Next NHD: `T98HDDIMAGE.R0` + NUL at 0 (Neko Project II kai
    // `fdd/sxsihdd.c`, DOSBox-X `bios_disk.cpp`).
    assert_eq!(best(&run(&fx::nhd(), "nhd")), Some(("pc98".into(), 0.95)));
    let mut near = fx::nhd();
    near[13] = b'1';
    assert!(run(&near, "nhd").hits.is_empty());
}

#[test]
fn hdf_amiga_or_x68000() {
    // Amiga RDB (`RDSK` block id), an Amiga partition hardfile (`DOS` +
    // a flag byte 0–7) and an X68000 SCSI disk (`X68SCSI1`, NetBSD/x68k
    // `disksubr.c`), all under `.hdf` only.
    assert_eq!(
        best(&run(&fx::amiga_rdb(), "hdf")),
        Some(("amiga".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::amiga_dos(), "hdf")),
        Some(("amiga".into(), 0.90))
    );
    assert_eq!(
        best(&run(&fx::x68k_scsi(), "hdf")),
        Some(("x68000".into(), 0.90))
    );
    let mut flag = fx::amiga_dos();
    flag[3] = 8;
    assert!(run(&flag, "hdf").hits.is_empty());
    // Keyed by extension: the same bytes under another one say nothing.
    for bytes in [fx::amiga_rdb(), fx::amiga_dos(), fx::x68k_scsi()] {
        assert!(run(&bytes, "img").hits.is_empty());
    }
}

#[test]
fn fdi_amiga() {
    // FDI 2.0 (Disk2FDI): `Formatted Disk Image file` at 0 (WinUAE
    // `disk.cpp`). A PC-98 Anex86 `.fdi` has no magic.
    assert_eq!(best(&run(&fx::fdi2(), "fdi")), Some(("amiga".into(), 0.90)));
    assert!(run(&fx::fdi2(), "bin").hits.is_empty());
    assert!(run(&fx::zeros(4096), "fdi").hits.is_empty());
}

#[test]
fn dim_atari_st() {
    // FastCopy DIM: `42 42` at 0, byte 3 and byte 0x0A zero (Hatari
    // `src/floppies/dim.c`).
    assert_eq!(
        best(&run(&fx::st_dim(), "dim")),
        Some(("atarist".into(), 0.85))
    );
    let mut near = fx::st_dim();
    near[0x0A] = 1;
    assert!(run(&near, "dim").hits.is_empty());
    assert!(run(&fx::st_dim(), "st").hits.is_empty());
}

#[test]
fn hdf_rdb_in_a_later_block() {
    // AmigaOS `devices/hardblocks.h`: the Rigid Disk Block may sit in any of
    // the first RDB_LOCATION_LIMIT (16) blocks, and WinUAE `filesys.cpp`
    // `rdb_mount` scans them, accepting a block whose longs sum to zero.
    for block in [1usize, 15] {
        let bytes = fx::amiga_rdb_at(block);
        assert_eq!(
            best(&run(&bytes, "hdf")),
            Some(("amiga".into(), 0.90)),
            "block {block}"
        );
        // A bad checksum outside block 0 is not an RDB.
        let mut bad = bytes.clone();
        bad[block * 512 + 8] ^= 1;
        assert!(run(&bad, "hdf").hits.is_empty(), "block {block}");
    }
    // Past the limit it says nothing.
    assert!(run(&fx::amiga_rdb_at(16), "hdf").hits.is_empty());
    assert!(run(&fx::amiga_rdb_at(1), "img").hits.is_empty());
}

#[test]
fn woz_and_2img() {
    // Applesauce WOZ reference: `WOZ1`/`WOZ2`, then `FF 0A 0D 0A`. The
    // 2IMG spec: `2IMG`, a creator tag, then the 64-byte header size.
    for version in [1u8, 2] {
        assert_eq!(
            best(&run(&fx::woz(version), "woz")),
            Some(("apple2".into(), 0.95)),
            "WOZ{version}"
        );
    }
    assert_eq!(
        best(&run(&fx::two_img(), "2mg")),
        Some(("apple2".into(), 0.95))
    );
    // A line-ending conversion broke the magic: not a WOZ any more.
    let mut mangled = fx::woz(2);
    mangled[5] = 0x0D;
    assert!(run(&mangled, "woz").hits.is_empty());
    let mut short = fx::two_img();
    short[8] = 0x20;
    assert!(run(&short, "2mg").hits.is_empty());
}

#[test]
fn prg_load_address_is_a_tiebreak_only() {
    // Commodore BASIC start per machine: Plus/4 $1001, C128 $1C01, PET
    // $0401, C64 $0801. Two bytes only: below the 0.50 review line.
    for (lo_hi, want) in [
        ([0x01, 0x10], "plus4"),
        ([0x01, 0x1C], "c128"),
        ([0x01, 0x04], "pet"),
        ([0x01, 0x08], "c64"),
    ] {
        let bytes = fx::prg(lo_hi);
        assert_eq!(
            best(&run(&bytes, "prg")),
            Some((want.into(), 0.40)),
            "{want}"
        );
        // Only under `.prg`.
        assert!(run(&bytes, "bin").hits.is_empty(), "{want}");
    }
    assert!(run(&fx::prg([0x00, 0xC0]), "prg").hits.is_empty());
}

#[test]
fn uzebox_header() {
    // Uzebox `tools/packrom/packrom.cpp` (2fc77093): the `.uze` header
    // opens with the 6-byte marker `UZEBOX`, which the bootloader checks.
    assert_eq!(
        best(&run(&fx::uzebox(), "uze")),
        Some(("uzebox".into(), 0.95))
    );
    let mut near = fx::uzebox();
    near[5] = b'Y';
    assert!(run(&near, "uze").hits.is_empty());
    assert!(coverly_core::identify::sniff::DECISIVE.contains(&"uzebox"));
}

#[test]
fn vircon32_header() {
    // vircon32-libretro `VirconDefinitions/FileFormats.hpp` (8b4ff35e):
    // a cartridge opens with `V32-CART` (a BIOS with `V32-BIOS`).
    assert_eq!(
        best(&run(&fx::vircon32(), "v32")),
        Some(("vircon32".into(), 0.95))
    );
    let mut bios = fx::vircon32();
    bios[4..8].copy_from_slice(b"BIOS");
    assert!(run(&bios, "v32").hits.is_empty());
    assert!(coverly_core::identify::sniff::DECISIVE.contains(&"vircon32"));
}

#[test]
fn wasm_magic_proves_nothing() {
    // `\0asm` only proves WebAssembly (spec §8.1): no system.
    let s = run(&fx::wasm(), "wasm");
    assert!(s.hits.is_empty(), "{:?}", s.hits);
    assert_eq!(s.not_a_rom, None);
}

#[test]
fn wad_magic() {
    // Doom WADs open with `IWAD` (a game) or `PWAD` (a patch or mod),
    // then the lump count and the directory offset (spec D15).
    for magic in [b"IWAD", b"PWAD"] {
        assert_eq!(
            best(&run(&fx::wad(magic), "wad")),
            Some(("doom".into(), 0.95)),
            "{}",
            String::from_utf8_lossy(magic)
        );
    }
    assert!(run(&fx::wad(b"XWAD"), "wad").hits.is_empty());
    assert!(coverly_core::identify::sniff::DECISIVE.contains(&"doom"));
}

#[test]
fn pak_alone_is_not_quake() {
    // OpenBOR uses the same `PACK` magic as Quake: it names nothing.
    let s = run(&fx::pack(), "pak");
    assert!(s.hits.iter().all(|h| h.system != "quake"), "{:?}", s.hits);
}
