use crate::{cache::Cache, Config, Context, IncompleteLink, WarningPolicy};
use codespan::{FileId, Files, Span};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use failure::Error;
use linkcheck::{validation::InvalidLink, Link};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::runtime::Runtime;

#[allow(unused_imports)]
use http::Uri;

fn lc_validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    cache: &Cache,
    files: &Files<String>,
) -> linkcheck::validation::Outcomes {
    let cache = Mutex::new(linkcheck::validation::Cache::from(cache));
    let ctx = Context {
        client: cfg.client(),
        filesystem_options: cfg.options(),
        cfg,
        src_dir,
        cache,
        files,
    };
    let links = collate_links(links, src_dir, files);

    let mut runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let mut outcomes = linkcheck::validation::Outcomes::default();

        for (current_dir, links) in links {
            outcomes
                .merge(linkcheck::validate(&current_dir, links, &ctx).await);
        }

        outcomes
    })
}

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
    outcomes: linkcheck::validation::Outcomes,
    incomplete_links: Vec<IncompleteLink>,
) -> ValidationOutcome {
    unimplemented!()
}

/// Try to validate the provided [`Link`]s.
pub fn validate(
    links: &[Link],
    cfg: &Config,
    src_dir: &Path,
    cache: &Cache,
    files: &Files<String>,
    incomplete_links: Vec<IncompleteLink>,
) -> Result<ValidationOutcome, Error> {
    let got = lc_validate(links, cfg, src_dir, cache, files);
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
    pub unknown_schema: Vec<Link>,
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
        diags: &mut Vec<Diagnostic<FileId>>,
        files: &Files<String>,
    ) {
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
            let diag = Diagnostic::error()
                .with_message(broken_link.reason.to_string())
                .with_labels(vec![Label::primary(link.file, link.span)
                    .with_message(broken_link.reason.to_string())]);
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
    files: &Files<String>,
) -> Span {
    let needle = format!("[{}]", incomplete.text);
    let src = files.source(incomplete.file);

    match src.find(&needle).map(|ix| ix as u32) {
        Some(start_ix) => Span::new(start_ix, start_ix + needle.len() as u32),
        None => files.source_span(incomplete.file),
    }
}
