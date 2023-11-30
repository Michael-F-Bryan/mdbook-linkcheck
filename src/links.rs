use crate::config::Config;
use crate::latex::{filter_out_latex, ByteIndexMap};
use codespan::{ByteIndex, FileId, Files, Span};
use linkcheck::Link;
use pulldown_cmark::{BrokenLink, CowStr};
use std::{cell::RefCell, fmt::Debug};

/// Search every file in the [`Files`] and collate all the links that are
/// found.
pub fn extract<I>(
    cfg: &Config,
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

        let (src, byte_index_map) = if cfg.latex_support {
            filter_out_latex(src)
        } else {
            (src.clone(), ByteIndexMap::new())
        };

        log::debug!("Scanning {}", files.name(file_id).to_string_lossy());

        let mapspan = |span: Span| {
            Span::new(
                ByteIndex(
                    byte_index_map.resolve(span.start().to_usize() as u32),
                ),
                ByteIndex(byte_index_map.resolve(span.end().to_usize() as u32)),
            )
        };

        links.extend(
            scan_links(file_id, &src, &mut |broken_link| {
                let BrokenLink {
                    reference, span, ..
                } = broken_link;
                log::debug!(
                    "Found a (possibly) broken link to [{}] at {:?}",
                    reference,
                    span
                );

                ////assert!(false, "kek panic, unreachable?");
                //println!(
                //    "start {:?} end {:?} res_a {:?} res_b {:?}",
                //    span.start,
                //    span.end,
                //    ByteIndex(byte_index_map.resolve(span.start as u32)),
                //    ByteIndex(byte_index_map.resolve(span.end as u32))
                //);
                let origspan = Span::new(
                    ByteIndex(span.start as u32),
                    ByteIndex(span.end as u32),
                );
                let span = mapspan(origspan);

                broken_links.borrow_mut().push(IncompleteLink {
                    reference: broken_link.reference.to_string(),
                    span,
                    file: file_id,
                });
                None
            })
            .map(|link| Link::new(link.href, mapspan(link.span), link.file)),
        );
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
