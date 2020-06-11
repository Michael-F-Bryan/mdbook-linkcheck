use crate::{config::HttpHeader, IncompleteLink};
use anyhow::Error;
use codespan::{FileId, Files, Span};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use linkcheck::{
    validation::{InvalidLink, Reason},
    Link,
};
use serde::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, SerializeMap, Serializer},
};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

const ABSOLUTE_LINK_WARNING_REASONING: &'static str = r#"When viewing a document directly from the file system and click on an
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

/// The policy used when emitting errors or warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorHandling {
    rules: Vec<Rule>,
}

impl ErrorHandling {
    pub(crate) fn on_incomplete_link(
        &self,
        link: &IncompleteLink,
        span: Span,
    ) -> Option<Diagnostic<FileId>> {
        let IncompleteLink { ref text, file } = link;
        let severity = Severity::Error;

        let msg = format!("Did you forget to define a URL for `{0}`?", text);
        let label = Label::primary(*file, span).with_message(msg);
        let note = format!(
                "hint: declare the link's URL. For example: `[{}]: http://example.com/`",
                text
            );

        let diag = Diagnostic::new(severity)
            .with_message("Potential incomplete link")
            .with_labels(vec![label])
            .with_notes(vec![note]);

        Some(diag)
    }

    pub(crate) fn on_invalid_link(
        &self,
        broken: &InvalidLink,
    ) -> Option<Diagnostic<FileId>> {
        let msg = most_specific_error_message(broken);
        let link = &broken.link;

        let diag =
            Diagnostic::error()
                .with_message(msg.clone())
                .with_labels(vec![
                    Label::primary(link.file, link.span).with_message(msg)
                ]);

        Some(diag)
    }

    pub(crate) fn on_absolute_link(
        &self,
        link: &Link,
        insert_explanatory_text: bool,
        files: &Files<String>,
    ) -> Option<Diagnostic<FileId>> {
        let severity = Severity::Error;

        let mut notes = Vec::new();

        if insert_explanatory_text {
            notes.push(String::from(ABSOLUTE_LINK_WARNING_REASONING));
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

        Some(diag)
    }

    pub(crate) fn on_header_interpolation_error(
        &self,
        header: &HttpHeader,
        error: &Error,
    ) {
        let log_level = log::Level::Warn;

        log::log!(
            log_level,
            "Unable to interpolate \"{}\" because {}",
            header,
            error,
        );
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

impl Serialize for ErrorHandling {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.rules.is_empty() {
            return serializer.serialize_none();
        }

        let mut ser = serializer.serialize_map(Some(self.rules.len()))?;

        for rule in &self.rules {
            rule.serialize_to(&mut ser)?;
        }

        ser.end()
    }
}

impl<'de> Deserialize<'de> for ErrorHandling {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        unimplemented!()
    }
}

impl Default for ErrorHandling {
    fn default() -> Self { ErrorHandling { rules: Vec::new() } }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Rule {}

impl Rule {
    fn serialize_to<S>(&self, ser: &mut S) -> Result<(), S::Error>
    where
        S: SerializeMap,
    {
        unimplemented!()
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
