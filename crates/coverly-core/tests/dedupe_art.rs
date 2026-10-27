//! Art resolution of duplicate groups by plan ids, and the offline
//! libretro lookup (dedupe UI spec D4, D7, D16).

use std::path::{Path, PathBuf};

use coverly_core::cache::Cache;
use coverly_core::dedupe::art::{
    group_catalogue_name, group_cover_path, libretro_cached_url, member_art_path, OfflineHttp,
};
use coverly_core::dedupe::{
    ArtSeen, DedupePlan, DupGroup, GroupAction, KeepReason, Member, MemberFacts, Relation,
    UnitKind, UnitRef,
};
use coverly_core::jobs::ArtStore;
use coverly_core::net::{Http, HttpRequest, HttpResponse, NetError};
use coverly_core::profiles::Media;
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource};
use coverly_core::systems::SystemDb;

const GB: &str = "Nintendo - Game Boy";
const TETRIS: &str = "Tetris (World) (Rev 1)";
const TETRIS_URL: &str =
    "https://thumbnails.libretro.com/Nintendo%20-%20Game%20Boy/Named_Boxarts/Tetris%20(World)%20(Rev%201).png";

/// An `Http` that fails the test if anything asks the network.
struct PanicHttp;

impl Http for PanicHttp {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        panic!("network request during an offline lookup: {}", req.url)
    }
}

fn member(id: usize, primary: &str, catalogue: Option<&str>, art: Vec<ArtSeen>) -> Member {
    Member {
        id,
        unit: UnitRef {
            primary: PathBuf::from(primary),
            files: vec![PathBuf::from(primary)],
            kind: UnitKind::Single,
            mtime_ns: 0,
            in_library: true,
            mtimes_ns: Vec::new(),
        },
        relation: Relation::Exact,
        size: 32,
        travels: Vec::new(),
        stays: Vec::new(),
        dest: None,
        catalogue_name: catalogue.map(str::to_string),
        serials: Vec::new(),
        confidence: 1.0,
        gamelist: None,
        art_leaves: Default::default(),
        facts: MemberFacts {
            art,
            ..MemberFacts::default()
        },
    }
}

fn cover(path: &str, size: u64) -> ArtSeen {
    ArtSeen {
        media: Media::Cover,
        path: PathBuf::from(path),
        size,
    }
}

fn plan(keeper: usize, members: Vec<Member>) -> DedupePlan {
    DedupePlan {
        root: PathBuf::from("/lib"),
        groups: vec![DupGroup {
            id: 7,
            system: "gb".into(),
            title: "Tetris".into(),
            keeper,
            reason: KeepReason::InLibrary,
            runner_up: None,
            members,
            action: GroupAction::Act,
            tie: false,
            choice_dropped: false,
        }],
        ..DedupePlan::default()
    }
}

const M1_ART: &str = "/lib/ROMS/GB/Imgs/Tetris (1).png";

fn keeper_without_art() -> DedupePlan {
    plan(
        0,
        vec![
            member(0, "/lib/ROMS/GB/Tetris.gb", None, Vec::new()),
            member(
                1,
                "/lib/ROMS/GB/Tetris (1).gb",
                Some(TETRIS),
                vec![cover(M1_ART, 10)],
            ),
        ],
    )
}

#[test]
fn group_cover_falls_back_to_first_member_with_art() {
    let p = keeper_without_art();
    assert_eq!(
        group_cover_path(&p, 7, Media::Cover),
        Some(Path::new(M1_ART))
    );
    assert_eq!(member_art_path(&p, 7, 0, Media::Cover), None);
    assert_eq!(
        member_art_path(&p, 7, 1, Media::Cover),
        Some(Path::new(M1_ART))
    );
    assert_eq!(group_cover_path(&p, 8, Media::Cover), None, "unknown group");
    assert_eq!(
        member_art_path(&p, 8, 1, Media::Cover),
        None,
        "unknown group"
    );
    assert_eq!(
        member_art_path(&p, 7, 2, Media::Cover),
        None,
        "unknown member"
    );
    assert_eq!(group_cover_path(&p, 7, Media::Title), None, "other media");
    assert_eq!(member_art_path(&p, 7, 1, Media::Title), None, "other media");
}

#[test]
fn keeper_art_wins() {
    let mut p = keeper_without_art();
    p.groups[0].members[0].facts.art = vec![cover("/lib/ROMS/GB/Imgs/Tetris.png", 12)];
    assert_eq!(
        group_cover_path(&p, 7, Media::Cover),
        Some(Path::new("/lib/ROMS/GB/Imgs/Tetris.png"))
    );
    // The keeper is found by `Member::id`, not by position.
    p.groups[0].keeper = 1;
    assert_eq!(
        group_cover_path(&p, 7, Media::Cover),
        Some(Path::new(M1_ART))
    );
}

#[test]
fn group_catalogue_name_prefers_the_keeper() {
    let mut p = keeper_without_art();
    assert_eq!(group_catalogue_name(&p, 7), Some(("gb", TETRIS)));
    p.groups[0].members[0].catalogue_name = Some("Tetris (Japan)".into());
    assert_eq!(group_catalogue_name(&p, 7), Some(("gb", "Tetris (Japan)")));
    assert_eq!(group_catalogue_name(&p, 8), None);
    p.groups[0]
        .members
        .iter_mut()
        .for_each(|m| m.catalogue_name = None);
    assert_eq!(group_catalogue_name(&p, 7), None);
}

/// Seeds the root and `Named_Boxarts` listings the way `LibretroSource`
/// caches them.
fn seed_listings(cache: &Cache) {
    cache
        .put_json("libretro-root-2", "root", &vec![GB.to_string()], None)
        .unwrap();
    cache
        .put_json(
            "libretro-index-2",
            &format!("{GB}/Named_Boxarts"),
            &vec![
                "Tetris (Japan).png".to_string(),
                format!("{TETRIS}.png"),
                "Tetris Plus (USA).png".to_string(),
            ],
            None,
        )
        .unwrap();
}

fn stale() -> LibretroConfig {
    LibretroConfig {
        ttl_secs: 0,
        ..LibretroConfig::default()
    }
}

#[test]
fn libretro_cached_url_reads_only_the_store() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::in_memory().unwrap();
    seed_listings(&cache);
    let store = ArtStore::new(tmp.path(), &cache);
    store.put(TETRIS_URL, b"png").unwrap();
    let db = SystemDb::builtin();
    let gb = db.get("gb").unwrap();

    // A fresh listing: nothing reaches the network.
    let fresh = LibretroSource::new(&PanicHttp, &cache, LibretroConfig::default());
    assert_eq!(
        libretro_cached_url(&fresh, &store, gb, Media::Cover, TETRIS).as_deref(),
        Some(TETRIS_URL)
    );

    // A stale listing is still served when the network is off.
    let offline = LibretroSource::new(&OfflineHttp, &cache, stale());
    assert_eq!(
        libretro_cached_url(&offline, &store, gb, Media::Cover, TETRIS).as_deref(),
        Some(TETRIS_URL)
    );
}

#[test]
fn libretro_cached_url_takes_another_region_already_downloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::in_memory().unwrap();
    seed_listings(&cache);
    let store = ArtStore::new(tmp.path(), &cache);
    let japan = TETRIS_URL.replace("Tetris%20(World)%20(Rev%201)", "Tetris%20(Japan)");
    store.put(&japan, b"png").unwrap();
    let db = SystemDb::builtin();
    let src = LibretroSource::new(&OfflineHttp, &cache, stale());
    assert_eq!(
        libretro_cached_url(&src, &store, db.get("gb").unwrap(), Media::Cover, TETRIS),
        Some(japan),
        "same title, other region"
    );
}

#[test]
fn libretro_cached_url_never_matches_fuzzily() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::in_memory().unwrap();
    seed_listings(&cache);
    let store = ArtStore::new(tmp.path(), &cache);
    let plus = TETRIS_URL.replace("Tetris%20(World)%20(Rev%201)", "Tetris%20Plus%20(USA)");
    store.put(&plus, b"png").unwrap();
    let db = SystemDb::builtin();
    let src = LibretroSource::new(&OfflineHttp, &cache, stale());
    assert_eq!(
        libretro_cached_url(
            &src,
            &store,
            db.get("gb").unwrap(),
            Media::Cover,
            "Tetrs (USA)"
        ),
        None
    );
}

#[test]
fn libretro_cached_url_none_without_blob() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::in_memory().unwrap();
    seed_listings(&cache);
    let store = ArtStore::new(tmp.path(), &cache);
    let db = SystemDb::builtin();
    let src = LibretroSource::new(&OfflineHttp, &cache, stale());
    assert_eq!(
        libretro_cached_url(&src, &store, db.get("gb").unwrap(), Media::Cover, TETRIS),
        None
    );
}

#[test]
fn libretro_cached_url_none_without_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::in_memory().unwrap();
    let store = ArtStore::new(tmp.path(), &cache);
    store.put(TETRIS_URL, b"png").unwrap();
    let db = SystemDb::builtin();
    let src = LibretroSource::new(&OfflineHttp, &cache, stale());
    assert_eq!(
        libretro_cached_url(&src, &store, db.get("gb").unwrap(), Media::Cover, TETRIS),
        None
    );
}

#[test]
fn offline_http_never_answers() {
    let err = OfflineHttp
        .get(&HttpRequest::get("https://thumbnails.libretro.com/"))
        .unwrap_err();
    assert!(matches!(err, NetError::Offline(_)));
    assert!(!matches!(err, NetError::RateLimited { .. }));
}
