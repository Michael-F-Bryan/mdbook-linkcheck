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

// Note: older versions of Rust (e.g. v1.46.0) don't know about "rustdoc" lints
#![allow(unknown_lints)]
#![deny(
    rustdoc::broken_intra_doc_links,
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

/// A semver range specifying which versions of `mdbook` this crate supports.
pub const COMPATIBLE_MDBOOK_VERSIONS: &str = "^0.4.0";

mod config;
mod context;
mod hashed_regex;
mod links;
mod validate;

pub use crate::{
    config::{Config, WarningPolicy},
    context::Context,
    hashed_regex::HashedRegex,
    links::{extract as extract_links, IncompleteLink},
    validate::{validate, NotInSummary, ValidationOutcome},
};

use anyhow::{Context as _, Error};
use codespan::{FileId, Files};
use codespan_reporting::{
    diagnostic::{Diagnostic, Severity},
    term::termcolor::{ColorChoice, StandardStream},
};
use linkcheck::validation::Cache;
use mdbook::{
    book::{Book, BookItem},
    renderer::RenderContext,
};
use semver::{Version, VersionReq};
use std::{fs::File, path::Path};

/// Run the link checking pipeline.
///
/// If `selected_files` is `Some`, then links in the given list of files are
/// checked, rather than checking links in all files.
///
/// If `cache_file` is `Some`, it is used as a cache; otherwise, no caching is
/// used, and any existing cache is ignored.
pub fn run(
    cache_file: Option<&Path>,
    colour: ColorChoice,
    ctx: &RenderContext,
    selected_files: Option<Vec<String>>,
) -> Result<(), Error> {
    let mut cache = if let Some(cache_file) = cache_file {
        load_cache(cache_file)
    } else {
        Cache::default()
    };

    log::info!("Started the link checker");
    log::debug!("Selected file: {:?}", selected_files);

    let cfg = crate::get_config(&ctx.config)?;
    crate::version_check(&ctx.version)?;

    if log::log_enabled!(log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            log::trace!("{}", line);
        }
    }

    let file_filter = |fname: &Path| {
        if let Some(ref selected_files) = selected_files {
            selected_files.contains(&fname.display().to_string())
        } else {
            true
        }
    };

    let (files, outcome) = check_links(&ctx, &mut cache, &cfg, file_filter)?;
    let diags = outcome.generate_diagnostics(&files, cfg.warning_policy);
    report_errors(&files, &diags, colour)?;

    if let Some(cache_file) = cache_file {
        save_cache(cache_file, &cache);
    }

    if diags.iter().any(|diag| diag.severity >= Severity::Error) {
        log::info!("{} broken links found", outcome.invalid_links.len());
        Err(Error::msg("One or more incorrect links"))
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
        Err(Error::msg(msg))
    }
}

/// A helper for reading the chapters of a [`Book`] into memory, filtering out
/// files using the given `filter`.
pub fn load_files_into_memory<F>(
    book: &Book,
    dest: &mut Files<String>,
    filter: F,
) -> Vec<FileId>
where
    F: Fn(&Path) -> bool,
{
    let mut ids = Vec::new();

    for item in book.iter() {
        match item {
            BookItem::Chapter(ref ch) => {
                if let Some(ref path) = ch.path {
                    if filter(&path) {
                        let id = dest.add(
                            path.display().to_string(),
                            ch.content.clone(),
                        );
                        ids.push(id);
                    }
                }
            },
            BookItem::Separator | BookItem::PartTitle(_) => {},
        }
    }

    ids
}

fn report_errors(
    files: &Files<String>,
    diags: &[Diagnostic<FileId>],
    colour: ColorChoice,
) -> Result<(), Error> {
    let mut writer = StandardStream::stderr(colour);
    let cfg = codespan_reporting::term::Config::default();

    for diag in diags {
        codespan_reporting::term::emit(&mut writer, &cfg, files, diag)?;
    }

    Ok(())
}

fn check_links<F>(
    ctx: &RenderContext,
    cache: &mut Cache,
    cfg: &Config,
    file_filter: F,
) -> Result<(Files<String>, ValidationOutcome), Error>
where
    F: Fn(&Path) -> bool,
{
    log::info!("Scanning book for links");
    let mut files = Files::new();
    let file_ids =
        crate::load_files_into_memory(&ctx.book, &mut files, file_filter);
    let (links, incomplete_links) =
        crate::extract_links(file_ids.clone(), &files);
    log::info!(
        "Found {} links ({} incomplete links)",
        links.len(),
        incomplete_links.len()
    );
    let src = dunce::canonicalize(ctx.source_dir())
        .context("Unable to resolve the source directory")?;
    let outcome = crate::validate(
        &links,
        &cfg,
        &src,
        cache,
        &files,
        &file_ids,
        incomplete_links,
    )?;

    Ok((files, outcome))
}

fn load_cache(filename: &Path) -> Cache {
    log::debug!("Loading cache from {}", filename.display());

    match File::open(filename) {
        Ok(f) => match serde_json::from_reader(f) {
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
            if let Err(e) = serde_json::to_writer(f, cache) {
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
        let got = version_check(mdbook::MDBOOK_VERSION);

        assert!(
            got.is_ok(),
            "Incompatible with mdbook dependency: {:#?}",
            got.unwrap_err()
        );
    }
}
