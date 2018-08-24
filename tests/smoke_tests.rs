extern crate failure;
extern crate mdbook;
extern crate mdbook_linkcheck;

use failure::Error;
use mdbook::renderer::RenderContext;
use mdbook::MDBook;
use mdbook_linkcheck::Config;
use std::path::Path;
use std::process::Command;

const ALL_GREEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/all-green");

#[test]
fn check_all_links_in_a_valid_book() {
    let root = Path::new(ALL_GREEN);
    run_link_checker(root).unwrap();
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
