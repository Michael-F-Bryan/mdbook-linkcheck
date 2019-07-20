use codespan::{ByteIndex, ByteOffset, ByteSpan, CodeMap, FileMap};
use http::uri::{Parts, Uri};
use pulldown_cmark::{Event, OffsetIter, Parser, Tag};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// A single link, and where it was found in the parent document.
#[derive(Debug, Clone)]
pub struct Link {
    pub uri: Uri,
    pub span: ByteSpan,
    pub file: Arc<FileMap>,
}

impl Link {
    pub(crate) fn parse(
        uri: &str,
        range: std::ops::Range<usize>,
        file: Arc<FileMap>,
        base_offset: ByteOffset,
    ) -> Result<Link, http::Error> {
        let start = ByteIndex(range.start as u32) + base_offset;
        let end = ByteIndex(range.end as u32) + base_offset;
        let span = ByteSpan::new(start, end);

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

    pub fn as_filesystem_path(&self, root_dir: &Path) -> PathBuf {
        debug_assert!(
            self.uri.scheme_str().is_none()
                || self.uri.scheme_str() == Some("file"),
            "this operation only makes sense for file URIs"
        );

        let path = Path::new(self.uri.path());

        if path.is_absolute() {
            root_dir.join(path)
        } else {
            // This link is relative to the file it was written in (or rather,
            // that file's parent directory)
            let parent_dir =
                self.file.name().as_ref().parent().unwrap_or(root_dir);
            parent_dir.join(path)
        }
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
            && self.span == other.span
            && Arc::ptr_eq(&self.file, &other.file)
    }
}

/// Search every file in the [`CodeMap`] and collate all the links that are
/// found.
pub fn extract_links(map: &CodeMap) -> Vec<Link> {
    map.iter().flat_map(|f| Links::new(f)).collect()
}

struct Links<'a> {
    events: OffsetIter<'a>,
    file: &'a Arc<FileMap>,
    base_offset: ByteOffset,
}

impl<'a> Links<'a> {
    fn new(file: &'a Arc<FileMap>) -> Links<'a> {
        Links {
            events: Parser::new(file.src().as_ref()).into_offset_iter(),
            file,
            base_offset: ByteOffset(i64::from(file.span().start().0)),
        }
    }
}

impl<'a> Iterator for Links<'a> {
    type Item = Link;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((event, range)) = self.events.next() {
            println!("{:?} @ {:?}", event, range);

            match event {
                Event::Start(Tag::Link(_, dest, _))
                | Event::Start(Tag::Image(_, dest, _)) => {
                    log::trace!(
                        "Found \"{}\" at {}..{}",
                        dest,
                        range.start,
                        range.end
                    );

                    match Link::parse(
                        &dest,
                        range.clone(),
                        Arc::clone(&self.file),
                        self.base_offset,
                    ) {
                        Ok(link) => return Some(link),
                        Err(e) => {
                            let line = self
                                .file
                                .find_line(
                                    ByteIndex(range.start as u32)
                                        + self.base_offset,
                                )
                                .expect(
                                    "The span should always be in this file",
                                );
                            log::warn!( "Unable to parse \"{}\" as a URI on line {}: {}", dest, line.number(), e);

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
    use codespan::FileName;

    #[test]
    fn detect_the_most_basic_link() {
        let src =
            "This is a link to [nowhere](http://doesnt.exist/)".to_string();
        let file = Arc::new(FileMap::new(FileName::virtual_("whatever"), src));
        let link: Uri = "http://doesnt.exist/".parse().unwrap();

        let got: Vec<Link> = Links::new(&file).collect();

        assert_eq!(got.len(), 1);

        // Depends on https://github.com/raphlinus/pulldown-cmark/issues/378
        // let start = ByteOffset(file.span().start().to_usize() as i64);
        // let should_be = Link {
        //     url: link,
        //     span: ByteSpan::new(ByteIndex(19) + start, ByteIndex(20) start),
        // };
        // assert_eq!(got[0], should_be);
        assert_eq!(got[0].uri, link);
    }
}
