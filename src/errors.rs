use std::path::PathBuf;
use std::fmt::{self, Display, Formatter};
use reqwest::StatusCode;
use failure::Error;

/// The error which were generated while checking links.
#[derive(Debug, Fail)]
#[fail(display = "there are broken links")]
pub struct BrokenLinks(pub Vec<Error>);

/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "Empty Link")]
pub struct EmptyLink {
    pub chapter: PathBuf,
    pub line: usize,
}

impl EmptyLink {
    pub(crate) fn new<P>(chapter: P, line: usize) -> EmptyLink
    where
        P: Into<PathBuf>,
    {
        let chapter = chapter.into();

        EmptyLink { chapter, line }
    }
}

/// Received an unsuccessful status code when fetching a resource from the
/// internet.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "{}", _0)]
pub struct UnsuccessfulStatus(pub StatusCode);

/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
#[fail(display = "File Not Found")]
pub struct FileNotFound {
    pub path: PathBuf,
    pub chapter: PathBuf,
    pub line: usize,
}

impl FileNotFound {
    pub(crate) fn new<P, Q>(path: P, chapter: Q, line: usize) -> FileNotFound
    where
        P: Into<PathBuf>,
        Q: Into<PathBuf>,
    {
        let path = path.into();
        let chapter = chapter.into();

        FileNotFound {
            path,
            chapter,
            line,
        }
    }
}

/// The user specified a `*.md` file when they probably meant `*.html`.
#[derive(Debug, Clone, PartialEq, Fail)]
pub struct MdSuggestion {
    pub found: PathBuf,
    pub suggested: PathBuf,
    pub chapter: PathBuf,
    pub line: usize,
}

impl MdSuggestion {
    pub(crate) fn new<P, Q>(original: P, chapter: Q, line: usize) -> MdSuggestion
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
