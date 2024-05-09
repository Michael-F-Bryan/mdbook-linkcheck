use crate::{Config, HashedRegex};
use http::header::{HeaderMap, HeaderName, HeaderValue};
use linkcheck::{
    validation::{Cache, Options},
    Link,
};
use reqwest::{Client, Url};
use std::sync::{Mutex, MutexGuard};

/// The [`linkcheck::validation::Context`].
#[derive(Debug)]
pub struct Context<'a> {
    pub(crate) cfg: &'a Config,
    pub(crate) cache: Mutex<Cache>,
    pub(crate) client: Client,
    pub(crate) filesystem_options: Options,
    pub(crate) interpolated_headers: Vec<(HashedRegex, Vec<(HeaderName, HeaderValue)>)>,
}

impl<'a> linkcheck::validation::Context for Context<'a> {
    fn client(&self) -> &Client {
        &self.client
    }

    fn filesystem_options(&self) -> &Options {
        &self.filesystem_options
    }

    fn cache(&self) -> Option<MutexGuard<Cache>> {
        Some(self.cache.lock().expect("Lock was poisoned"))
    }

    fn should_ignore(&self, link: &Link) -> bool {
        if !self.cfg.follow_web_links && link.href.parse::<Url>().is_ok() {
            return true;
        }

        self.cfg
            .exclude
            .iter()
            .any(|re| re.find(&link.href).is_some())
    }

    fn url_specific_headers(&self, url: &Url) -> HeaderMap {
        let url = url.to_string();
        let mut headers = HeaderMap::new();

        for (pattern, matching_headers) in &self.interpolated_headers {
            if pattern.find(&url).is_some() {
                for (name, value) in matching_headers {
                    headers.insert(name.clone(), value.clone());
                }
            }
        }

        headers
    }
}
