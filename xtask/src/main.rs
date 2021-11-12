use anyhow::{Context, Error};
use build_info::BuildInfo;
use env_logger::{Builder, Env};
use once_cell::sync::Lazy;
use std::{
    fs::File,
    io::{Seek, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use structopt::StructOpt;
use zip::{write::FileOptions, ZipWriter};

fn main() -> Result<(), Error> {
    Builder::from_env(Env::default().default_filter_or("info")).init();
    let cmd = Cmd::from_args();

    match cmd {
        Cmd::Dist { outdir } => {
            log::info!("Generating release artifacts");
            let binary = compile_binary()?;

            let archive = archive_name(&outdir)?;
            if let Some(parent) = archive.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Unable to create the \"{}\" directory",
                        parent.display()
                    )
                })?;
            }

            let f = File::create(&archive)
                .context("Unable to create the archive file")?;
            log::info!(
                "Writing the release archive to \"{}\"",
                archive.display()
            );
            let mut writer = ZipWriter::new(f);

            append_file(&mut writer, &binary)?;
            append_file(&mut writer, PROJECT_ROOT.join("README.md"))?;
            append_file(&mut writer, PROJECT_ROOT.join("LICENSE"))?;
            writer.finish().context("Unable to flush to the archive")?;
        },
    }

    Ok(())
}

fn append_file<W, P>(writer: &mut ZipWriter<W>, path: P) -> Result<(), Error>
where
    W: Write + Seek,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let name = path
        .file_name()
        .context("The path is empty")?
        .to_string_lossy();

    log::debug!("Adding \"{}\" to the archive", name);

    writer
        .start_file(name.as_ref(), FileOptions::default())
        .with_context(|| format!("unable to start writing \"{}\"", name))?;

    let mut f = File::open(path)
        .with_context(|| format!("Unable to open \"{}\"", path.display()))?;
    let bytes_written = std::io::copy(&mut f, writer).with_context(|| {
        format!(
            "Unable to copy the contents of \"{}\" across",
            path.display()
        )
    })?;
    writer.flush()?;
    log::debug!("Wrote {} bytes", bytes_written);

    Ok(())
}

fn archive_name(outdir: &Path) -> Result<PathBuf, Error> {
    let BuildInfo { compiler, .. } = get_build_info();

    let filename = format!(
        "mdbook-linkcheck.{}.zip",
        compiler.target_triple
    );

    Ok(outdir.join(filename))
}

fn compile_binary() -> Result<PathBuf, Error> {
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--package=mdbook-linkcheck")
        .status()
        .context("Unable to invoke `cargo`")?;
    anyhow::ensure!(status.success(), "Cargo returned an error");

    let release_dir = TARGET_DIR.join("release");

    let filename = if cfg!(windows) {
        "mdbook-linkcheck.exe"
    } else {
        "mdbook-linkcheck"
    };
    let binary = release_dir.join(filename);
    log::info!("Compiled to \"{}\"", binary.display());

    match Command::new("strip").arg(&binary).status() {
        Ok(_) => log::debug!("Stripped the binary"),
        Err(e) => log::warn!("Unable to strip the binary: {}", e),
    }

    Ok(binary)
}

#[derive(Debug, Clone, PartialEq, StructOpt)]
pub enum Cmd {
    #[structopt(about = "Create release artifacts")]
    Dist {
        #[structopt(short, long, default_value_os = DIST_DIR.as_os_str())]
        outdir: PathBuf,
    },
}

static PROJECT_ROOT: Lazy<PathBuf> = Lazy::new(|| {
    let dir = std::env::current_dir().unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    });

    for parent in dir.ancestors() {
        if parent.join(".git").is_dir() {
            return parent.to_path_buf();
        }
    }

    dir
});
static TARGET_DIR: Lazy<PathBuf> = Lazy::new(|| PROJECT_ROOT.join("target"));
static DIST_DIR: Lazy<PathBuf> = Lazy::new(|| TARGET_DIR.join("dist"));

build_info::build_info!(fn get_build_info);
