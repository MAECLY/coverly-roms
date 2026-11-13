//! What the game panel reads lazily when it opens (part C D3): the title
//! id, the product code and the 3DS region verdict of a game, the title id
//! and kind of each of its updates and DLC, and what an offline title list
//! knows about its newest version (D6).
//!
//! **Offline only.** Nothing here talks to a network: the newest version
//! comes from a [`LatestVersions`] (the 3.1 seam, [`NoTitleList`] in part C),
//! headers from the budgeted console readers or from what Organizar
//! already memoised for the same file, size and mtime. No key, ticket or
//! licence file is ever opened (`consoles::is_key_material`, checked by
//! name before any read).
//!
//! [`NoTitleList`]: coverly_core::catalog::latest::NoTitleList

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use coverly_core::cache::Cache;
use coverly_core::catalog::latest::{has_latest, LatestKnown, LatestVersions};
use coverly_core::consoles::ctr_region::{ctr_region, Certainty};
use coverly_core::consoles::{base_title_id, is_key_material, ConsoleId, TitleKind};
use coverly_core::identify::names::Region;
use coverly_core::organize::plan::memoised_console;
use coverly_core::sources::embedded::console_on_disk;
use serde::Serialize;

use super::art::inside_root;
use super::{installed_of, Session};
use crate::error::IpcError;

/// The 3DS region verdict of a product code, as the panel shows it
/// («Región: Europa, España», «Letra de región «T» sin identificar»).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RegionVerdictView {
    /// The last letter of the game code (`E`, `P`, `T`…).
    pub letter: String,
    /// The product code as read, trimmed and upper-cased.
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<Region>,
    pub certainty: Certainty,
}

/// What the headers say about one update or DLC of the game.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ChildDetailView {
    /// The child's position in the game's children (`ChildView.index`).
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_id: Option<String>,
    /// `update`, `dlc`, `demo`… when the header says; never `unknown`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TitleKind>,
}

/// The answer of `game_details`: everything is optional, an unreadable or
/// missing file gives empty facts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GameDetailsView {
    /// The webview's request id, echoed: an answer for an older request
    /// (another game opened meanwhile) is dropped by the webview.
    pub request: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_id: Option<String>,
    /// The serial or product code the header names (`CTR-P-AXXE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_code: Option<String>,
    /// 3DS only (part C §5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionVerdictView>,
    /// Only when a title list answered (never invented, D6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<LatestKnown>,
    /// With `latest`: whether the library already holds that version
    /// (`version_order`, D5); left out when nothing can be claimed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_installed: Option<bool>,
    /// One per child, in `RomEntry.children` order.
    pub children: Vec<ChildDetailView>,
}

/// The lazy facts of entry `id` of `session`. Takes no network client:
/// `latest` is an offline title list and `cache` the local organize facts
/// memo (`None` reads every header again).
pub fn game_details(
    session: &Session,
    id: u32,
    request: u32,
    latest: &dyn LatestVersions,
    cache: Option<&Cache>,
) -> Result<GameDetailsView, IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    let mut view = GameDetailsView {
        request,
        ..GameDetailsView::default()
    };
    if let Some(console) = read_console(session, &entry.rom.path, cache) {
        view.title_id = console.title_id.clone();
        view.product_code = console.serial.clone();
        if console.system == "3ds" {
            view.region = console.serial.as_deref().and_then(region_view);
        }
        let system = if entry.system_id.is_empty() {
            console.system.as_str()
        } else {
            entry.system_id.as_str()
        };
        view.latest = base_title_id(&console).and_then(|tid| latest.latest(system, &tid));
        view.latest_installed = view.latest.as_ref().and_then(|known| {
            has_latest(
                system,
                installed_of(system, &entry.rom).as_deref(),
                &known.version,
            )
        });
    }
    view.children = entry
        .rom
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let console = read_console(session, &child.path, cache);
            ChildDetailView {
                index: index as u32,
                title_id: console.as_ref().and_then(|c| c.title_id.clone()),
                kind: console.map(|c| c.kind).filter(|k| *k != TitleKind::Unknown),
            }
        })
        .collect();
    Ok(view)
}

fn region_view(code: &str) -> Option<RegionVerdictView> {
    let v = ctr_region(code)?;
    Some(RegionVerdictView {
        letter: v.letter.to_string(),
        code: v.code,
        region: v.region,
        country: v.country,
        certainty: v.certainty,
    })
}

/// The console verdict of the file or folder at `path`: never a key file
/// (refused by name, before anything is opened), never outside the
/// library; Organizar's memo first, else the budgeted readers.
fn read_console(session: &Session, path: &Path, cache: Option<&Cache>) -> Option<ConsoleId> {
    let name = path.file_name()?.to_string_lossy();
    if is_key_material(&name) {
        return None;
    }
    let canon = inside_root(session, path).ok()?;
    let meta = fs::metadata(&canon).ok()?;
    if meta.is_file() {
        if let Some(cache) = cache {
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_nanos()).ok());
            if let Some(found) =
                mtime_ns.and_then(|ns| memoised_console(cache, path, meta.len(), ns))
            {
                return Some(found);
            }
        }
    }
    console_on_disk(&canon)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use coverly_core::catalog::latest::{LatestSource, NoTitleList};
    use coverly_core::consoles::testing as ct;
    use coverly_core::consoles::{start_open_log, take_open_log};
    use coverly_core::dedupe::testing::pin_mtime;
    use coverly_core::game_folder::ChildRole;
    use coverly_core::scan::{ChildEntry, ScanOptions};
    use coverly_core::systems::SystemDb;
    use tempfile::TempDir;

    use super::*;
    use crate::library::{open_session, OpenRequest};

    /// Answers `1.3.0` for every title.
    struct Knows;
    impl LatestVersions for Knows {
        fn latest(&self, _system: &str, _base_title_id: &str) -> Option<LatestKnown> {
            Some(LatestKnown {
                version: "1.3.0".into(),
                source: LatestSource::TitleDb,
            })
        }
    }

    /// Writes `bytes` at `rel` (`/`-separated, joined by components) with a
    /// pinned mtime.
    fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        pin_mtime(&p, 1);
        p
    }

    fn open(root: &Path) -> Session {
        open_session(
            &OpenRequest {
                path: root,
                profile_id: Some("coverly-library"),
                scan: &ScanOptions::default(),
                generation: 1,
                custom: &[],
                default_profile: None,
                settings: None,
            },
            &SystemDb::builtin(),
            &mut |_| {},
        )
        .unwrap()
        .0
    }

    fn id_of(session: &Session, stem: &str) -> u32 {
        session
            .entries
            .iter()
            .position(|e| e.rom.stem == stem)
            .unwrap_or_else(|| panic!("no entry {stem}")) as u32
    }

    const TID: u64 = 0x0004_0000_0005_5E00;

    #[test]
    fn game_details_reads_the_3ds_verdict() {
        let lib = TempDir::new().unwrap();
        put(
            lib.path(),
            "ROMS/3DS/Kite Days.3ds",
            &ct::ncsd("CTR-P-AXXT", TID, true).head_bytes(),
        );
        let session = open(lib.path());
        let id = id_of(&session, "Kite Days");

        let view = game_details(&session, id, 4, &NoTitleList, None).unwrap();
        assert_eq!(view.request, 4);
        assert_eq!(view.product_code.as_deref(), Some("CTR-P-AXXT"));
        assert_eq!(view.title_id.as_deref(), Some("0004000000055E00"));
        let region = view.region.as_ref().unwrap();
        assert_eq!(region.certainty, Certainty::Unknown);
        assert_eq!(region.letter, "T");
        assert_eq!((region.region, region.country), (None, None));
        assert_eq!(view.latest, None);
        assert!(view.children.is_empty());

        let view = game_details(&session, id, 5, &Knows, None).unwrap();
        assert_eq!(
            view.latest,
            Some(LatestKnown {
                version: "1.3.0".into(),
                source: LatestSource::TitleDb,
            })
        );
        assert_eq!(
            serde_json::to_value(&view).unwrap()["latest"],
            serde_json::json!({ "version": "1.3.0", "source": "title-db" })
        );
    }

    /// With a title list, Rust says whether the library holds its newest
    /// version (one ordering, D5); nothing is claimed without a version.
    #[test]
    fn game_details_says_whether_the_latest_is_installed() {
        let lib = TempDir::new().unwrap();
        let head = ct::ncsd("CTR-P-AXXE", TID, true).head_bytes();
        put(lib.path(), "ROMS/3DS/Kite Days (v1.3.0).3ds", &head);
        put(lib.path(), "ROMS/3DS/Moss Garden (v1.0).3ds", &head);
        put(lib.path(), "ROMS/3DS/Paper Skies.3ds", &head);
        let session = open(lib.path());

        let at =
            |stem: &str| game_details(&session, id_of(&session, stem), 1, &Knows, None).unwrap();
        let has = at("Kite Days (v1.3.0)");
        assert_eq!(has.latest_installed, Some(true));
        assert_eq!(
            serde_json::to_value(&has).unwrap()["latest_installed"],
            serde_json::json!(true)
        );
        assert_eq!(at("Moss Garden (v1.0)").latest_installed, Some(false));
        let none = at("Paper Skies");
        assert!(none.latest.is_some());
        assert_eq!(none.latest_installed, None);
        assert!(serde_json::to_value(&none)
            .unwrap()
            .get("latest_installed")
            .is_none());

        let quiet = game_details(
            &session,
            id_of(&session, "Kite Days (v1.3.0)"),
            2,
            &NoTitleList,
            None,
        )
        .unwrap();
        assert_eq!((quiet.latest, quiet.latest_installed), (None, None));
    }

    type DetailsFn = fn(
        &Session,
        u32,
        u32,
        &dyn LatestVersions,
        Option<&Cache>,
    ) -> Result<GameDetailsView, IpcError>;

    /// Compile-level: the only inputs are the session, an offline title
    /// list and the local memo; there is no client to pass.
    #[test]
    fn game_details_is_offline() {
        let lib = TempDir::new().unwrap();
        put(lib.path(), "ROMS/3DS/Moss Garden.3ds", &[0u8; 64]);
        let session = open(lib.path());
        let call: DetailsFn = game_details;
        let view = call(&session, 0, 1, &NoTitleList, None).unwrap();
        assert_eq!(view.title_id, None);
    }

    #[test]
    fn game_details_with_a_missing_file() {
        let lib = TempDir::new().unwrap();
        let rom = put(lib.path(), "ROMS/3DS/Pebble League.3ds", &[0u8; 64]);
        let session = open(lib.path());
        fs::remove_file(&rom).unwrap();
        let view = game_details(&session, 0, 2, &Knows, None).unwrap();
        assert_eq!(
            view,
            GameDetailsView {
                request: 2,
                ..GameDetailsView::default()
            }
        );
        assert_eq!(
            game_details(&session, 99, 2, &Knows, None).unwrap_err(),
            IpcError::UnknownEntry(99)
        );
    }

    #[test]
    fn game_details_reads_children_and_never_opens_a_key() {
        let lib = TempDir::new().unwrap();
        let g = "ROMS/3DS/Lantern Harbor";
        let cia = |tid: u64| ct::cia(tid, false, false).head_bytes();
        put(lib.path(), &format!("{g}/Lantern Harbor.cia"), &cia(TID));
        put(
            lib.path(),
            &format!("{g}/Updates/Lantern Harbor (v1.2).cia"),
            &cia(0x0004_000E_0005_5E00),
        );
        let key = put(
            lib.path(),
            &format!("{g}/DLC/Lantern Harbor.tik"),
            &[0xEE; 64],
        );
        let mut session = open(lib.path());
        let id = id_of(&session, "Lantern Harbor");
        // The scan never lists a ticket as a child; one handed in anyway
        // (an older scan, a hand-made folder) must still not be opened.
        let children = &mut session.entries[id as usize].rom.children;
        children.push(ChildEntry {
            path: key.clone(),
            role: ChildRole::Dlc,
            size: 64,
            modified: SystemTime::UNIX_EPOCH,
        });
        let children = session.entries[id as usize].rom.children.clone();
        let k = children.len() - 1;

        start_open_log();
        let view = game_details(&session, id, 1, &NoTitleList, None).unwrap();
        let opened = take_open_log();
        // The log sees the reads: the game itself was opened…
        assert!(
            opened
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "Lantern Harbor.cia")),
            "{opened:?}"
        );
        // …and the ticket was not.
        assert!(
            !opened.iter().any(|p| p.file_name() == key.file_name()),
            "{opened:?}"
        );
        let update = children
            .iter()
            .position(|c| c.path.to_string_lossy().contains("v1.2"))
            .unwrap();
        assert_eq!(view.children.len(), children.len());
        let u = &view.children[update];
        assert_eq!(u.index, update as u32);
        assert_eq!(u.title_id.as_deref(), Some("0004000E00055E00"));
        assert_eq!(u.kind, Some(TitleKind::Update));
        assert_eq!(
            view.children[k],
            ChildDetailView {
                index: k as u32,
                ..ChildDetailView::default()
            }
        );
        // A key named as the base is never opened either.
        assert_eq!(read_console(&session, &key, None), None);
        assert!(!take_open_log().iter().any(|p| p == &key));
    }
}
