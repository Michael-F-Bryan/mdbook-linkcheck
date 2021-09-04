#[macro_use]
extern crate pretty_assertions;

use anyhow::Error;
use codespan::{FileId, Files};
use linkcheck::validation::{Cache, Reason};
use mdbook::{renderer::{RenderContext, Renderer}, MDBook};
use mdbook_linkcheck::{Config, HashedRegex, ValidationOutcome, WarningPolicy};
use std::{cell::Cell, collections::HashMap, convert::TryInto, iter::FromIterator, path::{Path, PathBuf}};

fn test_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests") }

#[test]
fn check_all_links_in_a_valid_book() {
    let root = test_dir().join("all-green");
    let expected_valid = &[
        "../chapter_1.md",
        "../chapter_1.md#Subheading",
        "./chapter_1.html",
        "./chapter_1.md",
        "./sibling.md",
        "/chapter_1.md",
        "/chapter_1.md#Subheading",
        "https://crates.io/crates/mdbook-linkcheck",
        "https://www.google.com/",
        "nested/",
        "nested/README.md",
        "sibling.md",
    ];

    let output = run_link_checker(&root).unwrap();

    let valid_links: Vec<_> = output
        .valid_links
        .iter()
        .map(|link| link.href.to_string())
        .collect();
    assert_same_links(expected_valid, valid_links);
    assert!(
        output.invalid_links.is_empty(),
        "Found invalid links: {:?}",
        output.invalid_links
    );
}

#[test]
fn correctly_find_broken_links() {
    let root = test_dir().join("broken-links");
    let expected = &[
        "./foo/bar/baz.html",
        "../../../../../../../../../../../../etc/shadow",
        "./asdf.png",
        "./chapter_1.md",
        "./second/directory.md",
        "http://this-doesnt-exist.com.au.nz.us/",
        "sibling.md",
    ];

    let output = run_link_checker(&root).unwrap();

    let broken: Vec<_> = output
        .invalid_links
        .iter()
        .map(|invalid| invalid.link.href.to_string())
        .collect();
    assert_same_links(broken, expected);
    // we also have one incomplete link
    assert_eq!(output.incomplete_links.len(), 1);
    assert_eq!(output.incomplete_links[0].reference, "incomplete link");
}

#[test]
fn detect_when_a_linked_file_isnt_in_summary_md() {
    let root = test_dir().join("broken-links");

    let output = run_link_checker(&root).unwrap();

    let broken_link = output
        .invalid_links
        .iter()
        .find(|invalid| invalid.link.href == "sibling.md")
        .unwrap();

    assert!(is_specific_error::<mdbook_linkcheck::NotInSummary>(
        &broken_link.reason
    ));
}

#[test]
fn emit_valid_suggestions_on_absolute_links() {
    let root = test_dir().join("absolute-links");

    TestRun::new(root)
        .after_validation(|files, outcome, _| {
            let diags =
                outcome.generate_diagnostics(files, WarningPolicy::Error);

            let suggestions = vec![
                "\"chapter_1.md\"",
                "\"nested/README.md\"",
                "\"../chapter_1.md\"",
            ];
            assert_eq!(suggestions.len(), diags.len());

            for (diag, suggestion) in
                diags.into_iter().zip(suggestions.into_iter())
            {
                assert!(
                    diag.notes.iter().any(|note| note.contains(suggestion)),
                    "It should have suggested {} for {:?}",
                    suggestion,
                    diag
                );
            }
        })
        .execute()
        .unwrap();
}

#[test]
fn skip_web_links() {
    let root = test_dir().join("external-links");
    let expected_valid = &[
        "../chapter_1.md",
        "../chapter_1.md#Subheading",
        "./chapter_1.html",
        "./chapter_1.md",
        "./sibling.md",
        "/chapter_1.md",
        "/chapter_1.md#Subheading",
        "nested/",
        "nested/README.md",
        "sibling.md",
    ];

    let expected_ignored = &[
        "https://crates.io/crates/mdbook-linkcheck",
        "https://www.google.com/",
        "https://nonexistent.forbidden.com/",
    ];

    let config = Config {
        follow_web_links: false,
        ..Default::default()
    };

    let output = run_link_checker_with_config(&root, config).unwrap();

    let valid_links: Vec<_> = output
        .valid_links
        .iter()
        .map(|link| link.href.to_string())
        .collect();
    assert_same_links(expected_valid, valid_links);

    let ignored_links: Vec<_> = output
        .ignored
        .iter()
        .map(|link| link.href.to_string())
        .collect();
    assert_same_links(expected_ignored, ignored_links);

    assert!(
        output.invalid_links.is_empty(),
        "Found invalid links: {:?}",
        output.invalid_links
    );
}

fn is_specific_error<E>(reason: &Reason) -> bool
where
    E: std::error::Error + 'static,
{
    if let Reason::Io(io) = reason {
        if let Some(inner) = io.get_ref() {
            return inner.is::<E>();
        }
    }

    false
}

#[track_caller]
fn assert_same_links<L, R, P, Q>(left: L, right: R)
where
    L: IntoIterator<Item = P>,
    P: AsRef<str>,
    R: IntoIterator<Item = Q>,
    Q: AsRef<str>,
{
    let mut left: Vec<_> =
        left.into_iter().map(|s| s.as_ref().to_string()).collect();
    left.sort();
    let mut right: Vec<_> =
        right.into_iter().map(|s| s.as_ref().to_string()).collect();
    right.sort();

    assert_eq!(left, right);
}

struct TestRun {
    config: Config,
    root: PathBuf,
    after_validation:
        Box<dyn Fn(&Files<String>, &ValidationOutcome, &Vec<FileId>)>,
    validation_outcome: Cell<Option<ValidationOutcome>>,
}

impl TestRun {
    fn new<P: Into<PathBuf>>(root: P) -> Self {
        TestRun {
            root: root.into(),
            config: Config {
                follow_web_links: true,
                traverse_parent_directories: false,
                exclude: vec![r"forbidden\.com".parse().unwrap()],
                http_headers: HashMap::from_iter(vec![(
                    HashedRegex::new(r"crates\.io").unwrap(),
                    vec!["Accept: text/html".try_into().unwrap()],
                )]),
                ..Default::default()
            },
            after_validation: Box::new(|_, _, _| {}),
            validation_outcome: Cell::new(None),
        }
    }

    fn new_with_config<P: Into<PathBuf>>(root: P, config: Config) -> Self {
        TestRun {
            root: root.into(),
            config,
            after_validation: Box::new(|_, _, _| {}),
            validation_outcome: Cell::new(None),
        }
    }

    fn after_validation<F>(self, cb: F) -> Self
    where
        F: Fn(&Files<String>, &ValidationOutcome, &Vec<FileId>) + 'static,
    {
        let after_validation = Box::new(cb);
        TestRun {
            after_validation,
            ..self
        }
    }

    fn initialise_logging(&self) {
        let _ = env_logger::builder()
            .filter(Some("linkcheck"), log::LevelFilter::Debug)
            .filter(Some("mdbook_linkcheck"), log::LevelFilter::Debug)
            .is_test(true)
            .try_init();
    }

    fn execute(self) -> Result<ValidationOutcome, Error> {
        self.initialise_logging();
        assert!(self.root.exists());

        let mut md = MDBook::load(&self.root).unwrap();
        md.config.set("output.linkcheck", &self.config).unwrap();

        md.execute_build_process(&self)?;

        Ok(self.validation_outcome.take().unwrap())
    }
}

impl Renderer for TestRun {
    fn name(&self) -> &str {
        "mdbook-linkcheck-TestRun"
    }

    fn render(&self, ctx: &RenderContext) -> anyhow::Result<()> {
        let mut files = Files::new();
        let src = dunce::canonicalize(ctx.source_dir()).unwrap();

        let noop_filter = |_: &Path| true;

        let file_ids = mdbook_linkcheck::load_files_into_memory(
            &ctx.book,
            &mut files,
            noop_filter,
        );
        let (links, incomplete) =
            mdbook_linkcheck::extract_links(file_ids.clone(), &files);

        let mut cache = Cache::default();
        let outcome = mdbook_linkcheck::validate(
            &links,
            &self.config,
            &src,
            &mut cache,
            &files,
            &file_ids,
            incomplete,
        )?;

        (self.after_validation)(&files, &outcome, &file_ids);

        self.validation_outcome.set(Some(outcome));
        Ok(())
    }
}

fn run_link_checker(root: &Path) -> Result<ValidationOutcome, Error> {
    TestRun::new(root).execute()
}

fn run_link_checker_with_config(
    root: &Path,
    config: Config,
) -> Result<ValidationOutcome, Error> {
    TestRun::new_with_config(root, config).execute()
}
