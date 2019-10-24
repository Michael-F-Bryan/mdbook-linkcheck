use codespan::{ByteIndex, FileId, Files, Span};
use http::uri::{Parts, Uri};
use pulldown_cmark::{Event, OffsetIter, Parser, Tag};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

/// A single link, and where it was found in the parent document.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// The link itself.
    pub uri: Uri,
    /// Where the link lies in its original text.
    pub span: Span,
    /// The file this link was originally found in.
    pub file: FileId,
}

impl Link {
    pub(crate) fn parse(
        uri: &str,
        range: std::ops::Range<usize>,
        file: FileId,
    ) -> Result<Link, http::Error> {
        let start = ByteIndex(range.start as u32);
        let end = ByteIndex(range.end as u32);
        let span = Span::new(start, end);

        // it might be a valid URI already
        if let Ok(uri) = uri.parse() {
            return Ok(Link { uri, span, file });
        }

        // otherwise, treat it like a relative path with no authority or scheme
        let mut parts = Parts::default();
        parts.path_and_query = Some(uri.parse()?);
        let uri = Uri::from_parts(parts)?;

        Ok(Link { uri, span, file })
    }

    pub(crate) fn as_filesystem_path(
        &self,
        root_dir: &Path,
        files: &Files,
    ) -> PathBuf {
        debug_assert!(
            self.uri.scheme_str().is_none()
                || self.uri.scheme_str() == Some("file"),
            "this operation only makes sense for file URIs"
        );

        let path = Path::new(self.uri.path());

        if path.is_absolute() {
            // absolute paths are resolved by joining the root and the path.
            // Note that you can't use Path::join() with another absolute path
            let mut full_path = root_dir.to_path_buf();
            full_path.extend(
                path.components()
                    .filter(|&c| c != std::path::Component::RootDir),
            );
            full_path
        } else {
            // This link is relative to the file it was written in (or rather,
            // that file's parent directory)
            let parent_dir = match Path::new(files.name(self.file)).parent() {
                Some(p) => root_dir.join(p),
                None => root_dir.to_path_buf(),
            };
            let got = parent_dir.join(path);
            got
        }
    }
}

/// Search every file in the [`CodeMap`] and collate all the links that are
/// found.
pub fn extract_links<I>(target_files: I, files: &Files) -> Vec<Link>
where
    I: IntoIterator<Item = FileId>,
{
    target_files
        .into_iter()
        .flat_map(|id| Links::new(id, files))
        .collect()
}

struct Links<'a> {
    events: OffsetIter<'a>,
    file: FileId,
    files: &'a Files,
}

impl<'a> Links<'a> {
    fn new(file: FileId, files: &'a Files) -> Links<'a> {
        let src = files.source(file);
        Links {
            events: Parser::new(src).into_offset_iter(),
            file,
            files,
        }
    }
}

impl<'a> Iterator for Links<'a> {
    type Item = Link;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::Start(Tag::Link(_, dest, _))
                | Event::Start(Tag::Image(_, dest, _)) => {
                    log::trace!(
                        "Found \"{}\" at {}..{}",
                        dest,
                        range.start,
                        range.end
                    );

                    match Link::parse(&dest, range.clone(), self.file) {
                        Ok(link) => return Some(link),
                        Err(e) => {
                            let location = self
                                .files
                                .location(self.file, range.start as u32)
                                .unwrap();
                            log::warn!( "Unable to parse \"{}\" as a URI on line {}: {}", dest, location.line, e);

                            continue;
                        },
                    }
                },
                _ => {},
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_the_most_basic_link() {
        let src = "This is a link to [nowhere](http://doesnt.exist/)";
        let link: Uri = "http://doesnt.exist/".parse().unwrap();
        let mut files = Files::new();
        let id = files.add("whatever", src);

        let got: Vec<Link> = Links::new(id, &files).collect();

        assert_eq!(got.len(), 1);

        // Depends on https://github.com/raphlinus/pulldown-cmark/issues/378
        // let start = ByteOffset(file.span().start().to_usize() as i64);
        // let should_be = Link {
        //     url: link,
        //     span: Span::new(ByteIndex(19) + start, ByteIndex(20) start),
        // };
        // assert_eq!(got[0], should_be);
        assert_eq!(got[0].uri, link);
    }
}
