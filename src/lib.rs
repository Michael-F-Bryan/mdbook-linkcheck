//! A `mdbook` backend which will check all links in a document are valid.

extern crate mdbook;
#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate pulldown_cmark;

use failure::Error;
use mdbook::renderer::RenderContext;

/// The exact version of `mdbook` this crate is compiled against.
pub const MDBOOK_VERSION: &'static str = env!("MDBOOK_VERSION");

/// The main entrypoint for this crate.
pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    info!("Checking for broken links");

    unimplemented!()
}


/// A collection of broken links.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "several broken links were found")]
pub struct BrokenLinks {
    pub links: Vec<BrokenLink>,
}

/// A broken link.
#[derive(Debug, Clone, PartialEq)]
pub struct BrokenLink {
    pub url: String,
    pub chapter: String,
}