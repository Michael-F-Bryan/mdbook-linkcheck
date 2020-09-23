use anyhow::{Context, Error};
use codespan_reporting::term::termcolor::ColorChoice;
use mdbook::{renderer::RenderContext, MDBook};
use std::{io, path::PathBuf};
use structopt::StructOpt;

fn main() -> Result<(), Error> {
    env_logger::init();
    let args = Args::from_args();

    // get a `RenderContext`, either from stdin (because we're used as a plugin)
    // or by instrumenting MDBook directly (in standalone mode).
    let ctx: RenderContext = if args.standalone {
        let md =
            MDBook::load(dunce::canonicalize(&args.root)?).map_err(to_sync)?;
        let destination = md.build_dir_for("linkcheck");
        RenderContext::new(md.root, md.book, md.config, destination)
    } else {
        serde_json::from_reader(io::stdin())
            .context("Unable to parse RenderContext")?
    };

    let cache_file = ctx.destination.join("cache.json");
    mdbook_linkcheck::run(&cache_file, args.colour, &ctx, args.selected_files)
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
    #[structopt(
        short = "f",
        long = "file",
        help = "Check only the given files (check all files if omitted)."
    )]
    selected_files: Option<Vec<String>>,
}

fn parse_colour(raw: &str) -> Result<ColorChoice, Error> {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "auto" => Ok(ColorChoice::Auto),
        "never" => Ok(ColorChoice::Never),
        "always" => Ok(ColorChoice::Always),
        _ => Err(Error::msg("Unknown colour choice")),
    }
}

fn to_sync(err: mdbook::errors::Error) -> Error {
    use std::{
        fmt::{self, Display, Formatter},
        sync::Mutex,
    };

    #[derive(Debug)]
    struct Synchronised(Mutex<mdbook::errors::Error>);

    impl Display for Synchronised {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            self.0.lock().expect("lock was poisoned").fmt(f)
        }
    }

    impl std::error::Error for Synchronised {}

    Error::from(Synchronised(Mutex::new(err)))
}
