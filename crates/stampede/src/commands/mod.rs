use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Yaml},
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

        let from_files = configs
            .into_iter()
            .fold(base, |f, path| f.admerge(Yaml::file(path)));

        match &self.subcommand {
            Subcommand::Transcode(opts) => {
                from_files.admerge(Serialized::defaults(opts.overrides()))
            }
            Subcommand::Deadroll(_) => from_files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use media::video_codec::VideoCodec;

    #[test]
    fn cli_target_overrides_yaml_target() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "config.yaml",
                "transcode:\n  target: vp9\nprocessing:\n  jobs: 2\n  threads_per_job: 4\n  paths: []",
            )?;

            let opts = Options::try_parse_from([
                "stampede",
                "transcode",
                "--config",
                "config.yaml",
                "--target",
                "h264",
            ])
            .unwrap();
            let config: Config = opts.figment().extract().unwrap();

            assert_eq!(config.transcode.target, VideoCodec::H264);
            Ok(())
        });
    }

    #[test]
    fn cli_jobs_overrides_yaml_processing_section() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "config.yaml",
                "transcode:\n  target: vp9\nprocessing:\n  jobs: 2\n  threads_per_job: 4\n  paths: []",
            )?;

            let opts = Options::try_parse_from([
                "stampede",
                "transcode",
                "--config",
                "config.yaml",
                "--jobs",
                "7",
            ])
            .unwrap();
            let config: Config = opts.figment().extract().unwrap();

            assert_eq!(config.processing.jobs, 7);
            Ok(())
        });
    }

    #[test]
    fn env_var_sets_config_path() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "custom.yaml",
                "transcode:\n  target: h264\nprocessing:\n  jobs: 1\n  threads_per_job: 1\n  paths: []",
            )?;
            jail.set_env("STAMPEDE_CONFIG", "custom.yaml");

            let opts = Options::try_parse_from(["stampede", "transcode"]).unwrap();
            let config: Config = opts.figment().extract().unwrap();

            assert_eq!(config.transcode.target, VideoCodec::H264);
            Ok(())
        });
    }

    #[test]
    fn missing_required_yaml_field_errors_cleanly() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "config.yaml",
                "processing:\n  jobs: 2\n  threads_per_job: 4\n  paths: []",
            )?;
            let opts =
                Options::try_parse_from(["stampede", "transcode", "--config", "config.yaml"])
                    .unwrap();
            let result: Result<Config, _> = opts.figment().extract();
            assert!(result.is_err());
            Ok(())
        });
    }
}
