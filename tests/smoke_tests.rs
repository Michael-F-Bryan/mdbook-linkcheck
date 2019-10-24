use failure::Error;
use mdbook::{renderer::RenderContext, MDBook};
use mdbook_linkcheck::{Cache, Config, ValidationOutcome};
use std::path::{Path, PathBuf};

fn test_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests") }

#[test]
fn check_all_links_in_a_valid_book() {
    let root = test_dir().join("all-green");
    let output = run_link_checker(&root).unwrap();

    assert!(output.invalid_links.is_empty(), "{:?}", output);
}

fn run_link_checker(root: &Path) -> Result<ValidationOutcome, Error> {
    assert!(root.exists());

    let mut md = MDBook::load(root).unwrap();
    let cfg = Config {
        follow_web_links: true,
        traverse_parent_directories: false,
        exclude: vec![r"forbidden\.com".parse().unwrap()],
        ..Default::default()
    };
    md.config.set("output.linkcheck", &cfg).unwrap();

    let ctx = RenderContext::new(root, md.book, md.config, root.to_path_buf());

    let codemap = mdbook_linkcheck::book_to_codemap(&ctx.book);
    let links = mdbook_linkcheck::extract_links(&codemap);
    let src = ctx.source_dir();
    let cache = Cache::default();
    mdbook_linkcheck::validate(&links, &cfg, &src, &cache)
}
