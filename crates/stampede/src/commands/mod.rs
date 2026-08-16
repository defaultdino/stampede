use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use figment::{
    Figment,
    providers::{Env, Format, Yaml},
};

pub mod deadroll;
pub mod transcode;

#[derive(Parser, Debug)]
enum Subcommand {
    /// transcode discovered media containers into the target codec
    Transcode(self::transcode::Options),
    /// detect dead roll (unimplemented)
    Deadroll(self::deadroll::Options),
}

#[derive(Parser, Debug)]
pub struct Options {
    #[arg(short, long, global = true, action = clap::ArgAction::Append)]
    config: Vec<PathBuf>,
    #[command(subcommand)]
    subcommand: Subcommand,
}

impl Options {
    pub fn run(self, figment: &Figment) -> anyhow::Result<ExitCode> {
        match self.subcommand {
            Subcommand::Transcode(opts) => opts.run(figment),
            Subcommand::Deadroll(opts) => opts.run(figment),
        }
    }

    pub fn figment(&self) -> Figment {
        let configs = if self.config.is_empty() {
            let env_var =
                std::env::var("STAMPEDE_CONFIG").unwrap_or_else(|_| "config.yaml".to_owned());
            std::env::split_paths(&env_var).collect::<Vec<_>>()
        } else {
            self.config.clone()
        };

        let base = Figment::new().merge(Env::prefixed("STAMPEDE_"));

        configs
            .into_iter()
            .fold(base, |f, path| f.admerge(Yaml::file(path)))
    }
}
