use std::path::{Path, PathBuf};
use std::fmt::{self, Display, Formatter};
use reqwest::StatusCode;
use failure::{Error, Fail};
use url::Url;

/// The error which were generated while checking links.
#[derive(Debug, Fail)]
#[fail(display = "there are broken links")]
pub struct BrokenLinks(pub Vec<Box<BrokenLink>>);

/// Some arbitrary broken link which occurs at a specific line in a chapter. The
/// `Display` impl should state why the link is "broken".
pub trait BrokenLink: Fail {
    /// Which chapter it was in.
    fn chapter(&self) -> &Path;
    /// The line this error occurred on.
    fn line(&self) -> usize;
}

macro_rules! impl_broken_link {
    ($name:ty) => {
        impl BrokenLink for $name {
            fn line(&self) -> usize {
                self.line
            }

            fn chapter(&self) -> &Path {
                &self.chapter
            }
        }
    }
}

impl_broken_link!(EmptyLink);
impl_broken_link!(FileNotFound);
impl_broken_link!(HttpError);
impl_broken_link!(MdSuggestion);
impl_broken_link!(UnsuccessfulStatus);

/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
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

impl Display for EmptyLink {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "The link is empty")
    }
}

/// Received an unsuccessful status code when fetching a resource from the
/// internet.
#[derive(Debug, Clone, PartialEq, Fail)]
pub struct UnsuccessfulStatus {
    pub url: Url,
    pub code: StatusCode,
    pub chapter: PathBuf,
    pub line: usize,
}

impl UnsuccessfulStatus {
    pub(crate) fn new<P>(url: Url, code: StatusCode, chapter: P, line: usize) -> UnsuccessfulStatus
    where
        P: Into<PathBuf>,
    {
        let chapter = chapter.into();

        UnsuccessfulStatus {
            url,
            code,
            chapter,
            line,
        }
    }
}

impl Display for UnsuccessfulStatus {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "\"{}\" returned {}",
            self.url,
            self.code,
        )
    }
}

/// The user specified a file which doesn't exist.
#[derive(Debug, Clone, PartialEq, Fail)]
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

impl Display for FileNotFound {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "\"{}\" doesn't exist",
            self.path.display(),
        )
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
            "Found \"{}\", did you mean \"{}\"?",
            self.found.display(),
            self.suggested.display(),
        )
    }
}

#[derive(Debug, Fail)]
pub struct HttpError {
    pub url: Url,
    pub chapter: PathBuf,
    pub line: usize,
    pub error: Error,
}

impl HttpError {
    pub(crate) fn new<P, E>(url: Url, chapter: P, line: usize, error: E) -> HttpError
    where
        P: Into<PathBuf>,
        E: Into<Error>,
    {
        let chapter = chapter.into();
        let error = error.into();

        HttpError {
            url,
            chapter,
            line,
            error,
        }
    }
}

impl Display for HttpError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "There was an error while fetching \"{}\", {}",
            self.url,
            self.error,
        )
    }
}
