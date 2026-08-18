use clap::Parser;
use std::process::ExitCode;

use crate::commands::setup_logging;

mod commands;
mod config;

fn main() -> anyhow::Result<ExitCode> {
    setup_logging();
    let options = self::commands::Options::parse();
    let figment = options.figment();
    options.run(&figment)
}
