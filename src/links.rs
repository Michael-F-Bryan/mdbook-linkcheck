use codespan::{FileId, Files};
use linkcheck::Link;
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
        let cb = on_broken_links(file_id, &broken_links);
        let src = files.source(file_id);
        log::debug!("Scanning {}", files.name(file_id).to_string_lossy());

        links.extend(
            linkcheck::scanners::markdown_with_broken_link_callback(src, &cb)
                .map(|(link, span)| Link::new(link, span, file_id)),
        );
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
