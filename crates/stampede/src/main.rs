use clap::Parser;
use std::process::ExitCode;

mod commands;
mod config;

fn main() -> anyhow::Result<ExitCode> {
    env_logger::init();
    let options = self::commands::Options::parse();
    let figment = options.figment();
    options.run(&figment)
}
