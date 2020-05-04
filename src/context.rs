use crate::Config;
use codespan::Files;
use linkcheck::validation::{Cache, Options};
use reqwest::Client;
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

/// The [`linkcheck::validation::Context`].
#[derive(Debug)]
pub struct Context<'a> {
    pub(crate) cfg: &'a Config,
    pub(crate) src_dir: &'a Path,
    pub(crate) cache: Mutex<Cache>,
    pub(crate) files: &'a Files<String>,
    pub(crate) client: Client,
    pub(crate) filesystem_options: Options,
}

impl<'a> linkcheck::validation::Context for Context<'a> {
    fn client(&self) -> &Client { &self.client }

    fn filesystem_options(&self) -> &Options { &self.filesystem_options }

    fn cache(&self) -> Option<MutexGuard<Cache>> {
        Some(self.cache.lock().expect("Lock was poisoned"))
    }
}
