//! Identifying one entry on demand with the core identifier. Offline-safe:
//! cached DATs (and the hash memo) answer without the network; when the
//! DATs are unreachable and not cached, the entry is identified by name and
//! the result says it was offline. A manual identity ("Search by title",
//! D32) wins over every other method.

use coverly_core::cache::Cache;
use coverly_core::identify::dat::{DatError, DatStore};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::identify::overrides::Overrides;
use coverly_core::identify::{IdMethod, Identifier, IdentifyOptions};
use coverly_core::net::Http;
use coverly_core::systems::SystemDb;
use serde::Serialize;

use super::Session;
use crate::error::IpcError;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IdentityView {
    /// DAT or arcade name, when identified.
    pub canonical: Option<String>,
    pub method: IdMethod,
    /// 1.0 for a DAT hit, 0.0 otherwise.
    pub confidence: f32,
    /// Parsed title (of the canonical name, or else of the file name).
    pub title: String,
    pub regions: Vec<Region>,
    /// The DATs could not be reached and none was cached.
    pub offline: bool,
}

pub fn identify_entry(
    session: &Session,
    id: u32,
    systems: &SystemDb,
    cache: &Cache,
    http: &dyn Http,
    database_base: &str,
    overrides: &Overrides,
) -> Result<IdentityView, IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    let system = systems
        .get(&entry.system_id)
        .ok_or(IpcError::UnknownEntry(id))?;
    let dats = DatStore::new(http, cache, database_base);
    // A manual identity (D32) answers first, without the network.
    let identifier =
        Identifier::new(cache, &dats, IdentifyOptions::default()).with_overrides(overrides);
    let (identity, offline) = match identifier.identify(&entry.rom, system, false) {
        Ok(identity) => (identity, false),
        Err(DatError::Cache(e)) => return Err(IpcError::Io(e.to_string())),
        Err(e) => {
            let offline = matches!(&e, DatError::Net(net) if net.pauses());
            (
                identifier.unidentified(parse_name(&entry.rom.stem, false)),
                offline,
            )
        }
    };
    Ok(IdentityView {
        canonical: identity.canonical,
        method: identity.method,
        confidence: identity.confidence,
        title: identity.parsed.title,
        regions: identity.parsed.regions,
        offline,
    })
}
