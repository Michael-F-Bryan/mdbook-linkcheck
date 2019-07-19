use failure::Error;
use mdbook::{renderer::RenderContext, MDBook};
use mdbook_linkcheck::Config;
use std::path::{Path, PathBuf};

fn test_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests") }

#[test]
#[ignore]
fn check_all_links_in_a_valid_book() {
    let root = test_dir().join("all-green");
    run_link_checker(&root).unwrap();
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

    let _render_ctx =
        RenderContext::new(root, md.book, cfg, root.to_path_buf());

    unimplemented!()
}
