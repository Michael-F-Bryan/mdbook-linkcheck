use failure::{Error, ResultExt, SyncFailure};
use mdbook::{renderer::RenderContext, MDBook};
use std::{io, path::PathBuf};
use structopt::StructOpt;

fn main() {
    env_logger::init();
    let _args = Args::from_args();

    unimplemented!();
}

fn _run(args: &Args) -> Result<(), Error> {
    // get a `RenderContext`, either from stdin (because we're used as a plugin)
    // or by instrumenting MDBook directly (in standalone mode).
    let ctx: RenderContext = if args.standalone {
        let md = MDBook::load(&args.root).map_err(SyncFailure::new)?;
        let destination = md.build_dir_for("linkcheck");
        RenderContext::new(md.root, md.book, md.config, destination)
    } else {
        serde_json::from_reader(io::stdin())
            .context("Unable to parse RenderContext")?
    };

    _check_links(&ctx)?;

    Ok(())
}

fn _check_links(ctx: &RenderContext) -> Result<(), Error> {
    log::info!("Started the link checker");

    mdbook_linkcheck::version_check(&ctx.version)?;

    let cfg = mdbook_linkcheck::get_config(&ctx.config)?;

    if log::log_enabled!(::log::Level::Trace) {
        for line in format!("{:#?}", cfg).lines() {
            log::trace!("{}", line);
        }
    }

    log::info!("Scanning book for links");
    unimplemented!();
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
}
