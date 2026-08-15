mod config;

use std::{path::PathBuf, process::ExitCode};
use anyhow::Ok;
use clap::{Parser};
use figment::{Figment, providers::{Env, Format, Yaml}};

#[derive(Parser, Debug)]
pub struct Options {
    #[arg(short, long, global = true, action = clap::ArgAction::Append)]
    config: Vec<PathBuf>,
    #[arg(short, long)]
    target: String,
    #[arg(long)]
    threads_per_job: u16,
    #[arg(long)]
    folders: Vec<String>
}

impl Options {
    fn run(self, _figment: &Figment) -> anyhow::Result<ExitCode> {
        Ok(ExitCode::SUCCESS)
    }

    fn figment(&self) -> Figment {
        let configs = if self.config.is_empty() {
            std::env::var("STAMPEDE_CONFIG")
                .unwrap_or_else(|_| "config.yaml".to_owned())
                .split(':')
                .map(PathBuf::from)
                .collect()
        } else {
            self.config.clone()
        };

        let base = Figment::new().merge(Env::prefixed("PMT_").split("_"));

        configs
            .into_iter()
            .fold(base, |f, path| f.admerge(Yaml::file(path)))
    }
}

fn main() -> anyhow::Result<ExitCode> {
    let options = self::Options::parse();
    let figment = options.figment();

    options.run(&figment)
}
