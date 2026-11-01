//! User-defined layout profiles (spec §5, D29): one TOML file per profile
//! in the custom profiles folder. Files are untrusted: size-capped,
//! validated, skipped with a warning when bad. Built-ins stay read-only.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::naming::validate_folder_override;
use super::{
    builtin_profiles, render, DiscLayout, FolderNaming, ImageFormat, Media, MediaSlot, Profile,
    ProfileFolder, TemplateVars,
};
pub use super::{default_accepts_containers, default_container_overrides};
use crate::archive::ContainerKind;
use crate::settings::{FieldError, FieldErrorCode};
use crate::systems::SystemDb;

/// Folder (inside the config dir) holding the profiles by default.
pub const PROFILES_DIR: &str = "profiles";
/// Larger files are not profiles.
pub const MAX_PROFILE_BYTES: u64 = 64 * 1024;
/// Template tokens (see [`render`]).
pub const TOKENS: [&str; 6] = [
    "{root}",
    "{system_dir}",
    "{rel_dir}",
    "{stem}",
    "{system_id}",
    "{libretro}",
];

fn default_format() -> ImageFormat {
    ImageFormat::Png
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotFile {
    pub template: String,
    #[serde(default = "default_format")]
    pub format: ImageFormat,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub roms_root: String,
    #[serde(default)]
    pub art_dirs: Vec<String>,
    #[serde(default)]
    pub gamelist: bool,
    /// Keyed by `cover`, `screenshot` or `title`.
    #[serde(default)]
    pub slots: BTreeMap<String, SlotFile>,
    /// How organise names system folders (`first-alias` | `id`).
    #[serde(default, skip_serializing_if = "is_default_naming")]
    pub system_folder: FolderNaming,
    /// Per-system folder names (system id → folder).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_names: BTreeMap<String, String>,
    /// Systems kept in another system's folder (system id → host id), as
    /// on Anbernic cards (`ngpc → ngp`): a copy of such a layout keeps them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub host_folders: BTreeMap<String, String>,
    /// Where disc games are extracted (spec §8.2), e.g.
    /// `[disc_layout]` `kind = "game-folder"` `m3u_for_multi = true`.
    #[serde(default, skip_serializing_if = "is_default_layout")]
    pub disc_layout: DiscLayout,
    /// Containers whose cartridge ROMs move as they are (`zip`, `7z`,
    /// `rar`, `tar.gz`, `gz`…; spec §8.3).
    #[serde(
        default = "default_accepts_containers",
        skip_serializing_if = "is_default_containers"
    )]
    pub accepts_containers: Vec<ContainerKind>,
    /// Per-system replacements of `accepts_containers` (system id →
    /// containers; `[]` = always extract). Starts from
    /// [`default_container_overrides`].
    #[serde(
        default = "default_container_overrides",
        skip_serializing_if = "is_default_overrides"
    )]
    pub container_overrides: BTreeMap<String, Vec<ContainerKind>>,
    /// Suffix a folder game of a system ends in on this layout (system id
    /// → `.ps3`-style suffix; systems batch 2 D13). Files from before it
    /// load with none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_game_suffix: BTreeMap<String, String>,
    /// Media of a folder game are named after its whole folder name
    /// (ES-DE); files from before it load with `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder_stem_keeps_suffix: bool,
    /// Folder, relative to a system's folder, the frontend reads DLC from
    /// (system id → template whose only token is `{title_id}`; Batocera
    /// `ps4 → DLC/{title_id}`, systems batch 3 D16). Files from before it
    /// load with none: their DLC stays beside its game.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dlc_folder: BTreeMap<String, String>,
}

fn is_default_layout(l: &DiscLayout) -> bool {
    *l == DiscLayout::default()
}

fn is_default_containers(c: &[ContainerKind]) -> bool {
    c == default_accepts_containers().as_slice()
}

fn is_default_overrides(o: &BTreeMap<String, Vec<ContainerKind>>) -> bool {
    *o == default_container_overrides()
}

fn is_default_naming(n: &FolderNaming) -> bool {
    *n == FolderNaming::default()
}

pub fn media_key(m: Media) -> &'static str {
    match m {
        Media::Cover => "cover",
        Media::Screenshot => "screenshot",
        Media::Title => "title",
    }
}

fn media_of(key: &str) -> Option<Media> {
    Media::ALL.into_iter().find(|m| media_key(*m) == key)
}

impl ProfileFile {
    pub fn from_profile(p: &Profile) -> ProfileFile {
        ProfileFile {
            id: p.id.clone(),
            name: p.name.clone(),
            roms_root: p.roms_root.clone(),
            art_dirs: p.art_dir_names.clone(),
            gamelist: p.gamelist,
            slots: Media::ALL
                .into_iter()
                .filter_map(|m| {
                    p.slot(m).map(|s| {
                        (
                            media_key(m).to_string(),
                            SlotFile {
                                template: s.template.clone(),
                                format: s.format,
                                max_width: s.max_size.map(|(w, _)| w),
                                max_height: s.max_size.map(|(_, h)| h),
                            },
                        )
                    })
                })
                .collect(),
            system_folder: p.system_folder,
            // A clone keeps the frontend's own folders (spec D5); Absent
            // is not persisted (a custom layout lists no device folders).
            folder_names: p
                .folders
                .iter()
                .filter_map(|(id, f)| match f {
                    ProfileFolder::Named(n) => Some((id.clone(), n.clone())),
                    ProfileFolder::Absent => None,
                })
                .collect(),
            host_folders: p.host_folders.clone(),
            disc_layout: p.disc_layout,
            accepts_containers: p.accepts_containers.clone(),
            container_overrides: p.container_overrides.clone(),
            folder_game_suffix: p.folder_game_suffix.clone(),
            folder_stem_keeps_suffix: p.folder_stem_keeps_suffix,
            dlc_folder: p.dlc_folder.clone(),
        }
    }

    /// The profile this file describes (call on a validated file).
    pub fn to_profile(&self) -> Profile {
        let slots = self
            .slots
            .iter()
            .filter_map(|(k, s)| {
                let size = match (s.max_width, s.max_height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                media_of(k).map(|m| {
                    (
                        m,
                        MediaSlot {
                            template: s.template.clone(),
                            format: s.format,
                            max_size: size,
                        },
                    )
                })
            })
            .collect();
        Profile {
            id: self.id.clone(),
            name: self.name.trim().to_string(),
            roms_root: self.roms_root.clone(),
            art_dir_names: self.art_dirs.clone(),
            mirror_subdirs: self
                .slots
                .values()
                .any(|s| s.template.contains("{rel_dir}")),
            fixed_format: false,
            gamelist: self.gamelist,
            slots,
            system_folder: self.system_folder,
            host_folders: self.host_folders.clone(),
            arcade_order: vec!["fbneo".to_string(), "mame".to_string()],
            known_folders: Vec::new(),
            disc_layout: self.disc_layout,
            accepts_containers: self.accepts_containers.clone(),
            container_overrides: self.container_overrides.clone(),
            // `folder_names` doubles as the layout's own folder table, so
            // scan and organise resolve them under it (spec D6).
            folders: self
                .folder_names
                .iter()
                .map(|(id, f)| (id.clone(), ProfileFolder::Named(f.trim().to_string())))
                .collect(),
            folder_game_suffix: self.folder_game_suffix.clone(),
            folder_stem_keeps_suffix: self.folder_stem_keeps_suffix,
            dlc_folder: self.dlc_folder.clone(),
        }
    }

    pub fn parse_toml(text: &str) -> Result<ProfileFile, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("a profile file serialises")
    }
}

/// A folder-game suffix: a dot and 1 to 16 ASCII letters or digits
/// (`.ps3`), so it never adds a separator, a space or a reserved name.
fn valid_folder_suffix(s: &str) -> bool {
    s.strip_prefix('.').is_some_and(|rest| {
        (1..=16).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// The only token a DLC folder template takes (the base game's id).
pub const DLC_FOLDER_TOKEN: &str = "{title_id}";

/// A DLC folder template: relative (no root, drive or `\`), every
/// `/`-separated segment plain and never `.` or `..`, and `{title_id}` its
/// only token, so it always stays inside the system's folder. A segment
/// never ends in `.` nor holds a character Windows refuses in a name
/// (`<>"|?*`): the folder made must be the one the frontend reads.
fn valid_dlc_folder(t: &str) -> bool {
    !t.is_empty()
        && t.trim() == t
        && t.len() <= 128
        && !t.starts_with('/')
        && tokens(t).iter().all(|tok| *tok == DLC_FOLDER_TOKEN)
        && t.split('/').all(|seg| {
            let literal = seg.replace(DLC_FOLDER_TOKEN, "");
            !seg.is_empty()
                && seg.trim() == seg
                && seg != "."
                && seg != ".."
                && !seg.ends_with('.')
                && !literal.contains(['\\', ':', '{', '}', '<', '>', '"', '|', '?', '*'])
                && !seg.chars().any(char::is_control)
        })
}

fn plain_name(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains(['/', '\\', ':'])
        && !s.chars().any(char::is_control)
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 40
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// Every token of `template` (`{…}`), in order.
fn tokens(template: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(close) => {
                out.push(&rest[open..open + close + 1]);
                rest = &rest[open + close + 1..];
            }
            None => {
                out.push(&rest[open..]);
                break;
            }
        }
    }
    out
}

/// Tokens that expand to paths: allowed only as a whole segment, since an
/// absolute path pushed inside a segment replaces everything before it.
const PATH_TOKENS: [&str; 3] = ["{root}", "{system_dir}", "{rel_dir}"];

/// Literal text of a segment (its tokens removed) is plain: no control
/// characters, `\` or `:`, and the segment is not dot-like or blank.
fn literal_segment_ok(seg: &str) -> bool {
    let trimmed = seg.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && !seg.contains(['\\', ':'])
        && !seg.chars().any(char::is_control)
}

fn template_error(t: &SlotFile) -> Option<FieldErrorCode> {
    use FieldErrorCode::*;
    let s = t.template.as_str();
    if s.trim().is_empty() {
        return Some(Required);
    }
    if tokens(s).iter().any(|tok| !TOKENS.contains(tok)) {
        return Some(Unknown);
    }
    // What is validated is exactly what is saved and rendered.
    if s.trim() != s {
        return Some(Invalid);
    }
    let segments: Vec<&str> = s.split('/').collect();
    // `{root}` or `{system_dir}` is the whole first segment, once.
    if segments.len() < 2 || !matches!(segments[0], "{root}" | "{system_dir}") {
        return Some(Invalid);
    }
    for (i, seg) in segments.iter().enumerate() {
        let path_tokens = PATH_TOKENS.iter().filter(|p| seg.contains(**p)).count();
        if path_tokens > 0 {
            let whole = PATH_TOKENS.contains(seg);
            let allowed_here = if i == 0 { true } else { *seg == "{rel_dir}" };
            if !whole || !allowed_here {
                return Some(Invalid);
            }
            continue;
        }
        if !literal_segment_ok(seg) {
            return Some(Invalid);
        }
    }
    let last = segments.last().copied().unwrap_or("");
    if !last.contains("{stem}") {
        return Some(Invalid);
    }
    let ext_ok = match t.format {
        ImageFormat::Png => last.ends_with(".png"),
        ImageFormat::Jpg => last.ends_with(".jpg") || last.ends_with(".jpeg"),
    };
    (!ext_ok).then_some(Invalid)
}

/// Problems of `p` as per-field errors (`slots.<media>.<field>`).
pub fn validate_profile(p: &ProfileFile, builtin_ids: &[String]) -> Vec<FieldError> {
    use FieldErrorCode::*;
    let mut out = Vec::new();
    if !valid_id(&p.id) {
        out.push(FieldError::new("id", Invalid));
    } else if builtin_ids.iter().any(|b| b.eq_ignore_ascii_case(&p.id)) {
        out.push(FieldError::new("id", Duplicate));
    }
    let name = p.name.trim();
    if name.is_empty() {
        out.push(FieldError::new("name", Required));
    } else if name.chars().count() > 60 || name.chars().any(char::is_control) {
        out.push(FieldError::new("name", Invalid));
    }
    let root = &p.roms_root;
    if !root.is_empty() && (root.starts_with('/') || !root.split('/').all(plain_name)) {
        out.push(FieldError::new("roms_root", Invalid));
    }
    if !p.art_dirs.iter().all(|d| plain_name(d)) {
        out.push(FieldError::new("art_dirs", Invalid));
    }
    if p.slots.is_empty() {
        out.push(FieldError::new("slots", Required));
    }
    for (key, slot) in &p.slots {
        if media_of(key).is_none() {
            out.push(FieldError::new(format!("slots.{key}"), Unknown));
            continue;
        }
        if let Some(code) = template_error(slot) {
            out.push(FieldError::new(format!("slots.{key}.template"), code));
        }
        match (slot.max_width, slot.max_height) {
            (Some(_), None) => {
                out.push(FieldError::new(format!("slots.{key}.max_height"), Required))
            }
            (None, Some(_)) => {
                out.push(FieldError::new(format!("slots.{key}.max_width"), Required))
            }
            _ => {}
        }
        for (field, v) in [
            ("max_width", slot.max_width),
            ("max_height", slot.max_height),
        ] {
            if let Some(v) = v {
                if !(16..=4096).contains(&v) {
                    out.push(FieldError {
                        field: format!("slots.{key}.{field}"),
                        code: Range,
                        min: Some(16.0),
                        max: Some(4096.0),
                    });
                }
            }
        }
    }
    let mirrors: Vec<bool> = p
        .slots
        .values()
        .map(|s| s.template.contains("{rel_dir}"))
        .collect();
    if mirrors.iter().any(|m| *m) && !mirrors.iter().all(|m| *m) {
        out.push(FieldError::new("slots", Order));
    }
    let db = SystemDb::builtin();
    for (system_id, host_id) in &p.host_folders {
        let known = |id: &str| db.get(id).is_some_and(|s| s.id == id);
        if !known(system_id) || !known(host_id) || system_id == host_id {
            out.push(FieldError::new(
                format!("host_folders.{system_id}"),
                Invalid,
            ));
        }
    }
    for (system_id, kinds) in &p.container_overrides {
        // A disc system never takes an archive as-is (spec D4), so a
        // non-empty override for one would be ignored without a word.
        let known = db
            .get(system_id)
            .is_some_and(|s| s.id == *system_id && (!s.multi_file || kinds.is_empty()));
        if !known {
            out.push(FieldError::new(
                format!("container_overrides.{system_id}"),
                Invalid,
            ));
        }
    }
    for (system_id, suffix) in &p.folder_game_suffix {
        if db.get(system_id).is_none_or(|s| s.id != *system_id) || !valid_folder_suffix(suffix) {
            out.push(FieldError::new(
                format!("folder_game_suffix.{system_id}"),
                Invalid,
            ));
        }
    }
    for (system_id, template) in &p.dlc_folder {
        if db.get(system_id).is_none_or(|s| s.id != *system_id) || !valid_dlc_folder(template) {
            out.push(FieldError::new(format!("dlc_folder.{system_id}"), Invalid));
        }
    }
    if !p.folder_names.is_empty() {
        let profile = p.to_profile();
        // The file's own table would vouch for itself: a name must also
        // be an alias of the system (scoped ones included) or a built-in
        // frontend's folder for it.
        let mut bare = profile.clone();
        bare.folders.clear();
        let builtins = builtin_profiles();
        for (system_id, folder) in &p.folder_names {
            let vouched = validate_folder_override(&db, &bare, system_id, folder).is_ok()
                || builtins.iter().any(|b| {
                    b.folder_owner(folder) == Some(system_id.as_str())
                        && validate_folder_override(&db, b, system_id, folder).is_ok()
                });
            if !vouched || validate_folder_override(&db, &profile, system_id, folder).is_err() {
                out.push(FieldError::new(
                    format!("folder_names.{system_id}"),
                    Invalid,
                ));
            }
        }
    }
    out
}

/// Where each valid slot of `p` puts the art of the example ROM, relative
/// to the library root, `/`-separated.
pub fn preview_paths(
    p: &ProfileFile,
    ex: &PreviewExample,
    systems: &SystemDb,
) -> BTreeMap<String, String> {
    let root = Path::new("/__library__");
    let system_dir = root.join(&p.roms_root).join(ex.system_folder.trim());
    let layout = p.to_profile();
    let system = systems.resolve_folder_in(Some(&layout), ex.system_folder.trim());
    let stem = Path::new(ex.file_name.trim())
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    for (key, slot) in &p.slots {
        if media_of(key).is_none() || template_error(slot).is_some() {
            continue;
        }
        let path = render(
            &slot.template,
            &TemplateVars {
                root,
                system_dir: &system_dir,
                rel_dir: Path::new(ex.rel_dir.trim().trim_matches('/')),
                stem: &stem,
                system_id: system.map_or("unknown", |s| s.id.as_str()),
                libretro: system.and_then(|s| s.libretro.as_deref()),
            },
        );
        // A hostile example (`..` folders) has no preview: it never shows
        // a path outside the library.
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let mut shown = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(s) => shown.push(s.to_string_lossy().into_owned()),
                _ => {
                    shown.clear();
                    break;
                }
            }
        }
        if !shown.is_empty() {
            out.insert(key.clone(), shown.join("/"));
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewExample {
    pub system_folder: String,
    pub rel_dir: String,
    pub file_name: String,
}

/// The custom profiles folder.
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn new(dir: PathBuf) -> ProfileStore {
        ProfileStore { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_of(&self, id: &str) -> io::Result<PathBuf> {
        if !valid_id(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a profile id: {id}"),
            ));
        }
        Ok(self.dir.join(format!("{id}.toml")))
    }

    /// Every valid profile file, sorted by id; problems as warnings.
    pub fn load(&self) -> (Vec<ProfileFile>, Vec<String>) {
        let mut found = Vec::new();
        let mut warnings = Vec::new();
        let Ok(read) = fs::read_dir(&self.dir) else {
            return (found, warnings);
        };
        let builtin_ids: Vec<String> = builtin_profiles().into_iter().map(|p| p.id).collect();
        let mut entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let shown = path.display().to_string();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) if m.is_file() => m,
                Ok(_) => {
                    warnings.push(format!("{shown}: not a regular file"));
                    continue;
                }
                Err(_) => continue,
            };
            if meta.len() > MAX_PROFILE_BYTES {
                warnings.push(format!("{shown}: larger than {MAX_PROFILE_BYTES} bytes"));
                continue;
            }
            let parsed = fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| ProfileFile::parse_toml(&t));
            let file = match parsed {
                Ok(f) => f,
                Err(e) => {
                    warnings.push(format!("{shown}: {e}"));
                    continue;
                }
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if stem != file.id {
                warnings.push(format!(
                    "{shown}: its id `{}` does not match the file name",
                    file.id
                ));
                continue;
            }
            let problems = validate_profile(&file, &builtin_ids);
            if !problems.is_empty() {
                let fields: Vec<String> = problems.into_iter().map(|e| e.field).collect();
                warnings.push(format!("{shown}: invalid {}", fields.join(", ")));
                continue;
            }
            found.push(file);
        }
        (found, warnings)
    }

    /// Writes `p` (validated by the caller) atomically; with `replaces`,
    /// the old file goes once the new one is written.
    pub fn save(&self, p: &ProfileFile, replaces: Option<&str>) -> io::Result<()> {
        let path = self.path_of(&p.id)?;
        let old = replaces.map(|id| self.path_of(id)).transpose()?;
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{}.toml.tmp", p.id));
        fs::write(&tmp, p.to_toml())?;
        fs::rename(&tmp, &path)?;
        if let Some(old) = old.filter(|o| *o != path) {
            match fs::remove_file(old) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// Deletes profile `id`; `false` when it did not exist.
    pub fn delete(&self, id: &str) -> io::Result<bool> {
        match fs::remove_file(self.path_of(id)?) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Built-ins first (`true`), then the custom ones.
pub fn all_profiles(custom: &[ProfileFile]) -> Vec<(Profile, bool)> {
    builtin_profiles()
        .into_iter()
        .map(|p| (p, true))
        .chain(custom.iter().map(|f| (f.to_profile(), false)))
        .collect()
}

/// The profile `id` (case-insensitive), built-ins first.
pub fn find_profile(custom: &[ProfileFile], id: &str) -> Option<Profile> {
    all_profiles(custom)
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| p.id.eq_ignore_ascii_case(id))
}
