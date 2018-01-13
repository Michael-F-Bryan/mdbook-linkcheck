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

use std::path::{Path, PathBuf};
use std::fmt::{self, Display, Formatter};
use std::ffi::OsStr;
use failure::{Error, ResultExt};
use pulldown_cmark::{Event, Parser, Tag};
use memchr::Memchr;
use mdbook::renderer::RenderContext;
use mdbook::book::{BookItem, Chapter};
use reqwest::StatusCode;
use url::Url;

/// The exact version of `mdbook` this crate is compiled against.
pub const MDBOOK_VERSION: &'static str = env!("MDBOOK_VERSION");

/// The main entrypoint for this crate.
///
/// If there were any broken links then you'll be able to downcast the `Error`
/// returned into `BrokenLinks`.
pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    info!("Checking for broken links");

    let cfg: Config = ctx.config
        .get_deserialized("output.linkcheck")
        .sync()
        .context("Unable to deserialize the `output.linkcheck` table. Is it in your book.toml?")?;

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
            if let Err(e) = check_link(link, &ctx, &cfg) {
                trace!("Error for {}, {}", link, e);
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

/// The error which were generated while checking links.
#[derive(Debug, Fail)]
#[fail(display = "there are broken links")]
pub struct BrokenLinks(pub Vec<Error>);

/// The configuration options available with this backend.
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
        let content = &self.chapter.content;
        if self.offset > content.len() {
            panic!(
                "Link has invalid offset. Got {} but chapter is only {} bytes long.",
                self.offset,
                self.chapter.content.len()
            );
        }

        Memchr::new(b'\n', content[..self.offset].as_bytes()).count() + 1
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

fn check_link(link: &Link, ctx: &RenderContext, cfg: &Config) -> Result<(), Error> {
    trace!("Checking {}", link);

    if link.url.is_empty() {
        let err = EmptyLink::new(&link.chapter.path, link.line_number());
        return Err(Error::from(err));
    }

    match Url::parse(&link.url) {
        Ok(link_url) => validate_external_link(link_url, cfg),
        Err(_) => check_link_in_book(link, ctx),
    }
}


/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "Empty Link")]
pub struct EmptyLink {
    chapter: PathBuf,
    line: usize,
}

impl EmptyLink {
    fn new<P>(chapter: P, line: usize) -> EmptyLink
    where
        P: Into<PathBuf>,
    {
        let chapter = chapter.into();

        EmptyLink { chapter, line }
    }
}


fn validate_external_link(url: Url, cfg: &Config) -> Result<(), Error> {
    if cfg.follow_web_links {
        debug!("Fetching \"{}\"", url);

        let response = reqwest::get(url.clone())?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            trace!("Unsuccessful Status {} for {}", status, url);
            Err(Error::from(UnsuccessfulStatus(status)))
        }
    } else {
        debug!("Ignoring \"{}\"", url);
        Ok(())
    }
}


/// Received an unsuccessful status code when fetching a resource from the
/// internet.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "{}", _0)]
pub struct UnsuccessfulStatus(pub StatusCode);

fn check_link_in_book(link: &Link, ctx: &RenderContext) -> Result<(), Error> {
    let path = Path::new(&link.url);

    let extension = path.extension();
    if extension == Some(OsStr::new("md")) {
        // linking to a `*.md` file is an error because we don't (yet)
        // automatically translate these links into `*.html`.
        let err = MdSuggestion::new(path, &link.chapter.path, link.line_number());
        Err(Error::from(err))
    } else if extension == Some(OsStr::new("html")) {
        check_link_to_chapter(link, ctx)
    } else {
        check_asset_link_is_valid(link, ctx)
    }
}

fn check_link_to_chapter(link: &Link, ctx: &RenderContext) -> Result<(), Error> {
    let path = match link.url.find("#") {
        Some(ix) => &link.url[..ix],
        None => &link.url,
    };

    let src = ctx.root.join(&ctx.config.book.src);

    // note: all chapter links are relative to the `src/` directory
    let chapter_path = src.join(path).with_extension("md");
    debug!("Searching for {}", chapter_path.display());

    if chapter_path.exists() {
        Ok(())
    } else {
        Err(Error::from(FileNotFound::new(path, &link.chapter.path, link.line_number())))
    }
}

/// Check the link is to a valid asset inside the book's `src/` directory. The
/// HTML renderer will copy this to the destination directory accordingly.
fn check_asset_link_is_valid(link: &Link, ctx: &RenderContext) -> Result<(), Error> {
    let path = Path::new(&link.url);
    let src = ctx.root.join(&ctx.config.book.src);

    debug_assert!(
        src.is_absolute(),
        "mdbook didn't give us the book root's absolute path"
    );

    let full_path = if path.is_absolute() {
        src.join(&path)
    } else {
        let directory = match link.chapter.path.parent() {
            Some(parent) => src.join(parent),
            None => src.clone(),
        };

        directory.join(&path)
    };

    // by this point we've resolved the link relative to the source chapter's
    // directory, and turned it into an absolute path. This *should* match a
    // file on disk.
    debug!("Searching for {}", full_path.display());

    match full_path.canonicalize() {
        Err(_) => Err(Error::from(FileNotFound::new(path, &link.chapter.path, link.line_number()))),
        Ok(p) => if p.exists() {
            Ok(())
        } else {
            Err(Error::from(FileNotFound::new(p, &link.chapter.path, link.line_number())))
        },
    }
}

/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "File Not Found")]
pub struct FileNotFound {
    path: PathBuf,
    chapter: PathBuf,
    line: usize,
}

impl FileNotFound {
    fn new<P, Q>(path: P, chapter: Q, line: usize) -> FileNotFound
    where
        P: Into<PathBuf>,
        Q: Into<PathBuf>,
    {
        let path = path.into();
        let chapter = chapter.into();

        FileNotFound { path, chapter, line }
    }
}

/// The user specified a `*.md` file when they probably meant `*.html`.
#[derive(Debug, Clone, PartialEq, Fail)]
pub struct MdSuggestion {
    found: PathBuf,
    suggested: PathBuf,
    chapter: PathBuf,
    line: usize,
}

impl MdSuggestion {
    fn new<P, Q>(original: P, chapter: Q, line: usize) -> MdSuggestion
    where
        P: Into<PathBuf>,
        Q: Into<PathBuf>,
    {
        let found = original.into();
        let suggested = found.with_extension("html");
        let chapter = chapter.into();

        MdSuggestion {
            found,
            suggested,
            chapter,
            line,
        }
    }
}

impl Display for MdSuggestion {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Found \"{}\" at {}#{}, did you mean \"{}\"?",
            self.found.display(),
            self.chapter.display(),
            self.line,
            self.suggested.display(),
        )
    }
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
