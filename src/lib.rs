//! A `mdbook` backend which will check all links in a document are valid.

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate mdbook;
extern crate memchr;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate url;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

mod config;
pub mod errors;
mod links;
mod validation;

pub use config::Config;
pub use links::Link;

use errors::BrokenLinks;
use failure::{Error, ResultExt, SyncFailure};
use mdbook::book::{Book, BookItem};
use mdbook::renderer::RenderContext;
use std::error::Error as StdError;

use links::collect_links;
use validation::check_link;

/// The main entrypoint for this crate.
///
/// If there were any broken links then you'll be able to downcast the `Error`
/// returned into `BrokenLinks`.
pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    info!("Started the link checker");

    let cfg = get_config(ctx)?;

    if log_enabled!(::log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            trace!("{}", line);
        }
    }

    info!("Scanning book for links");
    let links = all_links(&ctx.book);

    info!("Found {} links in total", links.len());
    validate_links(&links, ctx, &cfg).map_err(Error::from)
}

fn all_links(book: &Book) -> Vec<Link> {
    let mut links = Vec::new();

    for item in book.iter() {
        if let BookItem::Chapter(ref ch) = *item {
            let found = collect_links(ch);
            links.extend(found);
        }
    }

    links
}

fn validate_links(links: &[Link], ctx: &RenderContext, cfg: &Config) -> Result<(), BrokenLinks> {
    let mut errors = Vec::new();

    for link in links {
        if let Err(e) = check_link(link, ctx, &cfg) {
            trace!("Error for {}, {}", link, e);
            errors.push(e);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(BrokenLinks::new(errors))
    }
}

fn get_config(ctx: &RenderContext) -> Result<Config, Error> {
    match ctx.config.get("output.linkcheck") {
        Some(raw) => raw
            .clone()
            .try_into()
            .context("Unable to deserialize the `output.linkcheck` table.")
            .map_err(Error::from),
        None => Ok(Config::default()),
    }
}

/// A workaround because `error-chain` errors aren't `Sync`, yet `failure`
/// errors are required to be.
///
/// See also
/// [withoutboats/failure:109](https://github.com/withoutboats/failure/issues/109).
trait SyncResult<T, E> {
    fn sync(self) -> Result<T, SyncFailure<E>>
    where
        Self: Sized,
        E: StdError + Send + 'static;
}

impl<T, E> SyncResult<T, E> for Result<T, E> {
    fn sync(self) -> Result<T, SyncFailure<E>>
    where
        Self: Sized,
        E: StdError + Send + 'static,
    {
        self.map_err(SyncFailure::new)
    }
}
