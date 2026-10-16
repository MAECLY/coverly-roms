//! Settings → the option structs the scan, fill and writes take.

use crate::fill::FillSettings;
use crate::image::Fit;
use crate::matching::{default_region_order, parse_region_order};
use crate::profiles::{ImageFormat, Profile};
use crate::scan::ScanOptions;
use crate::write::OverwritePolicy;

use super::model::{OutputFormat, OverwriteSetting, Settings};

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
}
