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
        Some(ContainerMagic::Rar)
    );
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
    let pico = run(&fx::pico(), "md");
    assert!(pico.hits.is_empty());
    assert_eq!(pico.unsupported.as_deref(), Some("Sega Pico"));
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
