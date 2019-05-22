//! A `mdbook` backend which will check all links in a document are valid.

extern crate failure;
extern crate semver;
#[macro_use]
extern crate log;
extern crate mdbook;
extern crate memchr;
extern crate pulldown_cmark;
extern crate regex;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rayon;
extern crate url;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;
#[cfg(test)]
extern crate toml;

pub const COMPATIBLE_MDBOOK_VERSIONS: &str = "^0.2.0";

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
use rayon::prelude::*;
use semver::Version;
use std::error::Error as StdError;

use links::collect_links;
use validation::check_link;

/// The main entrypoint for this crate.
///
/// If there were any broken links then you'll be able to downcast the `Error`
/// returned into `BrokenLinks`.
pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    info!("Started the link checker");

    version_check(ctx)?;

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
    let broken_links: BrokenLinks = links
        .into_par_iter()
        .map(|l| check_link(l, ctx, &cfg))
        .filter_map(|result| result.err())
        .collect();

    if broken_links.links().is_empty() {
        Ok(())
    } else {
        Err(broken_links)
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

fn version_check(ctx: &RenderContext) -> Result<(), Error> {
    let compiled_for = Version::parse(mdbook::MDBOOK_VERSION)?;
    let mut upper_limit = compiled_for.clone();
    upper_limit.increment_minor();

    let found = Version::parse(&ctx.version)?;

    if compiled_for <= found && found < upper_limit {
        Ok(())
    } else {
        let msg = format!(
            "mdbook-linkcheck isn't compatible with this version of mdbook. Expected {} <= {} < {}",
            compiled_for, found, upper_limit,
        );
        Err(failure::err_msg(msg))
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
