use crate::{Config, Context, IncompleteLink, WarningPolicy};
use anyhow::Error;
use codespan::{FileId, Files};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use linkcheck::{
    validation::{Cache, InvalidLink, Options, Outcomes, Reason},
    Link,
};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt::{self, Display, Formatter},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tokio::runtime::Builder;

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

    let interpolated_headers = cfg.interpolate_headers(cfg.warning_policy);

    let ctx = Context {
        client: cfg.client(),
        filesystem_options: options,
        cfg,
        src_dir,
        cache: Mutex::new(cache.clone()),
        files,
        interpolated_headers,
    };
    let links = collate_links(links, src_dir, files);

    let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
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
        let resolved_link = match resolved_link.strip_prefix(&src_dir) {
            Ok(path) => path,
            // Not part of the book.
            Err(_) => return Ok(()),
        };
        let was_included_in_summary =
            file_names.iter().any(|summary_path| {
                let summary_path = Path::new(summary_path);
                if summary_path.parent() != resolved_link.parent() {
                    return false;
                }
                match (summary_path.file_name(), resolved_link.file_name()) {
                    (a, b) if a == b => true,
                    (Some(summary), Some(resolved)) => {
                        // index preprocessor rewrites summary paths before we get to them.
                        summary == Path::new("index.md") && resolved == Path::new("README.md")
                    }
                    _ => false,
                }
            });
        let ext = resolved_link.extension();
        let is_markdown = ext == Some(OsStr::new("md"));

        if was_included_in_summary || !is_markdown {
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
    // Note: we want to sort all outcomes by file and then its location in that
    // file.
    //
    // That way, when we emit diagnostics they'll be emitted for each file in
    // the order that it is listed in `SUMMARY.md`, then individual diagnostics
    // will be emitted from the start of each file to the end.
    fn sorted<T, F>(mut items: Vec<T>, mut key: F) -> Vec<T>
    where
        F: FnMut(&T) -> &Link,
    {
        items.sort_by_key(|item| {
            let link = key(item);
            (link.file, link.span)
        });
        items
    }
    fn sorted_link(items: Vec<Link>) -> Vec<Link> { sorted(items, |link| link) }

    ValidationOutcome {
        invalid_links: sorted(outcomes.invalid, |l| &l.link),
        ignored: sorted_link(outcomes.ignored),
        valid_links: sorted_link(outcomes.valid),
        unknown_category: sorted_link(outcomes.unknown_category),
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
        self.add_incomplete_link_diagnostics(warning_policy, &mut diags);
        self.warn_on_absolute_links(warning_policy, &mut diags, files);

        diags
    }

    fn add_incomplete_link_diagnostics(
        &self,
        warning_policy: WarningPolicy,
        diags: &mut Vec<Diagnostic<FileId>>,
    ) {
        let severity = match warning_policy {
            WarningPolicy::Error => Severity::Error,
            WarningPolicy::Warn => Severity::Warning,
            WarningPolicy::Ignore => return,
        };

        for incomplete in &self.incomplete_links {
            let IncompleteLink {
                ref reference,
                file,
                span,
            } = incomplete;

            let msg =
                format!("Did you forget to define a URL for `{0}`?", reference);
            let label = Label::primary(*file, *span).with_message(msg);
            let note = format!(
                "hint: declare the link's URL. For example: `[{}]: http://example.com/`",
                reference
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

    /// As shown in https://github.com/Michael-F-Bryan/mdbook-linkcheck/issues/33
    /// absolute links are actually a bit of a foot gun when the document is
    /// being read directly from the filesystem.
    fn warn_on_absolute_links(
        &self,
        warning_policy: WarningPolicy,
        diags: &mut Vec<Diagnostic<FileId>>,
        files: &Files<String>,
    ) {
        const WARNING_MESSAGE: &'static str = r#"When viewing a document directly from the file system and click on an
absolute link (e.g. `/index.md`), the browser will try to navigate to
`/index.md` on the current file system (i.e. the `index.md` file inside
`/` or `C:\`) instead of the `index.md` file at book's base directory as
intended.

This warning helps avoid the situation where everything will seem to work
fine when viewed using a web server (e.g. GitHub Pages or `mdbook serve`),
but users viewing the book from the file system may encounter broken links.

To ignore this warning, you can edit `book.toml` and set the warning policy to
"ignore".

    [output.linkcheck]
    warning-policy = "ignore"

For more details, see https://github.com/Michael-F-Bryan/mdbook-linkcheck/issues/33
"#;
        let severity = match warning_policy {
            WarningPolicy::Error => Severity::Error,
            WarningPolicy::Warn => Severity::Warning,
            WarningPolicy::Ignore => return,
        };

        let absolute_links = self
            .valid_links
            .iter()
            .filter(|link| link.href.starts_with("/"));

        let mut reasoning_emitted = false;

        for link in absolute_links {
            let mut notes = Vec::new();

            if !reasoning_emitted {
                notes.push(String::from(WARNING_MESSAGE));
                reasoning_emitted = true;
            }

            if let Some(suggested_change) =
                relative_path_to_file(files.name(link.file), &link.href)
            {
                notes.push(format!(
                    "Suggestion: change the link to \"{}\"",
                    suggested_change
                ));
            }

            let diag = Diagnostic::new(severity)
                .with_message("Absolute link should be made relative")
                .with_notes(notes)
                .with_labels(vec![Label::primary(link.file, link.span)
                    .with_message("Absolute link should be made relative")]);

            diags.push(diag);
        }
    }
}

// Path diffing, copied from https://crates.io/crates/pathdiff with some tweaks
fn relative_path_to_file<S, D>(start: S, destination: D) -> Option<String>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    let destination = destination.as_ref();
    let start = start.as_ref();
    log::debug!(
        "Trying to find the relative path from \"{}\" to \"{}\"",
        start.display(),
        destination.display()
    );

    let start = start.parent()?;
    let destination_name = destination.file_name()?;
    let destination = destination.parent()?;

    let mut ita = destination.components().skip(1);
    let mut itb = start.components();

    let mut comps: Vec<Component> = vec![];

    loop {
        match (ita.next(), itb.next()) {
            (None, None) => break,
            (Some(a), None) => {
                comps.push(a);
                comps.extend(ita.by_ref());
                break;
            },
            (None, _) => comps.push(Component::ParentDir),
            (Some(a), Some(b)) if comps.is_empty() && a == b => (),
            (Some(a), Some(b)) if b == Component::CurDir => comps.push(a),
            (Some(_), Some(b)) if b == Component::ParentDir => return None,
            (Some(a), Some(_)) => {
                comps.push(Component::ParentDir);
                for _ in itb {
                    comps.push(Component::ParentDir);
                }
                comps.push(a);
                comps.extend(ita.by_ref());
                break;
            },
        }
    }

    let path: PathBuf = comps
        .iter()
        .map(|c| c.as_os_str())
        .chain(std::iter::once(destination_name))
        .collect();

    // Note: URLs always use forward slashes
    Some(path.display().to_string().replace('\\', "/"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_some_simple_relative_paths() {
        let inputs = vec![
            ("index.md", "/other.md", "other.md"),
            ("index.md", "/nested/other.md", "nested/other.md"),
            ("nested/index.md", "/other.md", "../other.md"),
        ];

        for (start, destination, should_be) in inputs {
            let got = relative_path_to_file(start, destination).unwrap();
            assert_eq!(got, should_be);
        }
    }
}
