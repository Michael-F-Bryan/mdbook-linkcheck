use crate::Config;
use codespan::Files;
use http::HeaderMap;
use linkcheck::{
    validation::{Cache, Options},
    Link,
};
use reqwest::{Client, Url};
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

    fn should_ignore(&self, link: &Link) -> bool {
        self.cfg
            .exclude
            .iter()
            .any(|re| re.find(&link.href).is_some())
    }

    fn url_specific_headers(&self, url: &Url) -> HeaderMap {
        let url = url.to_string();
        let mut headers = HeaderMap::new();

        let extra_headers = self
            .cfg
            .http_headers
            .iter()
            .filter_map(|(re, extra)| {
                if re.find(&url).is_some() {
                    Some(extra)
                } else {
                    None
                }
            })
            .flatten();

        for header in extra_headers {
            let crate::config::HttpHeader {
                name,
                interpolated_value,
                ..
            } = header;
            headers.insert(name.clone(), interpolated_value.clone());
        }

        headers
    }
}
