use clap::Parser;
use std::process::ExitCode;

mod commands;
mod config;

fn main() -> anyhow::Result<ExitCode> {
    env_logger::init();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Fatal);
    let options = self::commands::Options::parse();
    let figment = options.figment();
    options.run(&figment)
}
