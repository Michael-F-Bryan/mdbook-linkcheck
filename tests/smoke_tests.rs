use codespan::Files;
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

#[test]
fn correctly_find_broken_links() {
    let root = test_dir().join("broken-links");
    let expected = vec!["asd"];

    let output = run_link_checker(&root).unwrap();

    let broken: Vec<_> = output
        .invalid_links
        .iter()
        .map(|invalid| invalid.link.uri.to_string())
        .collect();
    assert_eq!(broken, expected);
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

    let mut files = Files::new();
    let file_ids =
        mdbook_linkcheck::load_files_into_memory(&ctx.book, &mut files);
    let links = mdbook_linkcheck::extract_links(file_ids, &files);
    let src = ctx.source_dir();
    let cache = Cache::default();
    mdbook_linkcheck::validate(&links, &cfg, &src, &cache, &files)
}
