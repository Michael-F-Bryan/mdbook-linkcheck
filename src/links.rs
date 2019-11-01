use codespan::{ByteIndex, FileId, Files, Span};
use http::uri::{Parts, Uri};
use pulldown_cmark::{Event, OffsetIter, Parser, Tag};
use std::{
    cell::RefCell,
    fmt::Debug,
    path::{Component, Path, PathBuf},
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

        let path = decoded_path(self.uri.path());

        if path.has_root() {
            // absolute paths are resolved by joining the root and the path.
            // Note that you can't use Path::join() with another absolute path
            concat_paths(root_dir, &path)
        } else {
            // This link is relative to the file it was written in (or rather,
            // that file's parent directory)
            let src_file = Path::new(files.name(self.file));
            let src_file = if src_file.is_relative() {
                root_dir.join(src_file)
            } else {
                src_file.to_path_buf()
            };
            let parent_dir =
                src_file.parent().unwrap_or_else(|| Path::new("."));
            concat_paths(parent_dir, &path)
        }
    }
}

/// Concatenate two paths, skipping any prefix components (e.g. `C:` or `/`) in
/// the second path.
fn concat_paths(root: &Path, tail: &Path) -> PathBuf {
    let mut path = root.to_path_buf();

    let tail = tail.components().skip_while(|cmp| match cmp {
        Component::RootDir | Component::Prefix(_) => true,
        _ => false,
    });
    path.extend(tail);

    path
}

fn decoded_path(percent_encoded_path: &str) -> PathBuf {
    percent_encoding::percent_decode_str(percent_encoded_path)
        .decode_utf8()
        .ok()
        .map_or_else(
            || PathBuf::from(percent_encoded_path),
            |p| PathBuf::from(p.into_owned()),
        )
}

/// Search every file in the [`Files`] and collate all the links that are
/// found.
pub fn extract<I>(
    target_files: I,
    files: &Files,
) -> (Vec<Link>, Vec<IncompleteLink>)
where
    I: IntoIterator<Item = FileId>,
{
    let mut links = Vec::new();
    let broken_links = RefCell::new(Vec::new());

    for file_id in target_files {
        let cb = on_broken_links(file_id, &broken_links);
        log::debug!("Scanning {}", files.name(file_id));
        links.extend(Links::new(file_id, files, &cb));
    }

    (links, broken_links.into_inner())
}

/// Get a closure which can be used as the broken links callback, adding a new
/// [`IncompleteLink`] to the list.
fn on_broken_links<'a>(
    file: FileId,
    dest: &'a RefCell<Vec<IncompleteLink>>,
) -> impl Fn(&str, &str) -> Option<(String, String)> + 'a {
    move |raw, _| {
        log::debug!("Found a (possibly) broken link to [{}]", raw);

        dest.borrow_mut().push(IncompleteLink {
            text: raw.to_string(),
            file,
        });
        None
    }
}

/// A potential link that has a broken reference (e.g `[foo]` when there is no
/// `[foo]: ...` entry at the bottom).
#[derive(Debug, Clone, PartialEq)]
pub struct IncompleteLink {
    /// The reference name (e.g. the `foo` in `[foo]`).
    pub text: String,
    /// Which file was the incomplete link found in?
    pub file: FileId,
}

struct Links<'a> {
    events: OffsetIter<'a>,
    file: FileId,
    files: &'a Files,
}

impl<'a> Links<'a> {
    fn new(
        file: FileId,
        files: &'a Files,
        cb: &'a dyn Fn(&str, &str) -> Option<(String, String)>,
    ) -> Links<'a> {
        let src = files.source(file);
        Links {
            events: Parser::new_with_broken_link_callback(
                src,
                pulldown_cmark::Options::all(),
                Some(cb),
            )
            .into_offset_iter(),
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
                        "Found \"{}\" at {}..{} of file {:?}",
                        dest,
                        range.start,
                        range.end,
                        self.file,
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

        let got: Vec<Link> = Links::new(id, &files, &|_, _| None).collect();

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

    #[test]
    fn link_path_with_percent_encoding() {
        let uri = "./TechNote%20094%20Accessing%20Wintech%20download%20site%20Rev%20A.pdf";
        let should_be = Path::new(
            "./TechNote 094 Accessing Wintech download site Rev A.pdf",
        );

        let got = decoded_path(uri);

        assert_eq!(got, should_be);
    }
}
