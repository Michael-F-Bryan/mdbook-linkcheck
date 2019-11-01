use crate::{
    cache::{Cache, CacheEntry},
    Config, IncompleteLink, Link, WarningPolicy,
};
use codespan::{Files, Span};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use either::Either;
use failure::Error;
use http::HeaderMap;
use rayon::prelude::*;
use reqwest::{Client, StatusCode};
use std::{
    ffi::OsStr,
    fmt::{self, Display, Formatter},
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
    files: &Files,
    incomplete_links: Vec<IncompleteLink>,
) -> Result<ValidationOutcome, Error> {
    let mut outcome = ValidationOutcome {
        incomplete_links,
        ..Default::default()
    };

    let buckets =
        sort_into_buckets(links, |link| outcome.unknown_schema.push(link));

    log::debug!("Checking {} local links", buckets.file.len());
    validate_local_links(
        &buckets.file,
        cfg.traverse_parent_directories,
        src_dir,
        &mut outcome,
        files,
    );

    if cfg.follow_web_links {
        log::debug!("Checking {} web links", buckets.web.len());
        let mut web = buckets.web;
        remove_skipped_links(&mut web, &mut outcome, &cfg, files);
        validate_web_links(&web, cfg, &mut outcome, cache)?;
    } else {
        log::debug!("Ignoring {} web links", buckets.web.len());
        outcome.ignored.extend(buckets.web);
    }

    Ok(outcome)
}

/// Removes any web links we'd normally skip, adding them to the list of ignored
/// links.
fn remove_skipped_links(
    links: &mut Vec<Link>,
    outcome: &mut ValidationOutcome,
    cfg: &Config,
    files: &Files,
) {
    links.retain(|link| {
        let uri = link.uri.to_string();

        if cfg.should_skip(&uri) {
            let location =
                files.location(link.file, link.span.start()).unwrap();
            let name = files.name(link.file);
            log::debug!(
                "Skipping \"{}\" in {}, line {}",
                uri,
                name,
                location.line,
            );
            outcome.ignored.push(link.clone());
            false
        } else {
            true
        }
    })
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
    files: &Files,
) {
    debug_assert!(
        root_dir.is_absolute(),
        "The root directory should be absolute"
    );

    for link in links {
        if link.uri.path() == "" {
            // it's a link within the same document
            continue;
        }

        let path = link.as_filesystem_path(root_dir, files);
        match validate_local_link(
            link,
            root_dir,
            &path,
            traverse_parent_directories,
        ) {
            Ok(()) => outcome.valid_links.push(link.clone()),
            Err(e) => outcome.invalid_links.push(e),
        }
    }
}

fn validate_local_link(
    link: &Link,
    root_dir: &Path,
    path: &Path,
    traverse_parent_directories: bool,
) -> Result<(), InvalidLink> {
    let path = match dunce::canonicalize(&path) {
        Ok(p) => p,

        // as a special case markdown files can sometimes be linked to as
        // blah.html
        Err(_) if path.extension() == Some(OsStr::new("html")) => {
            let path = path.with_extension("md");
            return validate_local_link(
                link,
                root_dir,
                &path,
                traverse_parent_directories,
            );
        },

        Err(e) => {
            log::warn!("Unable to canonicalize {}: {}", path.display(), e);
            return Err(InvalidLink {
                link: link.clone(),
                reason: Reason::FileNotFound,
            });
        },
    };

    log::trace!("Checking \"{}\"", path.display());

    if !path.starts_with(root_dir) && !traverse_parent_directories {
        log::trace!("It lies outside the root directory and that is forbidden");
        Err(InvalidLink {
            link: link.clone(),
            reason: Reason::TraversesParentDirectories,
        })
    } else if file_exists(&path) {
        Ok(())
    } else {
        log::trace!("It doesn't exist");
        Err(InvalidLink {
            link: link.clone(),
            reason: Reason::FileNotFound,
        })
    }
}

fn file_exists(path: &Path) -> bool {
    if path.is_file() {
        return true;
    }

    // as a special case, handle links to the rendered html file
    if path.extension() == Some("html".as_ref())
        && path.with_extension("md").is_file()
    {
        return true;
    }

    // e.g. "./some-dir/" -> "./some-dir/index.md"
    if path.is_dir()
        && (path.join("index.md").is_file() || path.join("index.md").is_file())
    {
        return true;
    }

    false
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
            log::trace!(
                "Cached entry for \"{}\" is still fresh and was successful",
                url
            );
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
            cache.insert(url, CacheEntry::new(SystemTime::now(), false));
            Err(Reason::UnsuccessfulServerResponse(status))
        },
        Err(e) => {
            log::trace!("Request to \"{}\" failed: {}", url, e);
            cache.insert(url, CacheEntry::new(SystemTime::now(), false));
            Err(Reason::Client(e))
        },
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
    /// Links where validation failed.
    pub invalid_links: Vec<InvalidLink>,
    /// Links which have been ignored (e.g. due to
    /// [`Config::follow_web_links`]).
    pub ignored: Vec<Link>,
    /// Links which we don't know how to handle.
    pub unknown_schema: Vec<Link>,
    /// Potentially incomplete links.
    pub incomplete_links: Vec<IncompleteLink>,
}

impl ValidationOutcome {
    /// Generate a list of [`Diagnostic`] messages from this
    /// [`ValidationOutcome`].
    pub fn generate_diagnostics(
        &self,
        files: &Files,
        warning_policy: WarningPolicy,
    ) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        self.add_invalid_link_diagnostics(&mut diags);

        match warning_policy {
            WarningPolicy::Error => self.add_incomplete_link_diagnostics(
                Severity::Error,
                &mut diags,
                files,
            ),
            WarningPolicy::Warn => self.add_incomplete_link_diagnostics(
                Severity::Warning,
                &mut diags,
                files,
            ),
            WarningPolicy::Ignore => {},
        }

        diags
    }

    fn add_incomplete_link_diagnostics(
        &self,
        severity: Severity,
        diags: &mut Vec<Diagnostic>,
        files: &Files,
    ) {
        for incomplete in &self.incomplete_links {
            let IncompleteLink { ref text, file } = incomplete;
            let span = resolve_incomplete_link_span(incomplete, files);
            let msg =
                format!("Did you forget to define a URL for `{0}`?", text);
            let label = Label::new(*file, span, msg);
            let note = format!(
                "hint: declare the link's URL. For example: `[{}]: http://example.com/`",
                text
            );
            let diag =
                Diagnostic::new(severity, "Potential incomplete link", label)
                    .with_notes(vec![note]);
            diags.push(diag)
        }
    }

    fn add_invalid_link_diagnostics(&self, diags: &mut Vec<Diagnostic>) {
        for broken_link in &self.invalid_links {
            let link = &broken_link.link;
            let diag = Diagnostic::new_error(
                broken_link.to_string(),
                Label::new(
                    link.file,
                    link.span,
                    broken_link.reason.to_string(),
                ),
            );
            diags.push(diag);
        }
    }
}

/// HACK: this is a workaround for
/// [pulldown-cmark#165](https://github.com/raphlinus/pulldown-cmark/issues/165)
/// which uses good ol' string searching to find where an incomplete link may
/// have been defined.
fn resolve_incomplete_link_span(
    incomplete: &IncompleteLink,
    files: &Files,
) -> Span {
    let needle = format!("[{}]", incomplete.text);
    let src = files.source(incomplete.file);

    match src.find(&needle).map(|ix| ix as u32) {
        Some(start_ix) => Span::new(start_ix, start_ix + needle.len() as u32),
        None => files.source_span(incomplete.file),
    }
}

/// An invalid [`Link`] and the [`Reason`] for why it isn't valid.
#[derive(Debug)]
pub struct InvalidLink {
    /// The dodgy link.
    pub link: Link,
    /// Why the link isn't valid.
    pub reason: Reason,
}

impl Display for InvalidLink {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.reason {
            Reason::FileNotFound => write!(f, "File not found: {}", self.link.uri),
            Reason::TraversesParentDirectories => {
                write!(f, "\"{}\" links outside of the book directory, but this is forbidden", self.link.uri)
            },
            Reason::UnsuccessfulServerResponse(code) => {
                write!(f, "The server responded with {} for \"{}\"", code, self.link.uri)
            },
            Reason::Client(ref err) => write!(f, "Unable to retrieve \"{}\": {}", self.link.uri, err),
        }
    }
}

/// Why is this [`Link`] invalid?
#[derive(Debug)]
pub enum Reason {
    /// The link points to a file that doesn't exist.
    FileNotFound,
    /// The link points to a file outside of the book directory, and traversing
    /// outside the book directory is forbidden.
    TraversesParentDirectories,
    /// The server replied with an unsuccessful status code (according to
    /// [`StatusCode::is_success()`]).
    UnsuccessfulServerResponse(StatusCode),
    /// An error was encountered while checking a web link.
    Client(reqwest::Error),
}

impl Display for Reason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Reason::FileNotFound => "File not found".fmt(f),
            Reason::TraversesParentDirectories => {
                "Linking outside of the book directory is forbidden".fmt(f)
            },
            Reason::UnsuccessfulServerResponse(code) => {
                write!(f, "Server responded with {}", code)
            },
            Reason::Client(ref err) => err.fmt(f),
        }
    }
}

/// An unknown [`Uri::scheme_str()`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownScheme(pub Link);

#[cfg(test)]
mod tests {
    use super::*;
    use codespan::Files;

    #[test]
    fn sort_links_into_buckets() {
        let mut files = Files::new();
        let id = files.add("asd", "");
        let links = vec![Link::parse("path/to/file.md", 0..1, id).unwrap()];

        let got = sort_into_buckets(&links, |unknown| {
            panic!("Unknown schema: {:?}", unknown)
        });

        assert_eq!(got.file.len(), 1);
        assert_eq!(got.file[0], links[0]);
    }
}
