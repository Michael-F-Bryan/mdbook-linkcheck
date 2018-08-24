use failure::{Error, Fail};
use rayon::iter::{FromParallelIterator, IntoParallelIterator};
use reqwest::StatusCode;
use std::fmt::{self, Display, Formatter};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use url::Url;

/// The error which were generated while checking links.
#[derive(Debug, Fail)]
#[fail(display = "there are broken links")]
pub struct BrokenLinks(Vec<Box<BrokenLink>>);

impl BrokenLinks {
    pub fn links(&self) -> &[Box<BrokenLink>] {
        &self.0
    }
}

impl FromParallelIterator<Box<BrokenLink>> for BrokenLinks {
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: IntoParallelIterator<Item = Box<BrokenLink>>,
    {
        BrokenLinks(Vec::from_par_iter(par_iter))
    }
}

impl FromIterator<Box<BrokenLink>> for BrokenLinks {
    fn from_iter<I: IntoIterator<Item = Box<BrokenLink>>>(it: I) -> BrokenLinks {
        BrokenLinks(it.into_iter().collect())
    }
}

impl IntoIterator for BrokenLinks {
    type Item = Box<BrokenLink>;
    type IntoIter = <Vec<Self::Item> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// An iterator over all the links in [`BrokenLinks`].
///
/// [`BrokenLinks`]: struct.BrokenLinks.html
pub struct Links<'a> {
    parent: &'a BrokenLinks,
    cursor: usize,
}

impl<'a> Iterator for Links<'a> {
    type Item = &'a BrokenLink;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.parent.0.get(self.cursor).map(|b| &**b);
        self.cursor += 1;
        item
    }
}

impl<'a> IntoIterator for &'a BrokenLinks {
    type Item = &'a BrokenLink;
    type IntoIter = Links<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Links {
            parent: self,
            cursor: 0,
        }
    }
}

/// Some arbitrary broken link which occurs at a specific line in a chapter. The
/// `Display` impl should state why the link is "broken".
pub trait BrokenLink: Fail {
    /// Which chapter it was in.
    fn chapter(&self) -> &Path;
    /// The line this error occurred on.
    fn line(&self) -> usize;
    fn as_fail(&self) -> &Fail;
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

            fn as_fail(&self) -> &Fail {
                self
            }
        }
    };
}

impl_broken_link!(EmptyLink);
impl_broken_link!(FileNotFound);
impl_broken_link!(HttpError);
impl_broken_link!(UnsuccessfulStatus);
impl_broken_link!(ForbiddenPath);

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
        write!(f, "\"{}\" returned {}", self.url, self.code,)
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
        write!(f, "\"{}\" doesn't exist", self.path.display(),)
    }
}

/// An error occurred while trying to fetch the link from the internet.
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
            self.url, self.error,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Fail)]
pub struct ForbiddenPath {
    pub path: PathBuf,
    pub chapter: PathBuf,
    pub line: usize,
}

impl ForbiddenPath {
    pub(crate) fn new<P, Q>(path: P, chapter: Q, line: usize) -> ForbiddenPath
    where
        P: Into<PathBuf>,
        Q: Into<PathBuf>,
    {
        let path = path.into();
        let chapter = chapter.into();

        ForbiddenPath {
            path,
            chapter,
            line,
        }
    }
}

impl Display for ForbiddenPath {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "\"{}\" goes outside the book's source directory",
            self.path.display(),
        )
    }
}
