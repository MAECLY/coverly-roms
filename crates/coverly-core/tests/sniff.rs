//! Header magic per system (spec §6.3). Fixtures hold only the bytes a
//! check reads; everything else is zero.

use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::identify::sniff::{sniff, ContainerMagic, NotARom, SliceAt, Sniff};

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
