//! A `mdbook` backend which will check all links in a document are valid.

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate mdbook;
extern crate pulldown_cmark;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use std::fmt::{self, Display, Formatter};
use failure::Error;
use pulldown_cmark::{Event, Parser, Tag};
use mdbook::renderer::RenderContext;
use mdbook::book::{Book, BookItem, Chapter};

/// The exact version of `mdbook` this crate is compiled against.
pub const MDBOOK_VERSION: &'static str = env!("MDBOOK_VERSION");

/// The main entrypoint for this crate.
pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    info!("Checking for broken links");

    let cfg: Config = ctx.config.get_deserialized("output.linkcheck").sync()?;
    if log_enabled!(::log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            trace!("{}", line);
        }
    }

    debug!("Finding all links");
    let mut links = Vec::new();

    for item in ctx.book.iter() {
        if let BookItem::Chapter(ref ch) = *item {
            let found = collect_links(ch);
            links.extend(found);
        }
    }

    debug!("Found {} links", links.len());
    let mut errors = Vec::new();

    if !links.is_empty() {
        for link in &links {
            if let Err(e) = check_link(link, &ctx.book, &cfg) {
                errors.push(e);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(BrokenLinks(errors).into())
    }
}

#[derive(Debug, Fail)]
#[fail(display = "there are broken links")]
pub struct BrokenLinks(Vec<Error>);

#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub follow_web_links: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Link<'a> {
    url: String,
    offset: usize,
    chapter: &'a Chapter,
}

impl<'a> Link<'a> {
    fn line_number(&self) -> usize {
        if self.offset > self.chapter.content.len() {
            panic!(
                "Link has invalid offset. Got {} but chapter is only {} bytes long.",
                self.offset,
                self.chapter.content.len()
            );
        }

        unimplemented!()
    }
}

impl<'a> Display for Link<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "\"{}\" in {}#{}",
            self.url,
            self.chapter.path.display(),
            self.line_number()
        )
    }
}

/// Find all the links in a particular chapter.
fn collect_links(ch: &Chapter) -> Vec<Link> {
    let mut links = Vec::new();
    let mut parser = Parser::new(&ch.content);

    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Link(dest, _)) | Event::Start(Tag::Image(dest, _)) => {
                let link = Link {
                    url: dest.to_string(),
                    offset: parser.get_offset(),
                    chapter: ch,
                };

                trace!("Found {}", link);
                links.push(link);
            }
            _ => {}
        }
    }

    links
}

fn check_link(link: &Link, book: &Book, cfg: &Config) -> Result<(), Error> {
    unimplemented!();
}

use failure::SyncFailure;
use std::error::Error as StdError;

/// A workaround because `error-chain` errors aren't `Sync`, yet `failure`
/// errors are required to be.
///
/// See also https://github.com/withoutboats/failure/issues/109.
pub trait SyncResult<T, E> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_links_in_chapter() {
        let src = "[Reference other chapter](index.html) and [Google](https://google.com)";
        let ch = Chapter::new("Foo", src.to_string(), "index.md");

        let should_be = vec![
            Link {
                url: String::from("index.html"),
                offset: 1,
                chapter: &ch,
            },
            Link {
                url: String::from("https://google.com"),
                offset: 43,
                chapter: &ch,
            },
        ];

        let got = collect_links(&ch);

        assert_eq!(got, should_be);
    }
}
