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
    config::{Config, WarningPolicy},
    links::{extract as extract_links, IncompleteLink, Link},
    validate::{
        validate, InvalidLink, Reason, UnknownScheme, ValidationOutcome,
    },
};

use codespan::{FileId, Files};
use codespan_reporting::{
    diagnostic::{Diagnostic, Severity},
    term::termcolor::{ColorChoice, StandardStream},
};
use failure::{Error, ResultExt};
use mdbook::{
    book::{Book, BookItem},
    renderer::RenderContext,
};
use semver::{Version, VersionReq};
use std::{fs::File, path::Path};

/// Run the link checking pipeline.
pub fn run(
    cache_file: &Path,
    colour: ColorChoice,
    ctx: &RenderContext,
) -> Result<(), Error> {
    let cache = load_cache(cache_file);

    log::info!("Started the link checker");

    let cfg = crate::get_config(&ctx.config)?;
    crate::version_check(&ctx.version)?;

    if log::log_enabled!(log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            log::trace!("{}", line);
        }
    }

    let (files, outcome) = check_links(&ctx, &cache, &cfg).compat()?;
    log::debug!(
        "cache hits: {}, cache misses: {}",
        cache.cache_hits(),
        cache.cache_misses()
    );
    let diags = outcome.generate_diagnostics(&files, cfg.warning_policy);
    report_errors(&files, &diags, colour).compat()?;

    save_cache(cache_file, &cache);

    if diags.iter().any(|diag| diag.severity >= Severity::Error) {
        log::info!("{} broken links found", outcome.invalid_links.len());
        Err(failure::err_msg("One or more incorrect links"))
    } else {
        log::info!("No broken links found");
        Ok(())
    }
}

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

fn report_errors(
    files: &Files,
    diags: &[Diagnostic],
    colour: ColorChoice,
) -> Result<(), Error> {
    let mut writer = StandardStream::stderr(colour);
    let cfg = codespan_reporting::term::Config::default();

    for diag in diags {
        codespan_reporting::term::emit(&mut writer, &cfg, files, diag)?;
    }

    Ok(())
}

fn check_links(
    ctx: &RenderContext,
    cache: &Cache,
    cfg: &Config,
) -> Result<(Files, ValidationOutcome), Error> {
    log::info!("Scanning book for links");
    let mut files = Files::new();
    let file_ids = crate::load_files_into_memory(&ctx.book, &mut files);
    let (links, incomplete_links) = crate::extract_links(file_ids, &files);
    log::info!(
        "Found {} links ({} incomplete links)",
        links.len(),
        incomplete_links.len()
    );
    let src = dunce::canonicalize(ctx.source_dir())
        .context("Unable to resolve the source directory")?;
    let outcome =
        crate::validate(&links, &cfg, &src, &cache, &files, incomplete_links)?;

    Ok((files, outcome))
}

fn load_cache(filename: &Path) -> Cache {
    log::debug!("Loading cache from {}", filename.display());

    match File::open(filename) {
        Ok(f) => match Cache::load(f) {
            Ok(cache) => cache,
            Err(e) => {
                log::warn!("Unable to deserialize the cache: {}", e);
                Cache::default()
            },
        },
        Err(e) => {
            log::debug!("Unable to open the cache: {}", e);
            Cache::default()
        },
    }
}

fn save_cache(filename: &Path, cache: &Cache) {
    if let Some(parent) = filename.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("Unable to create the cache's directory: {}", e);
        }
    }

    log::debug!("Saving the cache to {}", filename.display());

    match File::create(filename) {
        Ok(f) => {
            if let Err(e) = cache.save(f) {
                log::warn!("Saving the cache as JSON failed: {}", e);
            }
        },
        Err(e) => log::warn!("Unable to create the cache file: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_stay_compatible_with_mdbook_dependency() {
        version_check(mdbook::MDBOOK_VERSION).unwrap();
    }
}
