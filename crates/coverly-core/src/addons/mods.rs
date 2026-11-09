//! Mod folders and texture packs (mods spec D4), key material first (D18).
//!
//! A mod is usually a **folder** moved whole as one unit. It is told by
//! its name, its parents' names, its listing and, for a few shapes, one
//! small plaintext manifest (`rules.txt`, `hires.txt`, `metadata.json`,
//! `riivolution/*.xml`). [`sniff_mod_dir`] is pure over that view; the
//! only fs-touching function is [`sniff_mod_dir_on_disk`].
//!
//! Every listed name is checked with [`is_key_material`] **before** any
//! small file is read: a folder holding a ticket, a licence or a key file
//! is [`ModVerdict::HoldsKeys`] and is never moved, and the key file is
//! never opened.

use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::Path;

use super::{one_component, valid_build_id, valid_game_id, valid_sony_serial, valid_title_id16};
use super::{AddonFormat, AddonKey, AddonKind};
use crate::consoles::is_key_material;

/// One directory as the walk sees it: its name, its parent names (nearest
/// first), its direct entries `(name, is_dir)` and, for listed
/// sub-folders, their entries one level down.
pub struct DirView<'a> {
    pub name: &'a str,
    pub parents: &'a [&'a str],
    pub entries: &'a [(String, bool)],
    pub children: &'a [(String, Vec<(String, bool)>)],
}

/// Capped small text files the sniff asks for (`rules.txt`, `hires.txt`,
/// `metadata.json`, `riivolution/*.xml`), by path relative to the
/// directory with `/` separators. `None` when missing or unreadable.
pub type SmallFiles<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What a directory is to the addon detection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModVerdict {
    /// A mod or texture pack, moved whole. `kind` is the format's kind,
    /// except a Cemu graphic pack named like a cheat (D4).
    Mod {
        format: AddonFormat,
        kind: AddonKind,
        key: AddonKey,
    },
    /// The folder holds key material by name: never moved, never read
    /// (D18).
    HoldsKeys,
    /// Not a mod this module knows (stays as today).
    NotAMod,
}

/// An MSU-1 set: `<name>.msu`, its `<name>-<N>.pcm` tracks (by track
/// number) and a same-named `.bml`, travelling as one unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msu1Set {
    pub name: String,
    pub files: Vec<String>,
}

/// Small text files are read up to this many bytes (UTF-8 lossy).
pub const SMALL_FILE_CAP: u64 = 1 << 20;

/// [`sniff_mod_dir_on_disk`] gives up (as [`ModVerdict::NotAMod`]) past
/// this many names in a folder tree: an unknown folder stays where it is.
pub const MAX_NAMES: usize = 200_000;

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn all_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Four letters and five digits without a dash (`CUSA00900`,
/// `PCSE00001`), upper-cased.
fn compact_serial(s: &str) -> Option<String> {
    let b = s.as_bytes();
    (b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit))
    .then(|| s.to_ascii_uppercase())
}

fn ext_is(name: &str, exts: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && exts.iter().any(|e| eq(ext, e)))
}

fn has_file(entries: &[(String, bool)], name: &str) -> bool {
    entries.iter().any(|(n, dir)| !dir && eq(n, name))
}

fn has_dir(entries: &[(String, bool)], name: &str) -> bool {
    entries.iter().any(|(n, dir)| *dir && eq(n, name))
}

/// A Dolphin (or Azahar) custom texture: `tex1_…` `.png`/`.dds`.
fn is_tex1(name: &str) -> bool {
    name.len() > 5
        && name
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("tex1_"))
        && ext_is(name, &["png", "dds"])
}

/// `tex1_…` files directly in the folder or one sub-folder down (packs
/// are often sorted into `HUD/`, `Characters/`…).
fn holds_tex1(dir: &DirView<'_>) -> bool {
    let tex1 = |e: &[(String, bool)]| e.iter().any(|(n, d)| !d && is_tex1(n));
    tex1(dir.entries)
        || dir
            .children
            .iter()
            .any(|(n, e)| has_dir(dir.entries, n) && tex1(e))
}

fn one_or_many(ids: BTreeSet<String>, one: impl FnOnce(String) -> AddonKey) -> AddonKey {
    match ids.len() {
        0 => AddonKey::None,
        1 => ids.into_iter().next().map_or(AddonKey::None, one),
        count => AddonKey::Many { count },
    }
}

fn verdict(format: AddonFormat, key: AddonKey) -> ModVerdict {
    ModVerdict::Mod {
        format,
        kind: format.kind(),
        key,
    }
}

/// D4 over one directory. Key material in any listed name wins first
/// (D18): the `small` closure is never called for such a folder.
pub fn sniff_mod_dir(dir: &DirView<'_>, small: SmallFiles<'_>) -> ModVerdict {
    let names = dir
        .entries
        .iter()
        .chain(dir.children.iter().flat_map(|(_, e)| e.iter()))
        .map(|(n, _)| n.as_str())
        .chain(dir.children.iter().map(|(n, _)| n.as_str()));
    for name in names {
        if is_key_material(name) {
            return ModVerdict::HoldsKeys;
        }
    }
    let Some(name) = one_component(dir.name) else {
        return ModVerdict::NotAMod;
    };
    let parent = |i: usize| dir.parents.get(i).copied().unwrap_or("");
    let entries = dir.entries;

    // shadPS4: `<CUSA>-mods/` beside its base (batch 3 D11 overlay).
    if let Some(id) = name
        .len()
        .checked_sub("-mods".len())
        .filter(|&n| name.get(n..).is_some_and(|t| eq(t, "-mods")))
        .and_then(|n| name.get(..n))
        .and_then(compact_serial)
    {
        return verdict(AddonFormat::Shadps4Mods, AddonKey::TitleId { id });
    }
    // Vita `rePatch/<TITLEID>/`, `reAddcont/<TITLEID>/`.
    if eq(parent(0), "rePatch") || eq(parent(0), "reAddcont") {
        if let Some(id) = compact_serial(name) {
            return verdict(AddonFormat::VitaRepatch, AddonKey::TitleId { id });
        }
    }
    // Xenia `plugins/<TITLEID8>/plugins.toml`.
    if eq(parent(0), "plugins") && name.len() == 8 && all_hex(name) {
        if has_file(entries, "plugins.toml") {
            let id = name.to_ascii_uppercase();
            return verdict(AddonFormat::XeniaPlugin, AddonKey::TitleId { id });
        }
        return ModVerdict::NotAMod;
    }
    // Switch `exefs_patches/<Set>/<BuildId>.ips`.
    if eq(parent(0), "exefs_patches") {
        let ids: BTreeSet<String> = entries
            .iter()
            .filter(|(n, dir)| !dir && ext_is(n, &["ips"]))
            .filter_map(|(n, _)| n.rsplit_once('.').and_then(|(s, _)| valid_build_id(s)))
            .collect();
        if ids.is_empty() {
            return ModVerdict::NotAMod;
        }
        let key = one_or_many(ids, |build_id| AddonKey::BuildId {
            build_id,
            title_id: None,
        });
        return verdict(AddonFormat::ExefsPatches, key);
    }
    // Dolphin `GraphicMods/<Name>/metadata.json` (+ `<GAMEID>.txt`).
    if eq(parent(0), "GraphicMods") && has_file(entries, "metadata.json") {
        return verdict(
            AddonFormat::DolphinGraphicMod,
            graphic_mod_key(entries, small),
        );
    }
    // Riivolution: the pack root holding `riivolution/<x>.xml`.
    if has_dir(entries, "riivolution") {
        if let Some(key) = riivolution_key(dir.children, small) {
            return verdict(AddonFormat::Riivolution, key);
        }
    }
    // Cemu graphic pack: `rules.txt` with `[Definition] titleIds = …`.
    if has_file(entries, "rules.txt") {
        if let Some((ids, cheat)) = small("rules.txt").as_deref().and_then(cemu_rules) {
            let kind = if cheat {
                AddonKind::Cheat
            } else {
                AddonKind::Mod
            };
            return ModVerdict::Mod {
                format: AddonFormat::CemuGraphicPack,
                kind,
                key: AddonKey::TitleIds { ids },
            };
        }
    }
    // Mesen HD pack: `hires.txt` with `<ver>`, named like the ROM.
    if has_file(entries, "hires.txt")
        && small("hires.txt").is_some_and(|t| t.to_ascii_lowercase().contains("<ver>"))
    {
        let key = AddonKey::RomName {
            name: name.to_string(),
        };
        return verdict(AddonFormat::MesenHdPack, key);
    }
    // 16-hex title ids: Switch (`01…`) and 3DS (`0004…`).
    if let Some(id) = valid_title_id16(name) {
        return title_id_dir(id, dir);
    }
    // PPSSPP `<GAMEID>/textures.ini` or `textures.zip`.
    if has_file(entries, "textures.ini") || has_file(entries, "textures.zip") {
        if let Some(id) = valid_game_id(name).filter(|id| id.len() == 9) {
            return verdict(AddonFormat::TexturePack, AddonKey::GameId { id });
        }
    }
    let under_textures = eq(parent(0), "textures");
    // DuckStation / PCSX2: `<SERIAL>/replacements|dumps`, `textures/<SERIAL>/`.
    if let Some(serial) = valid_sony_serial(name).filter(|_| name.contains(['-', '_'])) {
        let pack = has_dir(entries, "replacements")
            || has_dir(entries, "dumps")
            || (under_textures && !entries.is_empty());
        return if pack {
            verdict(AddonFormat::TexturePack, AddonKey::Serial { serial })
        } else {
            ModVerdict::NotAMod
        };
    }
    // Dolphin `<GAMEID>/tex1_*`, Flycast `textures/<GAMEID>/*.png`.
    if let Some(id) = valid_game_id(name).filter(|id| id.len() != 9) {
        let dolphin = holds_tex1(dir);
        let flycast = under_textures
            && id.len() == 6
            && entries
                .iter()
                .any(|(n, dir)| !dir && ext_is(n, &["png", "dds"]));
        if dolphin || flycast {
            return verdict(AddonFormat::TexturePack, AddonKey::GameId { id });
        }
    }
    // N64 Rice pack: `<INTERNAL NAME>/<INTERNAL NAME>#<CRC>#…png`.
    let prefix = format!("{name}#");
    if entries.iter().any(|(n, dir)| {
        !dir && n.len() > prefix.len()
            && n.is_char_boundary(prefix.len())
            && n[..prefix.len()].eq_ignore_ascii_case(&prefix)
            && ext_is(n, &["png"])
    }) {
        let key = AddonKey::N64Name {
            name: name.to_string(),
        };
        return verdict(AddonFormat::N64HiresTextures, key);
    }
    ModVerdict::NotAMod
}

/// A 16-hex title id folder: Switch LayeredFS mods (also one mod-name
/// level down, yuzu family), 3DS LayeredFS and textures. A Switch folder
/// that holds only `cheats/` is not a mod: its files are cheats (D3).
fn title_id_dir(id: String, dir: &DirView<'_>) -> ModVerdict {
    let entries = dir.entries;
    if id.starts_with("01") {
        let is_mod = |e: &[(String, bool)]| {
            has_dir(e, "romfs")
                || has_dir(e, "exefs")
                || has_file(e, "romfs.bin")
                || has_file(e, "exefs.nsp")
        };
        let nested = dir
            .children
            .iter()
            .any(|(n, e)| !eq(n, "cheats") && has_dir(entries, n) && is_mod(e));
        return if is_mod(entries) || nested {
            verdict(AddonFormat::SwitchMod, AddonKey::TitleId { id })
        } else {
            ModVerdict::NotAMod
        };
    }
    if id.starts_with("0004") {
        if holds_tex1(dir) {
            return verdict(AddonFormat::TexturePack, AddonKey::TitleId { id });
        }
        let layered = ["romfs", "exefs", "romfs_ext"]
            .iter()
            .any(|d| has_dir(entries, d))
            || ["exheader.bin", "code.bin", "code.ips", "code.bps"]
                .iter()
                .any(|f| has_file(entries, f));
        if layered {
            return verdict(AddonFormat::LayeredFs, AddonKey::TitleId { id });
        }
    }
    ModVerdict::NotAMod
}

/// A 6-character Dolphin id written as such (upper-case and digits, with
/// a digit), so a quoted word in a manifest is not taken for one.
fn strict_game_id6(s: &str) -> Option<String> {
    let looks = s.len() == 6
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && s.bytes().any(|b| b.is_ascii_digit());
    looks.then(|| valid_game_id(s)).flatten()
}

/// Dolphin graphics mod: game ids from `<GAMEID>.txt` entries, else from
/// quoted ids in `metadata.json`.
fn graphic_mod_key(entries: &[(String, bool)], small: SmallFiles<'_>) -> AddonKey {
    let mut ids: BTreeSet<String> = entries
        .iter()
        .filter(|(n, dir)| !dir && ext_is(n, &["txt"]))
        .filter_map(|(n, _)| n.rsplit_once('.'))
        .filter_map(|(s, _)| {
            strict_game_id6(s).or_else(|| valid_game_id(s).filter(|i| i.len() == 3 && s == i))
        })
        .collect();
    if ids.is_empty() {
        if let Some(text) = small("metadata.json") {
            ids = text.split('"').filter_map(strict_game_id6).collect();
        }
    }
    one_or_many(ids, |id| AddonKey::GameId { id })
}

/// Riivolution: `<id game="RMC">` in the pack's `riivolution/*.xml`.
/// `None` when the folder lists no `.xml` there.
fn riivolution_key(
    children: &[(String, Vec<(String, bool)>)],
    small: SmallFiles<'_>,
) -> Option<AddonKey> {
    let (dir_name, listing) = children.iter().find(|(n, _)| eq(n, "riivolution"))?;
    let xmls: Vec<&String> = listing
        .iter()
        .filter(|(n, dir)| !dir && ext_is(n, &["xml"]) && one_component(n).is_some())
        .map(|(n, _)| n)
        .collect();
    if xmls.is_empty() {
        return None;
    }
    let mut ids = BTreeSet::new();
    for xml in xmls {
        let Some(text) = small(&format!("{dir_name}/{xml}")) else {
            continue;
        };
        for rest in text.split("<id").skip(1) {
            let Some(tag) = rest.split('>').next() else {
                continue;
            };
            let Some(after) = tag.split("game=").nth(1) else {
                continue;
            };
            let value = after
                .trim_start_matches(['"', '\''])
                .split(['"', '\''])
                .next()
                .unwrap_or("");
            if let Some(id) = valid_game_id(value).filter(|id| id.len() != 9) {
                ids.insert(id);
            }
        }
    }
    Some(one_or_many(ids, |id| AddonKey::GameId { id }))
}

/// `rules.txt` `[Definition]` `titleIds = a,b` (any spacing and case):
/// the valid 16-hex ids, in order, and whether its `name` or `path`
/// contains «cheat». `None` without a valid title id.
pub fn cemu_rules(text: &str) -> Option<(Vec<String>, bool)> {
    let mut in_definition = false;
    let mut ids: Vec<String> = Vec::new();
    let mut cheat = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_definition = eq(line, "[Definition]");
            continue;
        }
        if !in_definition || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if eq(k, "titleIds") {
            for id in v.split(',').filter_map(|i| valid_title_id16(i.trim())) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        } else if (eq(k, "name") || eq(k, "path")) && v.to_ascii_lowercase().contains("cheat") {
            cheat = true;
        }
    }
    (!ids.is_empty()).then_some((ids, cheat))
}

/// Whether the first bytes of a `.pcm` are an MSU-1 track (`MSU1`).
pub fn is_msu1_pcm(head: &[u8]) -> bool {
    head.starts_with(b"MSU1")
}

/// The N64 internal name of a GLideN64 compiled pack
/// (`<INTERNAL NAME>_HIRESTEXTURES.htc|hts`).
pub fn n64_htc_name(file_name: &str) -> Option<String> {
    let (stem, ext) = one_component(file_name)?.rsplit_once('.')?;
    if !(eq(ext, "htc") || eq(ext, "hts")) {
        return None;
    }
    let suffix = "_HIRESTEXTURES";
    let cut = stem.len().checked_sub(suffix.len())?;
    let tail = stem.get(cut..)?;
    let name = &stem[..cut];
    (eq(tail, suffix) && !name.trim().is_empty()).then(|| name.to_string())
}

/// MSU-1 sets in one listing: `<name>.msu` + `<name>-<N>.pcm` (+
/// `<name>.bml`). Returns the sets and the strays (`Partial`): a `.pcm`
/// without its `.msu`, or a `.msu` without any track.
pub fn msu1_sets(names: &[String]) -> (Vec<Msu1Set>, Vec<String>) {
    let split_ext = |n: &str| {
        n.rsplit_once('.')
            .map(|(s, e)| (s.to_string(), e.to_ascii_lowercase()))
    };
    let track = |stem: &str| -> Option<(String, u32)> {
        let (base, n) = stem.rsplit_once('-')?;
        (!base.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            .then(|| n.parse().ok().map(|n| (base.to_string(), n)))
            .flatten()
    };
    let mut sets = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut strays = Vec::new();
    for msu in names {
        let Some((stem, ext)) = split_ext(msu) else {
            continue;
        };
        if ext != "msu" || stem.is_empty() {
            continue;
        }
        let mut tracks: Vec<(u32, &String)> = names
            .iter()
            .filter_map(|n| {
                let (s, e) = split_ext(n)?;
                let (base, num) = track(&s)?;
                (e == "pcm" && base == stem).then_some((num, n))
            })
            .collect();
        if tracks.is_empty() {
            strays.push(msu.clone());
            continue;
        }
        tracks.sort();
        let mut files = vec![msu.clone()];
        files.extend(tracks.into_iter().map(|(_, n)| n.clone()));
        if let Some(bml) = names
            .iter()
            .find(|n| split_ext(n).is_some_and(|(s, e)| e == "bml" && s == stem))
        {
            files.push(bml.clone());
        }
        claimed.extend(files.iter().cloned());
        sets.push(Msu1Set { name: stem, files });
    }
    for n in names {
        let pcm_track = split_ext(n).is_some_and(|(s, e)| e == "pcm" && track(&s).is_some());
        if pcm_track && !claimed.contains(n) {
            strays.push(n.clone());
        }
    }
    (sets, strays)
}

/// Lists one directory as `(name, is_dir)`, symlinks as files (never
/// followed).
fn list(path: &Path) -> io::Result<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let is_dir = entry.file_type()?.is_dir();
        out.push((entry.file_name().to_string_lossy().into_owned(), is_dir));
    }
    out.sort();
    Ok(out)
}

/// Whether any name in the tree under `path` is key material, by name
/// only (nothing is opened). `None` past [`MAX_NAMES`] names, or when a
/// part of the tree cannot be listed (it may hide key material, D18).
fn tree_holds_keys(path: &Path) -> Option<bool> {
    let mut seen = 0usize;
    for entry in walkdir::WalkDir::new(path).min_depth(1).follow_links(false) {
        let entry = entry.ok()?;
        seen += 1;
        if seen > MAX_NAMES {
            return None;
        }
        if is_key_material(&entry.file_name().to_string_lossy()) {
            return Some(true);
        }
    }
    Some(false)
}

/// Reads a small file the sniff asked for: every component checked, key
/// material refused by name, opened through the logged ROM opener, at
/// most [`SMALL_FILE_CAP`] bytes, UTF-8 lossy.
fn read_small(dir: &Path, rel: &str) -> Option<String> {
    let mut path = dir.to_path_buf();
    for part in rel.split('/') {
        path.push(one_component(part)?);
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    if is_key_material(&name) {
        return None;
    }
    let file = crate::consoles::open_rom(&path).ok()?;
    let mut bytes = Vec::new();
    file.take(SMALL_FILE_CAP).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// [`sniff_mod_dir`] on a real folder: lists it and its sub-folders one
/// level down, checks every name in its tree for key material first (by
/// name, nothing opened), then reads only the small files the sniff
/// asks for.
pub fn sniff_mod_dir_on_disk(path: &Path) -> io::Result<ModVerdict> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match tree_holds_keys(path) {
        Some(true) => return Ok(ModVerdict::HoldsKeys),
        Some(false) => {}
        None => return Ok(ModVerdict::NotAMod),
    }
    let parent_names: Vec<String> = path
        .ancestors()
        .skip(1)
        .filter_map(Path::file_name)
        .take(2)
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    let parents: Vec<&str> = parent_names.iter().map(String::as_str).collect();
    let entries = list(path)?;
    let mut children = Vec::new();
    for (child, is_dir) in &entries {
        if *is_dir {
            children.push((child.clone(), list(&path.join(child))?));
        }
    }
    let view = DirView {
        name: &name,
        parents: &parents,
        entries: &entries,
        children: &children,
    };
    Ok(sniff_mod_dir(&view, &|rel| read_small(path, rel)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multibyte_texture_name_does_not_panic() {
        // "te€1_x.png": byte 5 falls inside "€".
        for name in ["te€1_x.png", "€€.dds", "tex€.png"] {
            assert!(!is_tex1(name));
        }
    }

    fn e(list: &[&str]) -> Vec<(String, bool)> {
        list.iter()
            .map(|n| match n.strip_suffix('/') {
                Some(d) => (d.to_string(), true),
                None => (n.to_string(), false),
            })
            .collect()
    }

    fn no_reads(p: &str) -> Option<String> {
        panic!("read {p}")
    }

    fn sniff(name: &str, parents: &[&str], entries: &[&str]) -> ModVerdict {
        let entries = e(entries);
        let dir = DirView {
            name,
            parents,
            entries: &entries,
            children: &[],
        };
        sniff_mod_dir(&dir, &no_reads)
    }

    fn m(format: AddonFormat, key: AddonKey) -> ModVerdict {
        verdict(format, key)
    }

    fn title(id: &str) -> AddonKey {
        AddonKey::TitleId { id: id.into() }
    }

    fn game(id: &str) -> AddonKey {
        AddonKey::GameId { id: id.into() }
    }

    use AddonFormat as F;

    #[test]
    fn serial_texture_packs() {
        let want = m(
            F::TexturePack,
            AddonKey::Serial {
                serial: "SLUS-00594".into(),
            },
        );
        assert_eq!(sniff("SLUS-00594", &[], &["replacements/"]), want);
        assert_eq!(sniff("SLUS-00594", &[], &["dumps/"]), want);
        assert_eq!(
            sniff("SLUS-00594", &["textures", "duckstation"], &["a1b2.png"]),
            want
        );
        assert_eq!(sniff("SLUS_005.94", &[], &["replacements/"]), want);
        assert_eq!(
            sniff("SLUS-00594", &[], &["notes.txt"]),
            ModVerdict::NotAMod
        );
    }

    #[test]
    fn dolphin_textures_graphic_mods_and_flycast() {
        assert_eq!(
            sniff("GALE01", &["Textures", "Load"], &["tex1_64x64_abc_0.png"]),
            m(F::TexturePack, game("GALE01"))
        );
        assert_eq!(
            sniff("GAL", &[], &["tex1_64x64_abc_0.dds"]),
            m(F::TexturePack, game("GAL"))
        );
        assert_eq!(
            sniff("17707N", &["textures", "dc"], &["0a1b2c3d.png"]),
            m(F::TexturePack, game("17707N"))
        );
        assert_eq!(sniff("GALE01", &[], &["readme.txt"]), ModVerdict::NotAMod);

        let entries = e(&["metadata.json", "bloom.png"]);
        let dir = DirView {
            name: "Bloom",
            parents: &["GraphicMods", "Load"],
            entries: &entries,
            children: &[],
        };
        let small = |p: &str| {
            assert_eq!(p, "metadata.json");
            Some(r#"{"meta":{"title":"Bloom","author":"x"},"games":["GALE01"]}"#.to_string())
        };
        assert_eq!(
            sniff_mod_dir(&dir, &small),
            m(F::DolphinGraphicMod, game("GALE01"))
        );
        let entries = e(&["metadata.json", "GALE01.txt", "GALP01.txt"]);
        let dir = DirView {
            entries: &entries,
            ..dir
        };
        assert_eq!(
            sniff_mod_dir(&dir, &no_reads),
            m(F::DolphinGraphicMod, AddonKey::Many { count: 2 })
        );
    }

    #[test]
    fn riivolution_pack_root() {
        let entries = e(&["riivolution/", "MyMod/"]);
        let children = vec![
            ("riivolution".to_string(), e(&["mymod.xml"])),
            ("MyMod".to_string(), e(&["a.szs"])),
        ];
        let dir = DirView {
            name: "MyModPack",
            parents: &[],
            entries: &entries,
            children: &children,
        };
        let small = |p: &str| {
            assert_eq!(p, "riivolution/mymod.xml");
            Some(r#"<wiidisc version="1"><id game="RMC"/><patch id="x"/></wiidisc>"#.to_string())
        };
        assert_eq!(sniff_mod_dir(&dir, &small), m(F::Riivolution, game("RMC")));
        // Without a listed xml, it is not a Riivolution pack.
        let children = vec![("riivolution".to_string(), e(&[]))];
        let dir = DirView {
            children: &children,
            ..dir
        };
        assert_eq!(sniff_mod_dir(&dir, &no_reads), ModVerdict::NotAMod);
    }

    /// Dolphin and Azahar packs are often sorted into sub-folders
    /// (`GALE01/HUD/tex1_….png`): one level down counts too.
    #[test]
    fn texture_packs_sorted_into_sub_folders() {
        let entries = e(&["HUD/", "readme.txt"]);
        let children = vec![("HUD".to_string(), e(&["tex1_64x64_abc_0.png"]))];
        let dir = DirView {
            name: "GALE01",
            parents: &["Textures", "Load"],
            entries: &entries,
            children: &children,
        };
        assert_eq!(
            sniff_mod_dir(&dir, &no_reads),
            m(F::TexturePack, game("GALE01"))
        );
        let dir = DirView {
            name: "0004000000055D00",
            parents: &["textures", "load"],
            ..dir
        };
        assert_eq!(
            sniff_mod_dir(&dir, &no_reads),
            m(F::TexturePack, title("0004000000055D00"))
        );
        // A sub-folder without tex1 files is still nothing.
        let children = vec![("HUD".to_string(), e(&["a.png"]))];
        let dir = DirView {
            name: "GALE01",
            children: &children,
            ..dir
        };
        assert_eq!(sniff_mod_dir(&dir, &no_reads), ModVerdict::NotAMod);
    }

    #[test]
    fn ppsspp_textures() {
        assert_eq!(
            sniff("ULUS10041", &["TEXTURES"], &["textures.ini", "a.png"]),
            m(F::TexturePack, game("ULUS10041"))
        );
        assert_eq!(
            sniff("ULUS10041", &[], &["textures.zip"]),
            m(F::TexturePack, game("ULUS10041"))
        );
    }

    #[test]
    fn azahar_layered_fs_and_textures() {
        assert_eq!(
            sniff("0004000000055D00", &["mods", "load"], &["romfs/"]),
            m(F::LayeredFs, title("0004000000055D00"))
        );
        assert_eq!(
            sniff("0004000000055d00", &["titles", "luma"], &["code.ips"]),
            m(F::LayeredFs, title("0004000000055D00"))
        );
        assert_eq!(
            sniff(
                "0004000000055D00",
                &["textures", "load"],
                &["tex1_32x32_ff_0.png"]
            ),
            m(F::TexturePack, title("0004000000055D00"))
        );
        assert_eq!(
            sniff("0004000000055D00", &[], &["save.bin"]),
            ModVerdict::NotAMod
        );
    }

    #[test]
    fn switch_mods_and_cheats_only_folders() {
        assert_eq!(
            sniff("0100000000010000", &["contents", "atmosphere"], &["romfs/"]),
            m(F::SwitchMod, title("0100000000010000"))
        );
        assert_eq!(
            sniff("0100000000010000", &[], &["exefs.nsp"]),
            m(F::SwitchMod, title("0100000000010000"))
        );
        assert_eq!(
            sniff(
                "0100000000010000",
                &["contents", "atmosphere"],
                &["cheats/"]
            ),
            ModVerdict::NotAMod
        );
        // yuzu family: `load/<TitleID>/<ModName>/romfs`.
        let entries = e(&["60 FPS/"]);
        let children = vec![("60 FPS".to_string(), e(&["exefs/"]))];
        let dir = DirView {
            name: "0100000000010000",
            parents: &["load"],
            entries: &entries,
            children: &children,
        };
        assert_eq!(
            sniff_mod_dir(&dir, &no_reads),
            m(F::SwitchMod, title("0100000000010000"))
        );
    }

    #[test]
    fn exefs_patch_sets() {
        assert_eq!(
            sniff("MyPatch", &["exefs_patches"], &["A1B2C3D4E5F60718.ips"]),
            m(
                F::ExefsPatches,
                AddonKey::BuildId {
                    build_id: "A1B2C3D4E5F60718".into(),
                    title_id: None
                }
            )
        );
        assert_eq!(
            sniff(
                "Many",
                &["exefs_patches"],
                &["A1B2C3D4E5F60718.ips", "B1B2C3D4E5F60718.ips"]
            ),
            m(F::ExefsPatches, AddonKey::Many { count: 2 })
        );
        assert_eq!(
            sniff("Empty", &["exefs_patches"], &["readme.txt"]),
            ModVerdict::NotAMod
        );
    }

    #[test]
    fn cemu_graphic_packs() {
        let entries = e(&["rules.txt", "patches.txt"]);
        let dir = DirView {
            name: "BreathOfTheWild_60FPS",
            parents: &["graphicPacks"],
            entries: &entries,
            children: &[],
        };
        let rules = |t: &'static str| {
            move |p: &str| {
                assert_eq!(p, "rules.txt");
                Some(t.to_string())
            }
        };
        assert_eq!(
            sniff_mod_dir(
                &dir,
                &rules("[Definition]\ntitleIds = 00050000101C9400, 00050000101c9500\nname = \"60FPS\"\n")
            ),
            ModVerdict::Mod {
                format: F::CemuGraphicPack,
                kind: AddonKind::Mod,
                key: AddonKey::TitleIds {
                    ids: vec!["00050000101C9400".into(), "00050000101C9500".into()]
                }
            }
        );
        assert_eq!(
            sniff_mod_dir(
                &dir,
                &rules(
                    "[Definition]\nTitleIds=00050000101C9400\nname = \"Infinite Health Cheat\"\n"
                )
            ),
            ModVerdict::Mod {
                format: F::CemuGraphicPack,
                kind: AddonKind::Cheat,
                key: AddonKey::TitleIds {
                    ids: vec!["00050000101C9400".into()]
                }
            }
        );
        assert_eq!(
            sniff_mod_dir(&dir, &rules("[Definition]\nname = x\n")),
            ModVerdict::NotAMod
        );
    }

    #[test]
    fn cemu_rules_parse() {
        assert_eq!(
            cemu_rules("# x\n[Definition]\n  titleIds =  00050000101c9400 ,bad, 00050000101C9400\npath = \"Mods/Cheats/HP\"\n[Preset]\nname = cheat\n"),
            Some((vec!["00050000101C9400".into()], true))
        );
        assert_eq!(cemu_rules("[Preset]\ntitleIds = 00050000101C9400\n"), None);
    }

    #[test]
    fn vita_xenia_mesen_n64_and_shadps4() {
        assert_eq!(
            sniff("PCSE00001", &["rePatch", "ux0"], &["eboot.bin"]),
            m(F::VitaRepatch, title("PCSE00001"))
        );
        assert_eq!(
            sniff("pcse00001", &["reAddcont"], &["DLC1/"]),
            m(F::VitaRepatch, title("PCSE00001"))
        );
        assert_eq!(
            sniff("4D5307E6", &["plugins"], &["plugins.toml", "trainer.xex"]),
            m(F::XeniaPlugin, title("4D5307E6"))
        );
        assert_eq!(
            sniff("CUSA00900-mods", &[], &["dvdroot_ps4/"]),
            m(F::Shadps4Mods, title("CUSA00900"))
        );
        assert_eq!(
            sniff(
                "SUPER MARIO 64",
                &["hires_texture"],
                &["SUPER MARIO 64#4A3B2C1D#0#2_all.png"]
            ),
            m(
                F::N64HiresTextures,
                AddonKey::N64Name {
                    name: "SUPER MARIO 64".into()
                }
            )
        );
        let entries = e(&["hires.txt", "tiles.png"]);
        let dir = DirView {
            name: "Super Mario Bros. 3 (USA)",
            parents: &["HdPacks"],
            entries: &entries,
            children: &[],
        };
        let small = |p: &str| {
            assert_eq!(p, "hires.txt");
            Some("<ver>106\n<scale>2\n".to_string())
        };
        assert_eq!(
            sniff_mod_dir(&dir, &small),
            m(
                F::MesenHdPack,
                AddonKey::RomName {
                    name: "Super Mario Bros. 3 (USA)".into()
                }
            )
        );
        assert_eq!(
            n64_htc_name("SUPER MARIO 64_HIRESTEXTURES.htc").as_deref(),
            Some("SUPER MARIO 64")
        );
        assert_eq!(n64_htc_name("_HIRESTEXTURES.hts"), None);
        assert_eq!(n64_htc_name("Game.htc"), None);
    }

    #[test]
    fn mirrors_and_crafted_names_are_not_mods() {
        for (name, entries) in [
            ("USRDIR", &["EBOOT.BIN", "data/"][..]),
            ("dvdroot_ps4", &["eboot.bin"][..]),
            ("Content", &["0000000000000000/"][..]),
            ("BLUS30001", &["PS3_GAME/"][..]),
            ("..", &["replacements/"][..]),
            ("a/b", &["romfs/"][..]),
        ] {
            assert_eq!(sniff(name, &[], entries), ModVerdict::NotAMod, "{name}");
        }
    }

    #[test]
    fn a_mod_folder_with_a_key_is_holds_keys_and_reads_nothing() {
        let entries = vec![
            ("romfs".to_string(), true),
            ("title.tik".to_string(), false),
        ];
        let dir = DirView {
            name: "0100000000010000",
            parents: &["contents", "atmosphere"],
            entries: &entries,
            children: &[],
        };
        let small: SmallFiles<'_> = &|p| panic!("read {p}");
        assert!(matches!(sniff_mod_dir(&dir, small), ModVerdict::HoldsKeys));
        for key in ["title.keys", "prod.keys", "Game.rap"] {
            let entries = e(&["rules.txt", key]);
            let dir = DirView {
                name: "Pack",
                parents: &[],
                entries: &entries,
                children: &[],
            };
            assert_eq!(sniff_mod_dir(&dir, small), ModVerdict::HoldsKeys, "{key}");
        }
        // One level down too.
        let entries = e(&["romfs/"]);
        let children = vec![("romfs".to_string(), e(&["prod.keys"]))];
        let dir = DirView {
            name: "0100000000010000",
            parents: &[],
            entries: &entries,
            children: &children,
        };
        assert_eq!(sniff_mod_dir(&dir, small), ModVerdict::HoldsKeys);
    }

    #[test]
    fn msu1_sets_gather_their_tracks() {
        let names: Vec<String> = [
            "Game.sfc",
            "Game-2.pcm",
            "Game.msu",
            "Game-1.pcm",
            "Game.bml",
            "Other-1.pcm",
            "Lonely.msu",
            "Notes.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (sets, strays) = msu1_sets(&names);
        assert_eq!(
            sets,
            vec![Msu1Set {
                name: "Game".into(),
                files: vec![
                    "Game.msu".into(),
                    "Game-1.pcm".into(),
                    "Game-2.pcm".into(),
                    "Game.bml".into()
                ]
            }]
        );
        assert_eq!(
            strays,
            vec!["Lonely.msu".to_string(), "Other-1.pcm".to_string()]
        );
        assert!(is_msu1_pcm(b"MSU1\0\0\0\0"));
        assert!(!is_msu1_pcm(b"RIFF"));
    }
}
