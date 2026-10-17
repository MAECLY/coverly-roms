//! ROM identification: content hashes, names, DAT lookups (spec §16.2–§16.7).

pub mod dat;
pub mod evidence;
pub mod hash;
pub mod names;
pub mod overrides;
pub mod sniff;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::scan::RomEntry;
use crate::systems::System;
use dat::{DatError, DatStore};
use hash::{HashOutcome, DEFAULT_HASH_MAX_BYTES};
use names::ParsedName;

/// How ROMs are identified (spec §9 "matching method").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchMethod {
    /// Content hash against the DAT, then the file name.
    #[default]
    HashThenName,
    /// Content hash only: a ROM not in the DAT stays unknown. Arcade sets
    /// (never hashed) must have their short name in the arcade DAT.
    HashOnly,
    /// File name only: nothing is hashed.
    NameOnly,
}

/// How a ROM was identified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdMethod {
    /// Its content hash matched a DAT game.
    Hash,
    /// Its arcade short name matched a MAME/FBNeo set.
    ArcadeDat,
    /// Only its (parsed) file name is known.
    Name,
    /// Hash-only and not identified by a DAT (arcade sets included):
    /// never matched by name.
    Unknown,
    /// The user picked it in "Search by title" (D32).
    Manual,
}

/// What is known about one ROM.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// The DAT or arcade name, when identified.
    pub canonical: Option<String>,
    pub method: IdMethod,
    /// 1.0 for a DAT hit, 0.0 otherwise.
    pub confidence: f32,
    /// The canonical name parsed, or else the ROM stem parsed.
    pub parsed: ParsedName,
    /// The hashes, when the ROM was hashed.
    pub hash: Option<HashOutcome>,
}

/// Identification settings (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct IdentifyOptions {
    pub method: MatchMethod,
    /// Larger ROMs are identified by name.
    pub hash_max_bytes: u64,
    /// Translate arcade short names (`10yard`) to their description.
    pub arcade_translation: bool,
}

impl Default for IdentifyOptions {
    fn default() -> Self {
        IdentifyOptions {
            method: MatchMethod::HashThenName,
            hash_max_bytes: DEFAULT_HASH_MAX_BYTES,
            arcade_translation: true,
        }
    }
}

/// Identifies ROMs with the hash memo in the cache and on-demand DATs.
pub struct Identifier<'a> {
    cache: &'a Cache,
    dats: &'a DatStore<'a>,
    opts: IdentifyOptions,
    overrides: Option<&'a overrides::Overrides>,
}

impl<'a> Identifier<'a> {
    pub fn new(cache: &'a Cache, dats: &'a DatStore<'a>, opts: IdentifyOptions) -> Self {
        Identifier {
            cache,
            dats,
            opts,
            overrides: None,
        }
    }

    /// Manual identities (D32) win over every other method.
    pub fn with_overrides(mut self, overrides: &'a overrides::Overrides) -> Self {
        self.overrides = Some(overrides);
        self
    }

    /// A manual identity (D32) first. Arcade: short name → description (no
    /// hashing). Otherwise, unless
    /// name-only: hash (memoised by path + size + mtime) → DAT game. A miss
    /// falls back to the parsed name; under hash-only every miss (arcade,
    /// no DAT, not in the DAT) is `Unknown` and never matched by name.
    pub fn identify(
        &self,
        rom: &RomEntry,
        system: &System,
        strip_numbering: bool,
    ) -> Result<Identity, DatError> {
        if let Some(m) = self.overrides.and_then(|o| o.get(&rom.path)) {
            return Ok(Identity {
                parsed: names::parse_name(&m.name, false),
                canonical: Some(m.name.clone()),
                method: IdMethod::Manual,
                confidence: 1.0,
                hash: None,
            });
        }
        let parsed = names::parse_name(&rom.stem, strip_numbering);
        let dats = dat::dat_sources(system);
        if system.arcade {
            // Arcade sets are never hashed: the short-name lookup is their
            // DAT identification.
            if self.opts.arcade_translation {
                if let Some(desc) = self.dats.arcade_description(&dats, &rom.stem)? {
                    return Ok(Identity {
                        parsed: names::parse_name(&desc, false),
                        canonical: Some(desc),
                        method: IdMethod::ArcadeDat,
                        confidence: 1.0,
                        hash: None,
                    });
                }
            }
        } else if self.opts.method != MatchMethod::NameOnly && !dats.is_empty() {
            let mtime = crate::cache::mtime_ns(rom.modified);
            let outcome = match self.cache.quick_id(&rom.path, rom.size, mtime)? {
                Some(o) => o,
                None => {
                    // An unreadable file is identified by name.
                    let o = hash::hash_rom(
                        &rom.path,
                        &rom.ext,
                        system.header,
                        self.opts.hash_max_bytes,
                    )
                    .unwrap_or(HashOutcome::NameOnly);
                    self.cache.put_quick_id(&rom.path, rom.size, mtime, &o)?;
                    o
                }
            };
            if let Some(game) = self.dats.lookup(&dats, &outcome)? {
                return Ok(Identity {
                    parsed: names::parse_name(&game, false),
                    canonical: Some(game),
                    method: IdMethod::Hash,
                    confidence: 1.0,
                    hash: Some(outcome),
                });
            }
            if self.opts.method == MatchMethod::HashOnly {
                return Ok(Identity {
                    canonical: None,
                    method: IdMethod::Unknown,
                    confidence: 0.0,
                    parsed,
                    hash: Some(outcome),
                });
            }
        }
        Ok(self.unidentified(parsed))
    }

    /// The identity of a ROM no DAT identified (or whose identification
    /// failed): its parsed name, unless hash-only, which never falls back
    /// to the name (`Unknown`): not without a DAT, not for an arcade set
    /// missing from it (or with translation off), not after a DAT error.
    pub fn unidentified(&self, parsed: ParsedName) -> Identity {
        Identity {
            canonical: None,
            method: if self.opts.method == MatchMethod::HashOnly {
                IdMethod::Unknown
            } else {
                IdMethod::Name
            },
            confidence: 0.0,
            parsed,
            hash: None,
        }
    }
}
