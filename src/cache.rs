use failure::Error;
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::RwLock,
    time::{Duration, SystemTime},
};

/// A cache used to avoid unnecessary web requests.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    links: RwLock<BTreeMap<String, CacheEntry>>,
}

impl Cache {
    /// Save the [`Cache`] as JSON.
    pub fn save<W: Write>(&self, writer: W) -> Result<(), Error> {
        serde_json::to_writer(writer, self)?;
        Ok(())
    }

    /// Load a [`Cache`] from some JSON.
    pub fn load<R: Read>(reader: R) -> Result<Cache, Error> {
        serde_json::from_reader(reader).map_err(Error::from)
    }

    pub(crate) fn lookup(&self, url: &str) -> Option<CacheEntry> {
        let links = self.links.read().expect("Lock was poisoned");

        links.get(url).cloned()
    }

    pub(crate) fn insert<S: Into<String>>(&self, url: S, entry: CacheEntry) {
        self.links
            .write()
            .expect("Lock was poisoned")
            .insert(url.into(), entry);
    }
}

/// An entry in the cache.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CacheEntry {
    pub unix_timestamp: u64,
    pub successful: bool,
}

impl CacheEntry {
    pub fn new(now: SystemTime, successful: bool) -> CacheEntry {
        let unix_timestamp = match now.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(ts) => ts,
            Err(e) => panic!(
                "The timestamp was {:?} before the unix epoch",
                e.duration()
            ),
        };

        CacheEntry {
            unix_timestamp: unix_timestamp.as_secs(),
            successful,
        }
    }

    pub fn elapsed(&self) -> Duration {
        let ts =
            SystemTime::UNIX_EPOCH + Duration::from_secs(self.unix_timestamp);

        ts.elapsed().expect("Entry timestamp was in the future")
    }
}
