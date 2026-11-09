//! Base, update, DLC and demo facts per system through the public
//! readers (game folders spec D10, D11, D14): a table harness that the
//! kind and version tasks extend. Synthetic headers only, no keys.

use coverly_core::consoles::testing::{
    cia_with_code, gc_disc, meta_xml, ncsd, param_sfo, pbp, pkg, ps3_sfo, wii_wad, SparseAt,
};
use coverly_core::consoles::{identify, psf, sony, wiiu, Budgeted, ConsoleId, TitleKind};
use coverly_core::identify::names::Region;

/// A file fixture through [`identify`], as the walk reads it.
fn read(mut src: SparseAt, ext: &str, name: &str) -> Option<ConsoleId> {
    let len = src.len;
    identify(&mut Budgeted::new(&mut src, len), ext, name)
}

/// A small byte fixture as a file.
fn bytes(b: Vec<u8>) -> SparseAt {
    let mut s = SparseAt::new(b.len() as u64);
    s.put(0, &b);
    s
}

/// A `PARAM.SFO` as a folder probe reads it.
fn psf_id(b: &[u8], system: &str) -> Option<ConsoleId> {
    Some(sony::from_psf(&psf::parse(b)?, system, "PARAM.SFO"))
}

struct Row {
    what: &'static str,
    id: Option<ConsoleId>,
    system: &'static str,
    kind: TitleKind,
    region: Option<Region>,
}

fn regions() -> Vec<Row> {
    vec![
        Row {
            what: "3DS base cartridge, World product code",
            id: read(
                ncsd("CTR-P-EK2A", 0x0004_0000_0012_3400, false),
                "3ds",
                "g.3ds",
            ),
            system: "3ds",
            kind: TitleKind::Base,
            region: Some(Region::World),
        },
        Row {
            what: "3DS base CIA, USA product code",
            id: read(
                cia_with_code(0x0004_0000_0012_3400, "CTR-P-AXXE", false, false),
                "cia",
                "g.cia",
            ),
            system: "3ds",
            kind: TitleKind::Base,
            region: Some(Region::Usa),
        },
        Row {
            what: "3DS update: its own product code is never its region",
            id: read(
                cia_with_code(0x0004_000E_0012_3400, "CTR-P-AXXE", false, false),
                "cia",
                "u.cia",
            ),
            system: "3ds",
            kind: TitleKind::Update,
            region: None,
        },
        Row {
            what: "Wii U base meta.xml",
            id: wiiu::parse_meta_xml(
                meta_xml("0005000010101C00", "WUP-P-ARDP", "Test Game").as_bytes(),
            ),
            system: "wiiu",
            kind: TitleKind::Base,
            region: Some(Region::Europe),
        },
        Row {
            what: "Wii U update meta.xml: no region of its own",
            id: wiiu::parse_meta_xml(
                meta_xml("0005000E10101C00", "WUP-P-ARDP", "Test Game").as_bytes(),
            ),
            system: "wiiu",
            kind: TitleKind::Update,
            region: None,
        },
        Row {
            what: "GameCube disc",
            id: read(gc_disc("GM8E01", false), "iso", "g.iso"),
            system: "gamecube",
            kind: TitleKind::Unknown,
            region: Some(Region::Usa),
        },
        Row {
            what: "Wii disc",
            id: read(gc_disc("RMCP01", true), "iso", "g.iso"),
            system: "wii",
            kind: TitleKind::Unknown,
            region: Some(Region::Europe),
        },
        Row {
            what: "WiiWare WAD",
            id: read(wii_wad(0x0001_0001_5741_414A), "wad", "g.wad"),
            system: "wii",
            kind: TitleKind::Base,
            region: Some(Region::Japan),
        },
        Row {
            what: "PS3 PKG by its content id",
            id: read(
                pkg(1, "EP0001-NPEB00001_00-0000000000000000"),
                "pkg",
                "g.pkg",
            ),
            system: "ps3",
            kind: TitleKind::Unknown,
            region: Some(Region::Europe),
        },
        Row {
            what: "PS3 PARAM.SFO without a content id: the serial",
            id: psf_id(&ps3_sfo("BLUS30001", "DG"), "ps3"),
            system: "ps3",
            kind: TitleKind::Base,
            region: Some(Region::Usa),
        },
        Row {
            what: "PSP EBOOT.PBP by its DISC_ID",
            id: read(bytes(pbp("UG", "ULJM05000", b"")), "pbp", "EBOOT.PBP"),
            system: "psp",
            kind: TitleKind::Base,
            region: Some(Region::Japan),
        },
        Row {
            what: "Vita PARAM.SFO with a content id",
            id: psf_id(
                &param_sfo(&[
                    ("CATEGORY", "gd"),
                    ("CONTENT_ID", "JP0001-PCSG00001_00-0000000000000000"),
                    ("TITLE_ID", "PCSG00001"),
                ]),
                "psvita",
            ),
            system: "psvita",
            kind: TitleKind::Base,
            region: Some(Region::Japan),
        },
        Row {
            what: "Vita PARAM.SFO without a content id: no region",
            id: psf_id(
                &param_sfo(&[("CATEGORY", "gd"), ("TITLE_ID", "PCSE00123")]),
                "psvita",
            ),
            system: "psvita",
            kind: TitleKind::Base,
            region: None,
        },
    ]
}

#[test]
fn regions_per_system() {
    for row in regions() {
        let id = row
            .id
            .unwrap_or_else(|| panic!("{}: not identified", row.what));
        assert_eq!(
            (id.system.as_str(), id.kind, id.region),
            (row.system, row.kind, row.region),
            "{}",
            row.what
        );
    }
}
