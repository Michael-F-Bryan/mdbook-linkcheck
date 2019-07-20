use codespan::CodeMap;
use codespan_reporting::{
    termcolor::{ColorChoice, StandardStream},
    Diagnostic, Severity,
};
use failure::{Error, ResultExt, SyncFailure};
use mdbook::{renderer::RenderContext, MDBook};
use mdbook_linkcheck::{Cache, ValidationOutcome};
use std::{io, path::PathBuf};
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

    let (code, outcome) = check_links(&ctx)?;
    let diags = outcome.generate_diagnostics();
    report_errors(&code, &diags, args.colour)?;

    if diags.iter().any(|diag| diag.severity >= Severity::Error) {
        Err(failure::err_msg("One or more incorrect links"))
    } else {
        Ok(())
    }
}

fn report_errors(
    code: &CodeMap,
    diags: &[Diagnostic],
    colour: ColorChoice,
) -> Result<(), Error> {
    let mut writer = StandardStream::stderr(colour);

    for diag in diags {
        codespan_reporting::emit(&mut writer, code, diag)?;
    }

    Ok(())
}

fn check_links(
    ctx: &RenderContext,
) -> Result<(CodeMap, ValidationOutcome), Error> {
    log::info!("Started the link checker");

    mdbook_linkcheck::version_check(&ctx.version)?;

    let cfg = mdbook_linkcheck::get_config(&ctx.config)?;

    if log::log_enabled!(::log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            log::trace!("{}", line);
        }
    }

    log::info!("Scanning book for links");
    let codemap = mdbook_linkcheck::book_to_codemap(&ctx.book);
    let links = mdbook_linkcheck::extract_links(&codemap);
    log::info!("Found {} links", links.len());
    let src = ctx.source_dir();
    let cache = Cache::default();
    let outcome = mdbook_linkcheck::validate(&links, &cfg, &src, &cache)?;

    Ok((codemap, outcome))
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
        parse(try_from_str = "parse_colour"),
        default_value = "auto",
        raw(possible_values = "&[\"always\", \"auto\", \"never\"]")
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
