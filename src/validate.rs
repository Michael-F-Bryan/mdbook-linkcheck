use crate::{Cache, Config, Link};
use std::path::Path;

#[allow(unused_imports)]
use http::Uri;

pub fn validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    _cache: &Cache,
) -> ValidationOutcome {
    let mut outcome = ValidationOutcome::default();

    let buckets =
        sort_into_buckets(links, |link| outcome.unknown_schema.push(link));

    validate_local_links(
        &buckets.file,
        cfg.traverse_parent_directories,
        src_dir,
        &mut outcome,
    );

    if cfg.follow_web_links {
        unimplemented!()
    } else {
        outcome.ignored.extend(buckets.web);
    }

    outcome
}

fn sort_into_buckets<F: FnMut(Link)>(
    links: &[Link],
    mut unknown_schema: F,
) -> Buckets {
    let mut buckets = Buckets::default();

    for link in links {
        match link.uri.scheme_str() {
            Some("http") | Some("https") => buckets.web.push(link.clone()),
            None | Some("file") => buckets.file.push(link.clone()),
            _ => unknown_schema(link.clone()),
        }
    }

    buckets
}

fn validate_local_links(
    links: &[Link],
    traverse_parent_directories: bool,
    root_dir: &Path,
    outcome: &mut ValidationOutcome,
) {
    for link in links {
        let path = link.as_filesystem_path(root_dir);
        let link = link.clone();

        log::trace!("Checking \"{}\"", path.display());

        if !path.starts_with(root_dir) && !traverse_parent_directories {
            log::trace!(
                "It lies outside the root directory and that is forbidden"
            );
            outcome.invalid_links.push(InvalidLink {
                link,
                reason: Reason::TraversesParentDirectories,
            });
        } else if path.is_file() {
            outcome.valid_links.push(link);
        } else if path.is_dir() && path.join("index.md").is_file() {
            // e.g. "./some-dir/" -> "./some-dir/index.md"
            outcome.valid_links.push(link);
        } else {
            log::trace!("It doesn't exist");
            outcome.invalid_links.push(InvalidLink {
                link,
                reason: Reason::FileNotFound,
            });
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Buckets {
    web: Vec<Link>,
    file: Vec<Link>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ValidationOutcome {
    /// Valid links.
    pub valid_links: Vec<Link>,
    pub invalid_links: Vec<InvalidLink>,
    pub ignored: Vec<Link>,
    /// Links which we don't know how to handle.
    pub unknown_schema: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvalidLink {
    pub link: Link,
    pub reason: Reason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Reason {
    FileNotFound,
    TraversesParentDirectories,
}

/// An unknown [`Uri::scheme_str()`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownScheme(pub Link);

#[cfg(test)]
mod tests {
    use super::*;
    use codespan::{ByteOffset, FileMap, FileName};
    use std::sync::Arc;

    #[test]
    fn sort_links_into_buckets() {
        let links = vec![Link::parse(
            "path/to/file.md",
            0..1,
            Arc::new(FileMap::new(FileName::from(""), String::new())),
            ByteOffset(0),
        )
        .unwrap()];

        let got = sort_into_buckets(&links, |unknown| {
            panic!("Unknown schema: {:?}", unknown)
        });

        assert_eq!(got.file.len(), 1);
        assert_eq!(got.file[0], links[0]);
    }
}
