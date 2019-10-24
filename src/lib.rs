//! A `mdbook` backend which will check all links in a document are valid.
//!
//! The link-checking process has roughly three stages:
//!
//! 1. Find all the links in a body of markdown text (see [`extract_links`])
//! 2. Validate all the links we've found, taking into account cached results
//!    and configuration options
//! 3. Cache the results in the output directory for reuse by step 2 in the next
//!    round
//! 4. Emit errors/warnings to the user

#![deny(
    intra_doc_link_resolution_failure,
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

/// A semver range specifying which versions of `mdbook` this crate supports.
pub const COMPATIBLE_MDBOOK_VERSIONS: &str = "^0.3.0";

mod cache;
mod config;
mod links;
mod validate;

pub use crate::{
    cache::Cache,
    config::Config,
    links::{extract_links, IncompleteLink, Link},
    validate::{
        validate, InvalidLink, Reason, UnknownScheme, ValidationOutcome,
    },
};

use codespan::{FileId, Files};
use failure::{Error, ResultExt};
use mdbook::book::{Book, BookItem};
use semver::{Version, VersionReq};

/// Get the configuration used by `mdbook-linkcheck`.
pub fn get_config(cfg: &mdbook::Config) -> Result<Config, Error> {
    match cfg.get("output.linkcheck") {
        Some(raw) => raw
            .clone()
            .try_into()
            .context("Unable to deserialize the `output.linkcheck` table.")
            .map_err(Error::from),
        None => Ok(Config::default()),
    }
}

/// Check whether this library is compatible with the provided version string.
pub fn version_check(version: &str) -> Result<(), Error> {
    let constraints = VersionReq::parse(COMPATIBLE_MDBOOK_VERSIONS)?;
    let found = Version::parse(version)?;

    if constraints.matches(&found) {
        Ok(())
    } else {
        let msg = format!(
            "mdbook-linkcheck isn't compatible with this version of mdbook ({} is not in the range {})",
            found, constraints
        );
        Err(failure::err_msg(msg))
    }
}

/// A helper for reading the chapters of a [`Book`] into memory.
pub fn load_files_into_memory(book: &Book, dest: &mut Files) -> Vec<FileId> {
    let mut ids = Vec::new();

    for item in book.iter() {
        match item {
            BookItem::Chapter(ref ch) => {
                let id = dest.add(ch.path.display().to_string(), &ch.content);
                ids.push(id);
            },
            BookItem::Separator => {},
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_stay_compatible_with_mdbook_dependency() {
        version_check(mdbook::MDBOOK_VERSION).unwrap();
    }
}
