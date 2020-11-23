use codespan::{FileId, Files, Span};
use linkcheck::Link;
use pulldown_cmark::{BrokenLink, CowStr};
use std::{cell::RefCell, fmt::Debug};

/// Search every file in the [`Files`] and collate all the links that are
/// found.
pub fn extract<I>(
    target_files: I,
    files: &Files<String>,
) -> (Vec<Link>, Vec<IncompleteLink>)
where
    I: IntoIterator<Item = FileId>,
{
    let mut links = Vec::new();
    let broken_links = RefCell::new(Vec::new());

    for file_id in target_files {
        let src = files.source(file_id);
        log::debug!("Scanning {}", files.name(file_id).to_string_lossy());

        links.extend(scan_links(file_id, &*src, &mut |broken_link| {
            let BrokenLink {
                reference, span, ..
            } = broken_link;
            log::debug!(
                "Found a (possibly) broken link to [{}] at {:?}",
                reference,
                span
            );

            broken_links.borrow_mut().push(IncompleteLink {
                reference: broken_link.reference.to_string(),
                span: Span::new(span.start as u32, span.end as u32),
                file: file_id,
            });
            None
        }));
    }

    (links, broken_links.into_inner())
}

fn scan_links<'a, F>(
    file_id: FileId,
    src: &'a str,
    cb: &'a mut F,
) -> impl Iterator<Item = Link> + 'a
where
    F: FnMut(BrokenLink<'_>) -> Option<(CowStr<'a>, CowStr<'a>)> + 'a,
{
    linkcheck::scanners::markdown_with_broken_link_callback(src, Some(cb))
        .map(move |(link, span)| Link::new(link, span, file_id))
}

/// A potential link that has a broken reference (e.g `[foo]` when there is no
/// `[foo]: ...` entry at the bottom).
#[derive(Debug, Clone, PartialEq)]
pub struct IncompleteLink {
    /// The reference name (e.g. the `foo` in `[foo]`).
    pub reference: String,
    /// Which file was the incomplete link found in?
    pub file: FileId,
    /// Where this incomplete link occurred in the source text.
    pub span: Span,
}
