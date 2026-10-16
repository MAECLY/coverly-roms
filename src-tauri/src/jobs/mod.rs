//! Fill jobs of the desktop app (spec §7 steps 3–9, §8 Jobs).

pub mod view;

use std::path::PathBuf;

use coverly_core::fill::Endpoints;
use coverly_core::net::NetConfig;

/// Where fills keep their cache and journals, and how they reach the network.
#[derive(Clone, Debug)]
pub struct FillConfig {
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub net: NetConfig,
}

impl FillConfig {
    /// The real servers with the default network settings.
    pub fn new(cache_dir: PathBuf, data_dir: PathBuf) -> FillConfig {
        FillConfig {
            cache_dir,
            data_dir,
            endpoints: Endpoints::default(),
            net: NetConfig::default(),
        }
    }
}
