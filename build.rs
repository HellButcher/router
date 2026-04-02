use clap::{Command, CommandFactory};
use std::env;
use std::io;
use std::path::{Path, PathBuf};

#[path = "src/args/mod.rs"]
mod args;

const MANUAL_NAME: &str = "Router Manual";

fn render_man<'help>(
    out_dir: &Path,
    date: &str,
    pkg_name: &str,
    pkg_version: &'help str,
    pkg_authors: &'help str,
    parent_name: Option<&str>,
    cmd: &Command,
) -> io::Result<()> {
    let name;
    let bin_name;
    if let Some(parent_name) = parent_name {
        name = format!("{} {}", parent_name, cmd.get_name());
        bin_name = format!("{}-{}", parent_name, cmd.get_name());
    } else {
        name = cmd.get_name().to_owned();
        bin_name = cmd
            .get_bin_name()
            .unwrap_or_else(|| cmd.get_name())
            .to_owned();
    };
    let filename = format!("{}.1", bin_name);
    let cmd = cmd
        .clone()
        .name(name)
        .bin_name(&bin_name)
        .version(pkg_version.to_owned())
        .author(pkg_authors.to_owned());
    let man = clap_mangen::Man::new(cmd.clone())
        .title(bin_name.to_uppercase())
        .manual(MANUAL_NAME)
        .date(date)
        .source(format!("{} {}", pkg_name, pkg_version));
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;
    std::fs::write(out_dir.join(filename), buffer)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let man_out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("man1");
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let pkg_version = env::var("CARGO_PKG_VERSION").unwrap();
    let pkg_authors = env::var("CARGO_PKG_AUTHORS").unwrap();

    let date = String::from_utf8(
        std::process::Command::new("git")
            .args(["show", "-s", "--format=%cd", "--date=format:%Y-%m-%d"])
            .output()?
            .stdout,
    )?;
    let date = date.trim();
    let cmd = args::Cli::command();

    std::fs::create_dir_all(&man_out_dir)?;

    for subcmd in cmd.get_subcommands() {
        render_man(
            &man_out_dir,
            date,
            &pkg_name,
            &pkg_version,
            &pkg_authors,
            Some(cmd.get_name()),
            subcmd,
        )?;
    }

    render_man(
        &man_out_dir,
        date,
        &pkg_name,
        &pkg_version,
        &pkg_authors,
        None,
        &cmd,
    )?;

    Ok(())
}
