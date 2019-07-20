use crate::{
    cache::{Cache, CacheEntry},
    Config, Link,
};
use either::Either;
use failure::Error;
use http::HeaderMap;
use rayon::prelude::*;
use reqwest::Client;
use std::{
    path::Path,
    time::{Duration, SystemTime},
};

#[allow(unused_imports)]
use http::Uri;

/// Try to validate the provided [`Link`]s.
pub fn validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    cache: &Cache,
) -> Result<ValidationOutcome, Error> {
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
        validate_web_links(&buckets.file, cfg, &mut outcome, cache)?;
    } else {
        outcome.ignored.extend(buckets.web);
    }

    Ok(outcome)
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
    debug_assert!(
        root_dir.is_absolute(),
        "The root directory should be absolute"
    );

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
        } else if path.is_file()
            || (path.is_dir() && path.join("index.md").is_file())
        {
            // e.g. a normal file, or "./some-dir/" -> "./some-dir/index.md"
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

fn validate_web_links(
    links: &[Link],
    cfg: &Config,
    outcome: &mut ValidationOutcome,
    cache: &Cache,
) -> Result<(), Error> {
    let client = create_client(cfg)?;

    let (valid, invalid): (Vec<_>, Vec<_>) =
        links.par_iter().partition_map(|link| {
            match check_link(link, &client, cfg, cache) {
                Ok(_) => Either::Left(link.clone()),
                Err(e) => Either::Right(InvalidLink {
                    link: link.clone(),
                    reason: e,
                }),
            }
        });

    outcome.valid_links.extend(valid);
    outcome.invalid_links.extend(invalid);

    Ok(())
}

fn create_client(cfg: &Config) -> Result<Client, Error> {
    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::USER_AGENT, cfg.user_agent.parse()?);

    let client = Client::builder()
        .use_sys_proxy()
        .default_headers(headers)
        .build()?;

    Ok(client)
}

fn check_link(
    link: &Link,
    client: &Client,
    cfg: &Config,
    cache: &Cache,
) -> Result<(), Reason> {
    let url = link.uri.to_string();

    if let Some(entry) = cache.lookup(&url) {
        if entry.successful
            && entry.elapsed() < Duration::from_secs(cfg.cache_timeout)
        {
            return Ok(());
        }
    }

    log::trace!("Sending a GET request to \"{}\"", url);

    match client.get(&url).send() {
        Ok(ref response) if response.status().is_success() => {
            cache.insert(url, CacheEntry::new(SystemTime::now(), true));
            Ok(())
        },
        Ok(response) => {
            let status = response.status();
            log::trace!("\"{}\" replied with {}", url, status);
            Err(Reason::UnsuccessfulServerResponse(status))
        },
        Err(e) => Err(Reason::Client(e)),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Buckets {
    web: Vec<Link>,
    file: Vec<Link>,
}

/// The outcome of validating a set of links.
#[derive(Debug, Default)]
pub struct ValidationOutcome {
    /// Valid links.
    pub valid_links: Vec<Link>,
    pub invalid_links: Vec<InvalidLink>,
    pub ignored: Vec<Link>,
    /// Links which we don't know how to handle.
    pub unknown_schema: Vec<Link>,
}

/// An invalid [`Link`] and the [`Reason`] for why it isn't valid.
#[derive(Debug)]
pub struct InvalidLink {
    pub link: Link,
    pub reason: Reason,
}

#[derive(Debug)]
pub enum Reason {
    FileNotFound,
    TraversesParentDirectories,
    UnsuccessfulServerResponse(reqwest::StatusCode),
    Client(reqwest::Error),
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
