use mdbook::renderer::RenderContext;
use reqwest;
use std::ffi::OsStr;
use std::path::Path;
use url::Url;

use errors::{BrokenLink, EmptyLink, FileNotFound, ForbiddenPath, HttpError, UnsuccessfulStatus};
use {Config, Link};

pub fn check_link(link: &Link, ctx: &RenderContext, cfg: &Config) -> Result<(), Box<BrokenLink>> {
    trace!("Checking {}", link);

    if link.url.is_empty() {
        let err = EmptyLink::new(&link.chapter.path, link.line_number());
        return Err(Box::new(err));
    }

    match Url::parse(&link.url) {
        Ok(link_url) => validate_external_link(link, &link_url, cfg),
        Err(_) => check_link_in_book(link, ctx, cfg),
    }
}

fn validate_external_link(link: &Link, url: &Url, cfg: &Config) -> Result<(), Box<BrokenLink>> {
    if !cfg.follow_web_links {
        debug!("Ignoring \"{}\"", url);
        return Ok(());
    }

    debug!("Fetching \"{}\"", url);

    let response = reqwest::get(url.clone()).map_err(|e| {
        Box::new(HttpError::new(
            url.clone(),
            link.chapter.path.clone(),
            link.line_number(),
            e,
        )) as Box<BrokenLink>
    })?;
    let status = response.status();

    if status.is_success() {
        Ok(())
    } else {
        trace!("Unsuccessful Status {} for {}", status, url);
        Err(Box::new(UnsuccessfulStatus::new(
            url.clone(),
            status,
            link.chapter.path.clone(),
            link.line_number(),
        )))
    }
}

fn check_link_in_book(
    link: &Link,
    ctx: &RenderContext,
    cfg: &Config,
) -> Result<(), Box<BrokenLink>> {
    if link.url.starts_with('#') {
        // this just jumps to another spot on the page
        return Ok(());
    }

    let absolute_chapter_path = ctx.source_dir().join(&link.chapter.path);

    let path = match link.url.find('#') {
        Some(ix) => Path::new(&link.url[..ix]),
        None => Path::new(&link.url),
    };

    if !cfg.can_traverse_parent_directories
        && path_is_outside_book(path, &absolute_chapter_path, &ctx.source_dir())
    {
        return Err(Box::new(ForbiddenPath::new(
            path,
            &link.chapter.path,
            link.line_number(),
        )));
    }

    if path.extension() == Some(OsStr::new("html")) {
        check_link_to_chapter(link, ctx)
    } else {
        check_asset_link_is_valid(link, ctx)
    }
}

fn path_is_outside_book(path: &Path, chapter: &Path, src: &Path) -> bool {
    let chapter_dir = match chapter.parent() {
        Some(p) => p,
        None => return false,
    };

    let joined = match chapter_dir.join(path).canonicalize() {
        Ok(j) => j,
        Err(_) => return false,
    };

    !joined.starts_with(&src)
}

fn check_link_to_chapter(link: &Link, ctx: &RenderContext) -> Result<(), Box<BrokenLink>> {
    let mut path = match link.url.find('#') {
        Some(ix) => &link.url[..ix],
        None => &link.url,
    };

    // note: all chapter links are relative to the `src/` directory regardless
    // of whether they start with "/" or not.
    if path.starts_with("/") {
        path = &path[1..];
    }

    let src = ctx.root.join(&ctx.config.book.src);

    let chapter_path = src.join(path).with_extension("md");
    debug!("Searching for {}", chapter_path.display());

    if chapter_path.exists() {
        Ok(())
    } else {
        Err(Box::new(FileNotFound::new(
            path,
            &link.chapter.path,
            link.line_number(),
        )))
    }
}

/// Check the link is to a valid asset inside the book's `src/` directory. The
/// HTML renderer will copy this to the destination directory accordingly.
fn check_asset_link_is_valid(link: &Link, ctx: &RenderContext) -> Result<(), Box<BrokenLink>> {
    let path = Path::new(&link.url);
    let src = ctx.root.join(&ctx.config.book.src);

    debug_assert!(
        src.is_absolute(),
        "mdbook didn't give us the book root's absolute path"
    );

    let full_path = if path.is_absolute() {
        src.join(&path)
    } else {
        let directory = match link.chapter.path.parent() {
            Some(parent) => src.join(parent),
            None => src.clone(),
        };

        directory.join(&path)
    };

    // by this point we've resolved the link relative to the source chapter's
    // directory, and turned it into an absolute path. This *should* match a
    // file on disk.
    debug!("Searching for {}", full_path.display());

    match full_path.canonicalize() {
        Err(_) => Err(Box::new(FileNotFound::new(
            path,
            &link.chapter.path,
            link.line_number(),
        ))),
        Ok(p) => if p.exists() {
            Ok(())
        } else {
            Err(Box::new(FileNotFound::new(
                p,
                &link.chapter.path,
                link.line_number(),
            )))
        },
    }
}
