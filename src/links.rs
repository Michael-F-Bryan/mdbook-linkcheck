use codespan::{ByteIndex, ByteOffset, ByteSpan, CodeMap, FileMap};
use pulldown_cmark::{Event, OffsetIter, Parser, Tag};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: Url,
    pub span: ByteSpan,
}

pub fn extract_links(map: &CodeMap) -> Vec<Link> {
    map.iter().flat_map(|f| Links::new(f)).collect()
}

struct Links<'a, S> {
    events: OffsetIter<'a>,
    file: &'a FileMap<S>,
    base_offset: ByteOffset,
}

impl<'a, S: AsRef<str> + 'a> Links<'a, S> {
    fn new(file: &'a FileMap<S>) -> Links<'a, S> {
        Links {
            events: Parser::new(file.src().as_ref()).into_offset_iter(),
            file,
            base_offset: ByteOffset(file.span().start().0 as i64),
        }
    }

    fn process_link(
        &self,
        url: &str,
        range: std::ops::Range<usize>,
    ) -> Option<Link> {
        let start = ByteIndex(range.start as u32) + self.base_offset;
        let end = ByteIndex(range.end as u32) + self.base_offset;
        let span = ByteSpan::new(start, end);

        log::trace!("Found \"{}\" at {}", url, span);

        match Url::parse(url) {
            Ok(url) => Some(Link { url, span }),
            Err(e) => {
                let line = self
                    .file
                    .find_line(start)
                    .expect("The span should always be in this file");
                log::warn!(
                    "Unable to parse \"{}\" as a URL on line {}: {}",
                    url,
                    line.number(),
                    e
                );

                None
            },
        }
    }
}

impl<'a, S: AsRef<str> + 'a> Iterator for Links<'a, S> {
    type Item = Link;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((event, range)) = self.events.next() {
            println!("{:?} @ {:?}", event, range);

            match event {
                Event::Start(Tag::Link(_, dest, _))
                | Event::Start(Tag::Image(_, dest, _)) => {
                    if let Some(link) = self.process_link(&*dest, range) {
                        return Some(link);
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
        let src = "This is a link to [nowhere](http://doesnt.exist/)";
        let file = FileMap::new(FileName::virtual_("whatever"), src);
        let link = Url::parse("http://doesnt.exist/").unwrap();

        let got: Vec<Link> = Links::new(&file).collect();

        assert_eq!(got.len(), 1);

        // Depends on https://github.com/raphlinus/pulldown-cmark/issues/378
        // let start = ByteOffset(file.span().start().to_usize() as i64);
        // let should_be = Link {
        //     url: link,
        //     span: ByteSpan::new(ByteIndex(19) + start, ByteIndex(20) start),
        // };
        // assert_eq!(got[0], should_be);
        assert_eq!(got[0].url, link);
    }
}
