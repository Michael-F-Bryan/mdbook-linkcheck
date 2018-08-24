extern crate failure;
extern crate mdbook;
extern crate mdbook_linkcheck;

use failure::Error;
use mdbook::renderer::RenderContext;
use mdbook::MDBook;
use mdbook_linkcheck::errors::*;
use mdbook_linkcheck::Config;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    assert_eq!(links.len(), 3);

    let non_existent_url = links[0].as_fail().downcast_ref::<HttpError>().unwrap();
    assert_eq!(
        non_existent_url.url.as_str(),
        "http://this-doesnt-exist.com.au.nz.us/"
    );

    let missing_chapter = links[1].as_fail().downcast_ref::<FileNotFound>().unwrap();
    assert_eq!(missing_chapter.path, Path::new("./foo/bar/baz.html"));

    let etc_shadow = links[2].as_fail().downcast_ref::<ForbiddenPath>().unwrap();
    assert_eq!(
        etc_shadow.path,
        Path::new("../../../../../../../../../../../../etc/shadow")
    );
}

fn mdbook_version() -> String {
    let default_version = mdbook_linkcheck::COMPATIBLE_MDBOOK_VERSIONS
        .replace("^", "")
        .to_string();

    let output = match Command::new("mdbook").arg("--version").output() {
        Ok(o) => o,
        Err(_) => return default_version,
    };

    let stdout = match String::from_utf8(output.stdout) {
        Ok(v) => v,
        Err(_) => return default_version,
    };

    extract_version_string(&stdout).unwrap_or(default_version)
}

fn extract_version_string(version: &str) -> Option<String> {
    version.split_whitespace().nth(2).map(ToOwned::to_owned)
}

fn run_link_checker(root: &Path) -> Result<(), Error> {
    assert!(root.exists());

    let md = MDBook::load(root).unwrap();
    let mut cfg = md.config;
    cfg.set(
        "output.linkcheck",
        Config {
            follow_web_links: true,
            can_traverse_parent_directories: false,
        },
    ).unwrap();

    let render_ctx = RenderContext {
        version: mdbook_version(),
        book: md.book,
        config: cfg,
        destination: root.join("book"),
        root: root.to_path_buf(),
    };

    mdbook_linkcheck::check_links(&render_ctx)
}
