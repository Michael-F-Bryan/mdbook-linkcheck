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
    if !cfg.follow_web_links || cfg.should_skip(url.as_str()) {
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

    if !cfg.traverse_parent_directories
        && path_is_outside_book(path, &absolute_chapter_path, &ctx.source_dir())
    {
        return Err(Box::new(ForbiddenPath::new(
            path,
            &link.chapter.path,
            link.line_number(),
        )));
    }

    let chapter_dir = absolute_chapter_path.parent().unwrap();
    let target = if path.is_absolute() {
        ctx.source_dir().join(path.strip_prefix("/").unwrap())
    } else {
        chapter_dir.join(path)
    };

    debug!(
        "Searching for \"{}\" from {}#{}",
        target.display(),
        link.chapter.path.display(),
        link.line_number()
    );

    let html_equivalent_exists =
        target.extension() == Some(OsStr::new("html")) && target.with_extension("md").exists();

    if target.exists() || html_equivalent_exists {
        Ok(())
    } else {
        Err(Box::new(FileNotFound::new(
            path,
            &link.chapter.path,
            link.line_number(),
        )))
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
