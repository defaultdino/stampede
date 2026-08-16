use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use figment::Figment;
use media::job::process_media;
use media::video_codec::VideoCodec;
use serde::Serialize;

use crate::config::Config;

use self::pipeline::transcode;

mod pipeline;
mod stream_route;
mod transcoder;

#[derive(Parser, Serialize, Debug)]
pub struct Options {
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

impl Options {
    pub fn run(self, figment: &Figment) -> anyhow::Result<ExitCode> {
        let config: Config = figment.extract().context("failed to extract config")?;

        let opts = Arc::new(
            get_codec_opts(&config, config.target)
                .cloned()
                .unwrap_or_default(),
        );
        let target = config.target;
        let log_enabled = config.job.log;

        process_media(&config.job, move |path| {
            match transcode(&opts, log_enabled, path, target) {
                Ok(_) => {
                    log::info!("finished transcoding video stream to {}", target.codec_id());
                }
                Err(e) => {
                    log::error!("failed to transcode video stream {}", e);
                }
            }
        });

        Ok(ExitCode::SUCCESS)
    }
}

fn get_codec_opts(config: &Config, codec: VideoCodec) -> Option<&HashMap<String, String>> {
    config.codecs.get(&codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_and_threads_per_job_are_optional() {
        let opts = Options::try_parse_from(["stampede"]).unwrap();
        assert!(opts.target.is_none());
        assert!(opts.threads_per_job.is_none());
    }

    #[test]
    fn accepts_explicit_target() {
        let opts = Options::try_parse_from(["stampede", "--target", "h264"]).unwrap();
        assert_eq!(opts.target, Some(VideoCodec::H264));
    }
}
