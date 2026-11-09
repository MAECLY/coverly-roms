//! Base, update, DLC and demo facts per system through the public
//! readers (game folders spec D10, D11, D14): a table harness that the
//! kind and version tasks extend. Synthetic headers only, no keys.

use coverly_core::consoles::testing::{
    cia, cia_with_code, gc_disc, meta_xml, ncsd, nsp, param_sfo, pbp, pkg, ps3_sfo, wad_head,
    wii_wad, SparseAt,
};
use coverly_core::consoles::{identify, psf, sony, wiiu, Budgeted, ConsoleId, TitleKind};
use coverly_core::identify::names::Region;
use coverly_core::identify::sniff::{sniff_named, NotARom};

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

/// A plaintext `cnmt.xml` of `kind` for `id` at `version`.
fn cnmt(kind: &str, id: &str, version: u32) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ContentMeta>\n  <Type>{kind}</Type>\n  \
         <Id>0x{id}</Id>\n  <Version>{version}</Version>\n</ContentMeta>\n"
    )
    .into_bytes()
}

/// An NSP holding one NCA and its `cnmt.xml` in clear.
fn nsp_with_cnmt(kind: &str, id: &str, version: u32) -> SparseAt {
    let xml = cnmt(kind, id, version);
    nsp(&[
        ("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.nca", &[0x55; 64]),
        ("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.cnmt.xml", &xml),
    ])
}

struct Kind {
    what: &'static str,
    id: Option<ConsoleId>,
    system: &'static str,
    kind: TitleKind,
    title_id: Option<&'static str>,
    version: Option<&'static str>,
}

/// Game folders spec D14 (a), (b), (d) through the readers.
fn kinds() -> Vec<Kind> {
    vec![
        Kind {
            what: "Switch DataPatch NSP by its cnmt.xml",
            id: read(
                nsp_with_cnmt("DataPatch", "0100000000011801", 65536),
                "nsp",
                "x.nsp",
            ),
            system: "switch",
            kind: TitleKind::Dlc,
            title_id: Some("0100000000011801"),
            version: Some("65536"),
        },
        Kind {
            what: "Switch Delta NSP by its cnmt.xml",
            id: read(
                nsp_with_cnmt("Delta", "0100000000010C00", 131072),
                "nsp",
                "x.nsp",
            ),
            system: "switch",
            kind: TitleKind::Update,
            title_id: Some("0100000000010C00"),
            version: Some("131072"),
        },
        Kind {
            what: "Switch system title NSP by its cnmt.xml",
            id: read(
                nsp_with_cnmt("SystemProgram", "0100000000000809", 0),
                "nsp",
                "x.nsp",
            ),
            system: "switch",
            kind: TitleKind::System,
            title_id: Some("0100000000000809"),
            version: Some("0"),
        },
        Kind {
            what: "Wii DLC WAD",
            id: read(wad_head(0x0001_0005_7746_3445, 3), "wad", "d.wad"),
            system: "wii",
            kind: TitleKind::Dlc,
            title_id: Some("0001000577463445"),
            version: Some("3"),
        },
        Kind {
            what: "WiiWare channel WAD with its TMD version",
            id: read(wad_head(0x0001_0001_5746_3445, 513), "wad", "c.wad"),
            system: "wii",
            kind: TitleKind::Base,
            title_id: Some("0001000157463445"),
            version: Some("513"),
        },
        Kind {
            what: "Wii system channel WAD",
            id: read(wad_head(0x0001_0002_4841_4241, 7), "wad", "s.wad"),
            system: "wii",
            kind: TitleKind::System,
            title_id: Some("0001000248414241"),
            version: Some("7"),
        },
        Kind {
            what: "Wii game channel WAD stays unknown",
            id: read(wad_head(0x0001_0004_5241_4141, 0), "wad", "g.wad"),
            system: "wii",
            kind: TitleKind::Unknown,
            title_id: Some("0001000452414141"),
            version: Some("0"),
        },
        Kind {
            what: "3DS system title CIA",
            id: read(cia(0x0004_001B_0001_0002, false, false), "cia", "s.cia"),
            system: "3ds",
            kind: TitleKind::System,
            title_id: Some("0004001B00010002"),
            version: Some("1040"),
        },
    ]
}

#[test]
fn kinds_per_system() {
    for row in kinds() {
        let id = row
            .id
            .unwrap_or_else(|| panic!("{}: not identified", row.what));
        assert_eq!(
            (
                id.system.as_str(),
                id.kind,
                id.title_id.as_deref(),
                id.version.as_deref()
            ),
            (row.system, row.kind, row.title_id, row.version),
            "{}",
            row.what
        );
    }
}

/// Game folders plan P-1: a lone system title is no ROM and no hit.
#[test]
fn system_titles_are_not_roms() {
    let cases: Vec<(SparseAt, &str, &str)> = vec![
        (cia(0x0004_001B_0001_0002, false, false), "cia", "s.cia"),
        (wad_head(0x0001_0002_4841_4241, 7), "wad", "s.wad"),
        (
            nsp_with_cnmt("SystemProgram", "0100000000000809", 0),
            "nsp",
            "s.nsp",
        ),
    ];
    for (mut src, ext, name) in cases {
        let len = src.len;
        let s = sniff_named(&mut src, len, ext, name);
        assert_eq!(s.not_a_rom, Some(NotARom::ConsoleSystem), "{name}");
        assert!(s.console.is_none(), "{name}");
    }
    // A game WAD and a 3DS game stay games.
    let mut src = wii_wad(0x0001_0001_5741_4145);
    let len = src.len;
    assert_eq!(sniff_named(&mut src, len, "wad", "g.wad").not_a_rom, None);
}
