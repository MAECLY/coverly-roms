//! The art a duplicates screen shows (dedupe UI spec D4, D7, D16): a
//! group's cover or a copy's own art, found from the plan alone (the art
//! paths were recorded in [`MemberFacts::art`](super::MemberFacts) at plan
//! time), and an already downloaded libretro-thumbnails image, looked up
//! without ever reaching the network.
//!
//! Nothing here reads an image or checks a path against the library root:
//! the caller canonicalises and caps what it serves.

use std::path::Path;

use super::{DedupePlan, DupGroup, Member};
use crate::identify::names::parse_name;
use crate::jobs::ArtStore;
use crate::net::{Http, HttpRequest, HttpResponse, NetError};
use crate::profiles::Media;
use crate::sources::{ArtQuery, ArtSource, MatchKind};
use crate::systems::System;

fn group(plan: &DedupePlan, group: u64) -> Option<&DupGroup> {
    plan.groups.iter().find(|g| g.id == group)
}

fn art_of(m: &Member, media: Media) -> Option<&Path> {
    m.facts
        .art
        .iter()
        .find(|a| a.media == media)
        .map(|a| a.path.as_path())
}

/// The keeper first, then the other members in `members` order.
fn keeper_first(g: &DupGroup) -> impl Iterator<Item = &Member> {
    let keeper = g.members.iter().find(|m| m.id == g.keeper);
    keeper
        .into_iter()
        .chain(g.members.iter().filter(move |m| m.id != g.keeper))
}

/// The art path a member recorded for `media` (spec D4, D7). `member` is
/// the [`Member::id`].
pub fn member_art_path(
    plan: &DedupePlan,
    group_id: u64,
    member: usize,
    media: Media,
) -> Option<&Path> {
    group(plan, group_id)?
        .members
        .iter()
        .find(|m| m.id == member)
        .and_then(|m| art_of(m, media))
}

/// The group cover: the keeper's `media`, else the first member (in
/// `members` order) with that media. Reads only `MemberFacts.art`.
pub fn group_cover_path(plan: &DedupePlan, group_id: u64, media: Media) -> Option<&Path> {
    keeper_first(group(plan, group_id)?).find_map(|m| art_of(m, media))
}

/// The catalogue name to look up on libretro for the group, with the
/// group's system id: the keeper's `catalogue_name`, else the first
/// member's that has one.
pub fn group_catalogue_name(plan: &DedupePlan, group_id: u64) -> Option<(&str, &str)> {
    let g = group(plan, group_id)?;
    keeper_first(g)
        .find_map(|m| m.catalogue_name.as_deref())
        .map(|name| (g.system.as_str(), name))
}

/// An `Http` that never reaches the network: every request fails with
/// `NetError::Offline`, which makes `LibretroSource` serve its cached
/// listing even when stale (`listing()` returns the stored copy for any
/// error but a rate limit).
#[derive(Clone, Copy, Debug, Default)]
pub struct OfflineHttp;

impl Http for OfflineHttp {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        Err(NetError::Offline(format!("offline lookup: {}", req.url)))
    }
}

/// The URL of the libretro candidate for `name` that is already in the art
/// store, if any (spec D16). Only canonical and exact-title candidates
/// count (the catalogue name, then the same title in another region); no
/// fuzzy matching. Never fetches an image: with `source` built over
/// [`OfflineHttp`] nothing reaches the network at all. Any source error
/// (no listing cached, system not on libretro) is `None`.
pub fn libretro_cached_url(
    source: &dyn ArtSource,
    store: &ArtStore,
    system: &System,
    media: Media,
    name: &str,
) -> Option<String> {
    let rom = parse_name(name, false);
    let query = ArtQuery {
        system,
        media,
        canonical: Some(name),
        rom: &rom,
        stem: name,
        by_name: true,
        fuzzy: false,
        min_score: 100,
        max_candidates: 1,
        hash: None,
        size: 0,
        file_name: name,
    };
    let candidates = source.candidates(&query).ok()?;
    candidates
        .iter()
        .filter(|c| c.kind != MatchKind::Fuzzy)
        .flat_map(|c| std::iter::once(&c.url).chain(c.fallback_url.iter()))
        .find(|url| store.get(url).is_some())
        .cloned()
}
