mod discover;
mod transcode;

use anyhow::Context;
use clap::Parser;
use crossbeam_channel::{Receiver, Sender, unbounded};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Yaml},
};
use serde::{self, Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
    thread::{self},
};

use crate::{
    discover::discover_media_containers,
    transcode::{transcode, video_codec::VideoCodec},
};

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    target: VideoCodec,
    jobs: u8,
    threads_per_job: u8,
    folders: Vec<PathBuf>,
    #[serde(default)]
    codecs: HashMap<VideoCodec, HashMap<String, String>>,
}

#[derive(Parser, Serialize, Debug)]
pub struct Options {
    #[serde(skip)]
    #[arg(short, long, global = true, action = clap::ArgAction::Append)]
    config: Vec<PathBuf>,
    /// the target codec to transcode container media into
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(short, long, value_enum)]
    target: Option<VideoCodec>,
    /// number of transcode jobs to perform concurrently
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(short, long)]
    jobs: Option<u8>,
    /// number of threads to utilize per concurrent job
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    threads_per_job: Option<u8>,
    /// folders to scan for media containers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    folders: Option<Vec<String>>,
}

fn get_codec_opts(
    config: &Config,
    codec: VideoCodec,
) -> Option<&HashMap<String, String>> {
    config.codecs.get(&codec)
}

fn create_and_join_threads(config: &Config, s: Sender<PathBuf>, r: Receiver<PathBuf>) {
    let opts: HashMap<String, String> = get_codec_opts(config, config.target)
        .cloned()
        .unwrap_or_default();
    let opts = Arc::new(opts);

    let target = config.target;

    let discovery_handles: Vec<_> = config
        .folders
        .iter()
        .cloned()
        .map(|path| {
            let s = s.clone();
            thread::spawn(move || discover_media_containers(&s, path))
        })
        .collect();

    let transcode_handles: Vec<_> = (0..config.jobs)
        .map(|_| {
            let r = r.clone();
            let opts = Arc::clone(&opts);
            thread::spawn(move || {
                while let Ok(path) = r.recv() {
                    transcode(&opts, path, target);
                }
            })
        })
        .collect();

    drop(s);
    drop(r);

    for h in discovery_handles {
        h.join().expect("discovery thread panicked");
    }

    for h in transcode_handles {
        h.join().expect("transcode thread panicked");
    }
}

impl Options {
    fn run(self, figment: &Figment) -> anyhow::Result<ExitCode> {
        let config: Config = figment.extract().context("failed to extract config")?;
        let (s, r) = unbounded::<PathBuf>();
        create_and_join_threads(&config, s, r);
        Ok(ExitCode::SUCCESS)
    }

    fn figment(&self) -> Figment {
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
            .admerge(Serialized::defaults(self))
    }
}

fn main() -> anyhow::Result<ExitCode> {
    env_logger::init();
    let options = Options::parse();
    let figment = options.figment();

    options.run(&figment)
}
