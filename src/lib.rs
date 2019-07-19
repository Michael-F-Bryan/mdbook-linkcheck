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

pub const COMPATIBLE_MDBOOK_VERSIONS: &str = "^0.3.0";

mod config;

pub use config::Config;

use mdbook::renderer::RenderContext;
use semver::Version;
use failure::{Error, ResultExt};

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
    unimplemented!();
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
