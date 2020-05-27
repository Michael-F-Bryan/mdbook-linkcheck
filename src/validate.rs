use crate::{Config, Context, IncompleteLink, WarningPolicy};
use anyhow::Error;
use codespan::{FileId, Files, Span};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use linkcheck::{
    validation::{Cache, InvalidLink, Options, Outcomes, Reason},
    Link,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::{self, Display, Formatter},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::runtime::Runtime;

fn lc_validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    cache: &mut Cache,
    files: &Files<String>,
    file_ids: &[FileId],
) -> Outcomes {
    let file_names = file_ids
        .iter()
        .map(|id| files.name(*id).to_os_string())
        .collect();

    let options = Options::default()
        .with_root_directory(src_dir)
        .expect("The source directory doesn't exist?")
        .set_alternate_extensions(vec![(
            "html".to_string(),
            vec!["md".to_string()],
        )])
        .set_links_may_traverse_the_root_directory(
            cfg.traverse_parent_directories,
        )
        // take into account the `index` preprocessor which rewrites `README.md`
        // to `index.md` (which tne gets rendered as `index.html`)
        .set_default_file("README.md")
        .set_custom_validation(ensure_included_in_book(src_dir, file_names));

    let ctx = Context {
        client: cfg.client(),
        filesystem_options: options,
        cfg,
        src_dir,
        cache: Mutex::new(cache.clone()),
        files,
    };
    let links = collate_links(links, src_dir, files);

    let mut runtime = Runtime::new().unwrap();
    let got = runtime.block_on(async {
        let mut outcomes = Outcomes::default();

        for (current_dir, links) in links {
            outcomes
                .merge(linkcheck::validate(&current_dir, links, &ctx).await);
        }

        outcomes
    });

    // move the cache out of ctx. We'd get a borrowing error if anything was
    // using it
    let updated_cache = ctx.cache;

    *cache = updated_cache
        .into_inner()
        .expect("We statically know this isn't used");
    got
}

fn ensure_included_in_book(
    src_dir: &Path,
    file_names: Vec<OsString>,
) -> impl Fn(&Path, Option<&str>) -> Result<(), Reason> {
    let src_dir = src_dir.to_path_buf();

    move |resolved_link, _| {
        let part_of_the_book = resolved_link.starts_with(&src_dir);
        let was_included_in_summary =
            file_names.iter().any(|name| resolved_link.ends_with(name));

        if !part_of_the_book || was_included_in_summary {
            Ok(())
        } else {
            use std::io::{Error, ErrorKind};

            Err(Reason::Io(Error::new(
                ErrorKind::Other,
                NotInSummary {
                    path: resolved_link.to_path_buf(),
                },
            )))
        }
    }
}

/// An error that is emitted if something links to a file that exists on disk,
/// but isn't included in the book.
#[derive(Debug)]
pub struct NotInSummary {
    /// The file's full path.
    pub path: PathBuf,
}

impl Display for NotInSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "It looks like \"{}\" wasn't included in SUMMARY.md",
            self.path.display()
        )
    }
}

impl std::error::Error for NotInSummary {}

fn collate_links<'a>(
    links: &'a [Link],
    src_dir: &Path,
    files: &'a Files<String>,
) -> impl Iterator<Item = (PathBuf, Vec<linkcheck::Link>)> {
    let mut links_by_directory: HashMap<PathBuf, Vec<linkcheck::Link>> =
        HashMap::new();

    for link in links {
        let mut path = src_dir.join(files.name(link.file));
        path.pop();
        links_by_directory
            .entry(path)
            .or_default()
            .push(link.clone());
    }

    links_by_directory.into_iter()
}

fn merge_outcomes(
    outcomes: Outcomes,
    incomplete_links: Vec<IncompleteLink>,
) -> ValidationOutcome {
    ValidationOutcome {
        invalid_links: outcomes.invalid,
        ignored: outcomes.ignored,
        valid_links: outcomes.valid,
        unknown_category: outcomes.unknown_category,
        incomplete_links,
    }
}

/// Try to validate the provided [`Link`]s.
pub fn validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    cache: &mut Cache,
    files: &Files<String>,
    file_ids: &[FileId],
    incomplete_links: Vec<IncompleteLink>,
) -> Result<ValidationOutcome, Error> {
    let got = lc_validate(links, cfg, src_dir, cache, files, file_ids);
    Ok(merge_outcomes(got, incomplete_links))
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
    pub unknown_category: Vec<Link>,
    /// Potentially incomplete links.
    pub incomplete_links: Vec<IncompleteLink>,
}

impl ValidationOutcome {
    /// Generate a list of [`Diagnostic`] messages from this
    /// [`ValidationOutcome`].
    pub fn generate_diagnostics(
        &self,
        files: &Files<String>,
        warning_policy: WarningPolicy,
    ) -> Vec<Diagnostic<FileId>> {
        let mut diags = Vec::new();

        self.add_invalid_link_diagnostics(&mut diags);
        self.add_incomplete_link_diagnostics(warning_policy, &mut diags, files);

        diags
    }

    fn add_incomplete_link_diagnostics(
        &self,
        warning_policy: WarningPolicy,
        diags: &mut Vec<Diagnostic<FileId>>,
        files: &Files<String>,
    ) {
        let severity = match warning_policy {
            WarningPolicy::Error => Severity::Error,
            WarningPolicy::Warn => Severity::Warning,
            WarningPolicy::Ignore => return,
        };

        for incomplete in &self.incomplete_links {
            let IncompleteLink { ref text, file } = incomplete;

            let span = resolve_incomplete_link_span(incomplete, files);
            let msg =
                format!("Did you forget to define a URL for `{0}`?", text);
            let label = Label::primary(*file, span).with_message(msg);
            let note = format!(
                "hint: declare the link's URL. For example: `[{}]: http://example.com/`",
                text
            );

            let diag = Diagnostic::new(severity)
                .with_message("Potential incomplete link")
                .with_labels(vec![label])
                .with_notes(vec![note]);
            diags.push(diag)
        }
    }

    fn add_invalid_link_diagnostics(
        &self,
        diags: &mut Vec<Diagnostic<FileId>>,
    ) {
        for broken_link in &self.invalid_links {
            let link = &broken_link.link;
            let msg = most_specific_error_message(&broken_link);
            let diag = Diagnostic::error()
                .with_message(msg.clone())
                .with_labels(vec![
                    Label::primary(link.file, link.span).with_message(msg)
                ]);
            diags.push(diag);
        }
    }
}

fn most_specific_error_message(link: &InvalidLink) -> String {
    if link.reason.file_not_found() {
        return format!("File not found: {}", link.link.href);
    }

    match link.reason {
        Reason::Io(ref io) => io.to_string(),
        Reason::Web(ref web) if web.is_status() => {
            let status = web.status().expect(
                "Response::error_for_status() always contains a status code",
            );
            let url = web
                .url()
                .expect("Response::error_for_status() always contains a URL");

            match status.canonical_reason() {
                Some(reason) => format!(
                    "Server returned {} {} for {}",
                    status.as_u16(),
                    reason,
                    url
                ),
                None => {
                    format!("Server returned {} for {}", status.as_u16(), url)
                },
            }
        },
        Reason::Web(ref web) => web.to_string(),
        // fall back to the Reason's Display impl
        _ => link.reason.to_string(),
    }
}

/// HACK: this is a workaround for
/// [pulldown-cmark#165](https://github.com/raphlinus/pulldown-cmark/issues/165)
/// which uses good ol' string searching to find where an incomplete link may
/// have been defined.
fn resolve_incomplete_link_span(
    incomplete: &IncompleteLink,
    files: &Files<String>,
) -> Span {
    let needle = format!("[{}]", incomplete.text);
    let src = files.source(incomplete.file);

    match src.find(&needle).map(|ix| ix as u32) {
        Some(start_ix) => Span::new(start_ix, start_ix + needle.len() as u32),
        None => files.source_span(incomplete.file),
    }
}
