//! Settings → the option structs the scan, fill and writes take.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::fill::{FillSettings, ManualOptions};
use crate::image::Fit;
use crate::jobs::ApplyOptions;
use crate::matching::{default_region_order, parse_region_order};
use crate::net::{NetConfig, ProxyMode};
use crate::profiles::{ImageFormat, Profile};
use crate::scan::ScanOptions;
use crate::write::OverwritePolicy;

use super::model::{OutputFormat, OverwriteSetting, ProxyChoice, Settings};

/// `template` with its image extension (any case: `.png`, `.JPG`,
/// `.Jpeg`) set to `format`'s lowercase one.
fn with_extension(template: &str, format: ImageFormat) -> String {
    let ext = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpg => "jpg",
    };
    for old in [".png", ".jpg", ".jpeg"] {
        let Some(cut) = template.len().checked_sub(old.len()) else {
            continue;
        };
        if template.is_char_boundary(cut) && template[cut..].eq_ignore_ascii_case(old) {
            return format!("{}.{ext}", &template[..cut]);
        }
    }
    template.to_string()
}

impl Settings {
    /// The Library group as scan options.
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            include_hidden: self.library.include_hidden,
            follow_symlinks: self.library.follow_symlinks,
            ignored_dirs: self.library.ignored_dirs.clone(),
            ignored_exts: self.library.ignored_exts.clone(),
        }
    }

    /// The Art media + Matching + Sources groups as fill settings (systems,
    /// scope and refresh keep their defaults; callers set them).
    pub fn fill_settings(&self) -> FillSettings {
        let m = &self.matching;
        FillSettings {
            media: self.art.media.clone(),
            strategy: m.strategy,
            regions: parse_region_order(&m.regions.join(","))
                .unwrap_or_else(|_| default_region_order()),
            match_method: m.match_method,
            hash_max_bytes: u64::from(m.max_hash_mb) * 1024 * 1024,
            arcade_translation: m.arcade_translation,
            fuzzy: m.fuzzy,
            auto_threshold: m.auto_threshold,
            review_threshold: m.review_threshold,
            languages: m.languages.clone(),
            sources: self
                .sources
                .order
                .iter()
                .filter(|e| e.enabled)
                .map(|e| e.id.clone())
                .collect(),
            index_ttl_secs: i64::from(self.network.index_refresh_hours) * 3600,
            ..FillSettings::default()
        }
    }

    /// `base` with the Art group applied: the one slot of a single-slot
    /// layout holds `single_slot_media`; the output format (unless the
    /// layout fixes it) and the per-profile max size override the slots.
    /// Idempotent.
    pub fn art_profile(&self, base: &Profile) -> Profile {
        let mut p = base.clone();
        if p.slots.len() == 1 {
            let slot = p.slots.drain().next().map(|(_, slot)| slot);
            if let Some(slot) = slot {
                p.slots.insert(self.art.single_slot_media, slot);
            }
        }
        let format = match (p.fixed_format, self.art.output_format) {
            (true, _) | (_, OutputFormat::Profile) => None,
            (false, OutputFormat::Png) => Some(ImageFormat::Png),
            (false, OutputFormat::Jpg) => Some(ImageFormat::Jpg),
        };
        let size = self.art.max_size.get(&p.id).map(|m| (m.width, m.height));
        for slot in p.slots.values_mut() {
            if let Some(f) = format {
                slot.format = f;
                slot.template = with_extension(&slot.template, f);
            }
            if size.is_some() {
                slot.max_size = size;
            }
        }
        p
    }

    /// How images fit the slot box ("keep aspect").
    pub fn fit(&self) -> Fit {
        if self.art.keep_aspect {
            Fit::Contain
        } else {
            Fit::Stretch
        }
    }

    /// The overwrite policy inside a job (D25: `ask` is asked before the
    /// job starts, so inside it, it is `never` unless the user chose to
    /// replace).
    pub fn overwrite_policy(&self) -> OverwritePolicy {
        match self.art.overwrite {
            OverwriteSetting::Backup => OverwritePolicy::Backup,
            OverwriteSetting::Never | OverwriteSetting::Ask => OverwritePolicy::Never,
        }
    }

    /// The Network group as HTTP settings (D26: `system` = environment).
    pub fn net_config(&self) -> NetConfig {
        let n = &self.network;
        NetConfig {
            concurrency: n.concurrency.clamp(1, 6) as usize,
            timeout: Duration::from_secs(u64::from(n.timeout_secs)),
            retries: n.retries,
            proxy: match n.proxy {
                ProxyChoice::System | ProxyChoice::Env => ProxyMode::Env,
                ProxyChoice::Manual => ProxyMode::Manual(n.proxy_url.trim().to_string()),
            },
            ..NetConfig::default()
        }
    }

    /// Apply options for a job on the library at `root` (scope, review and
    /// resume are set by the caller).
    pub fn apply_options(&self, root: &Path) -> ApplyOptions {
        ApplyOptions {
            overwrite: self.overwrite_policy(),
            concurrency: self.network.concurrency.clamp(1, 6) as usize,
            fit: self.fit(),
            jpeg_quality: self.art.jpeg_quality.clamp(1, 100),
            keep_jobs: self.safety.undo_depth.max(1) as usize,
            cache_max_bytes: Some(u64::from(self.network.cache_max_mb) * 1024 * 1024),
            clean_apple_double: self.safety.clean_apple_double
                && crate::write::on_removable_volume(root),
            ..ApplyOptions::default()
        }
    }

    /// Options of the drawer's manual writes (output and history).
    pub fn manual_options(&self) -> ManualOptions {
        ManualOptions {
            fit: self.fit(),
            jpeg_quality: self.art.jpeg_quality.clamp(1, 100),
            keep_jobs: self.safety.undo_depth.max(1) as usize,
        }
    }

    /// Where jobs keep journals and backups (D27).
    pub fn data_dir(&self, default: &Path) -> PathBuf {
        self.safety
            .backup_dir
            .clone()
            .unwrap_or_else(|| default.to_path_buf())
    }

    /// Whether going from `self` to `next` changes what an open library
    /// shows (scan options, the single-slot media, the output format, the
    /// max sizes or the custom profiles folder): the library must be
    /// reopened.
    pub fn needs_reopen(&self, next: &Settings) -> bool {
        let lib = |s: &Settings| {
            (
                s.library.ignored_dirs.clone(),
                s.library.ignored_exts.clone(),
                s.library.include_hidden,
                s.library.follow_symlinks,
            )
        };
        lib(self) != lib(next)
            || self.art.single_slot_media != next.art.single_slot_media
            || self.art.output_format != next.art.output_format
            || self.art.max_size != next.art.max_size
            || self.advanced.custom_dir != next.advanced.custom_dir
    }
}
