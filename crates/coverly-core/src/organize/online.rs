//! Optional online identification for organise (Task 24): ScreenScraper
//! asked by CRC when the offline levels leave a file below Auto. Built by
//! the planner only when the source is available, enabled, the user
//! account exists and the run is not offline.

use super::evidence::OnlineLookup;
use crate::identify::hash::HashOutcome;
use crate::sources::screenscraper::ScreenScraperSource;
use crate::systems::SystemDb;

/// Most systems asked per file: each question costs daily quota.
pub const MAX_SYSTEMS: usize = 3;

/// [`OnlineLookup`] over ScreenScraper's `jeuInfos.php`.
pub struct ScreenScraperLookup<'a> {
    source: &'a ScreenScraperSource<'a>,
    db: &'a SystemDb,
}

impl<'a> ScreenScraperLookup<'a> {
    pub fn new(source: &'a ScreenScraperSource<'a>, db: &'a SystemDb) -> Self {
        ScreenScraperLookup { source, db }
    }
}

impl OnlineLookup for ScreenScraperLookup<'_> {
    /// Asks the first [`MAX_SYSTEMS`] of `systems` that have a ScreenScraper
    /// id, in order, and stops at the first hit. With no candidate systems
    /// nothing bounds the question, so nothing is asked. Any error (quota
    /// used up, refused account, network) ends the lookup with no match:
    /// online evidence is a bonus, never a requirement.
    fn lookup(
        &self,
        systems: &[String],
        file_name: &str,
        crc32: &str,
        size: u64,
    ) -> Vec<(String, String)> {
        let hash = HashOutcome::ZipEntry {
            crc32: crc32.to_string(),
            size,
            inner_name: file_name.to_string(),
        };
        let asked = systems
            .iter()
            .filter_map(|id| Some((id, self.db.get(id)?.screenscraper?)))
            .take(MAX_SYSTEMS);
        for (system, sid) in asked {
            match self.source.identify_by_hash(sid, file_name, &hash) {
                Ok(Some(game)) => return vec![(system.clone(), game)],
                Ok(None) => {}
                Err(_) => break,
            }
        }
        Vec::new()
    }
}
