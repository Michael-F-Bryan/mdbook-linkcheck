use mdbook::book::Chapter;
use memchr::Memchr;
use pulldown_cmark::{Event, Parser, Tag};
use std::fmt::{self, Display, Formatter};
use std::cmp::Ordering;

/// Information about a link in one of the book's chapters.
#[derive(Debug, Clone)]
pub struct Link<'a> {
    pub url: String,
    pub offset: usize,
    pub chapter: &'a Chapter,
}

impl<'a> Link<'a> {
    pub fn line_number(&self) -> usize {
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

impl<'a> PartialEq for Link<'a> {
    fn eq(&self, other: &Link<'a>) -> bool {
        self.url.eq(&other.url)
    }
}

impl<'a> Eq for Link<'a> {}

impl<'a> PartialOrd for Link<'a> {
    fn partial_cmp(&self, other: &Link<'a>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for Link<'a> {
    fn cmp(&self, other: &Link<'a>) -> Ordering {
        self.url.cmp(&other.url)
    }
}


/// Find all the links in a particular chapter.
pub fn collect_links(ch: &Chapter) -> Vec<Link> {
    let mut links = Vec::new();
    let mut parser = Parser::new(&ch.content);

    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Link(_, dest, _)) | Event::Start(Tag::Image(_, dest, _)) => {
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
    
    // Remove duplicate links
    links.sort_unstable();
    links.dedup();

    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdbook::book::Chapter;

    #[test]
    fn find_links_in_chapter() {
        let src = "[Reference other chapter](index.html) and [Google](https://google.com)";
        let ch = Chapter::new("Foo", src.to_string(), "index.md", Vec::new());

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
