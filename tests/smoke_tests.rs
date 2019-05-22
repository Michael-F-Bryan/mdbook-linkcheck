extern crate failure;
extern crate mdbook;
extern crate mdbook_linkcheck;

use failure::Error;
use mdbook::renderer::RenderContext;
use mdbook::MDBook;
use mdbook_linkcheck::errors::*;
use mdbook_linkcheck::Config;
use std::path::{Path, PathBuf};

fn test_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn check_all_links_in_a_valid_book() {
    let root = test_dir().join("all-green");
    run_link_checker(&root).unwrap();
}

#[test]
fn book_with_broken_links() {
    let root = test_dir().join("broken-links");
    let result = run_link_checker(&root).unwrap_err();

    let broken_links = result.downcast_ref::<BrokenLinks>().unwrap();
    let links = broken_links.links();

    assert_eq!(links.len(), 5);

    let non_existent_url = links[0].as_fail().downcast_ref::<HttpError>().unwrap();
    assert_eq!(
        non_existent_url.url.as_str(),
        "http://this-doesnt-exist.com.au.nz.us/"
    );

    let missing_chapter = links[1].as_fail().downcast_ref::<FileNotFound>().unwrap();
    assert_eq!(missing_chapter.path, Path::new("./foo/bar/baz.html"));

    // Allowing something like this (by default) might be a bit of a security
    // issue...
    let etc_shadow = links[2].as_fail().downcast_ref::<ForbiddenPath>().unwrap();
    assert_eq!(
        etc_shadow.path,
        Path::new("../../../../../../../../../../../../etc/shadow")
    );

    // Nested links which are relative to the book root instead of the current
    // file are errors
    let deeply_nested_relative = links[3].as_fail().downcast_ref::<FileNotFound>().unwrap();
    assert_eq!(deeply_nested_relative.path, Path::new("./chapter_1.md"));
    let other_nested = links[4].as_fail().downcast_ref::<FileNotFound>().unwrap();
    assert_eq!(other_nested.path, Path::new("./second/directory.md"));
}

fn run_link_checker(root: &Path) -> Result<(), Error> {
    assert!(root.exists());

    let md = MDBook::load(root).unwrap();
    let mut cfg = md.config;
    cfg.set(
        "output.linkcheck",
        Config {
            follow_web_links: true,
            traverse_parent_directories: false,
            exclude: vec![r"forbidden\.com".parse().unwrap()],
        },
    )
    .unwrap();

    let render_ctx = RenderContext::new(root, md.book, cfg, root.to_path_buf());

    mdbook_linkcheck::check_links(&render_ctx)
}
