use codespan::Files;
use codespan_reporting::{
    diagnostic::{Diagnostic, Severity},
    term::termcolor::{ColorChoice, StandardStream},
};
use failure::{Error, ResultExt, SyncFailure};
use mdbook::{renderer::RenderContext, MDBook};
use mdbook_linkcheck::{Cache, ValidationOutcome};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

fn main() -> Result<(), Error> {
    env_logger::init();
    let args = Args::from_args();

    // get a `RenderContext`, either from stdin (because we're used as a plugin)
    // or by instrumenting MDBook directly (in standalone mode).
    let ctx: RenderContext = if args.standalone {
        let md = MDBook::load(args.root.canonicalize()?)
            .map_err(SyncFailure::new)?;
        let destination = md.build_dir_for("linkcheck");
        RenderContext::new(md.root, md.book, md.config, destination)
    } else {
        serde_json::from_reader(io::stdin())
            .context("Unable to parse RenderContext")?
    };

    let cache_file = ctx.destination.join("cache.json");
    let cache = load_cache(&cache_file);

    let (files, outcome) = check_links(&ctx, &cache)?;
    log::debug!(
        "cache hits: {}, cache misses: {}",
        cache.cache_hits(),
        cache.cache_misses()
    );
    let diags = outcome.generate_diagnostics();
    report_errors(&files, &diags, args.colour)?;

    save_cache(&cache_file, &cache);

    if diags.iter().any(|diag| diag.severity >= Severity::Error) {
        log::info!("{} broken links found", outcome.invalid_links.len());
        Err(failure::err_msg("One or more incorrect links"))
    } else {
        log::info!("No broken links found");
        Ok(())
    }
}

fn load_cache(filename: &Path) -> Cache {
    log::debug!("Loading cache from {}", filename.display());

    match File::open(filename) {
        Ok(f) => match Cache::load(f) {
            Ok(cache) => cache,
            Err(e) => {
                log::warn!("Unable to deserialize the cache: {}", e);
                Cache::default()
            },
        },
        // Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
        //     log::debug!("Cache file doesn't exist: {}", e);
        //     Cache::default()
        // },
        Err(e) => {
            log::debug!("Unable to open the cache: {}", e);
            Cache::default()
        },
    }
}

fn save_cache(filename: &Path, cache: &Cache) {
    if let Some(parent) = filename.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("Unable to create the cache's directory: {}", e);
        }
    }

    log::debug!("Saving the cache to {}", filename.display());

    match File::create(filename) {
        Ok(f) => {
            if let Err(e) = cache.save(f) {
                log::warn!("Saving the cache as JSON failed: {}", e);
            }
        },
        Err(e) => log::warn!("Unable to create the cache file: {}", e),
    }
}

fn report_errors(
    files: &Files,
    diags: &[Diagnostic],
    colour: ColorChoice,
) -> Result<(), Error> {
    let mut writer = StandardStream::stderr(colour);
    let cfg = codespan_reporting::term::Config::default();

    for diag in diags {
        codespan_reporting::term::emit(&mut writer, &cfg, files, diag)?;
    }

    Ok(())
}

fn check_links(
    ctx: &RenderContext,
    cache: &Cache,
) -> Result<(Files, ValidationOutcome), Error> {
    log::info!("Started the link checker");

    mdbook_linkcheck::version_check(&ctx.version)?;

    let cfg = mdbook_linkcheck::get_config(&ctx.config)?;

    if log::log_enabled!(::log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            log::trace!("{}", line);
        }
    }

    log::info!("Scanning book for links");
    let mut files = Files::new();
    let file_ids =
        mdbook_linkcheck::load_files_into_memory(&ctx.book, &mut files);
    let links = mdbook_linkcheck::extract_links(file_ids, &files);
    log::info!("Found {} links", links.len());
    let src = ctx.source_dir();
    let outcome =
        mdbook_linkcheck::validate(&links, &cfg, &src, &cache, &files)?;

    Ok((files, outcome))
}

#[derive(Debug, Clone, StructOpt)]
struct Args {
    #[structopt(
        short = "s",
        long = "standalone",
        help = "Run standalone (i.e. not as a mdbook plugin)"
    )]
    standalone: bool,
    #[structopt(
        help = "The book to render.",
        parse(from_os_str),
        default_value = "."
    )]
    root: PathBuf,
    #[structopt(
        short = "c",
        long = "colour",
        help = "Output colouring",
        parse(try_from_str = parse_colour),
        default_value = "auto",
        possible_values = &["always", "auto", "never"]
    )]
    colour: ColorChoice,
}

fn parse_colour(raw: &str) -> Result<ColorChoice, Error> {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "auto" => Ok(ColorChoice::Auto),
        "never" => Ok(ColorChoice::Never),
        "always" => Ok(ColorChoice::Always),
        _ => Err(failure::err_msg("Unknown colour choice")),
    }
}
