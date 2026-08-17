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

#[derive(Serialize)]
struct TranscodeOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<VideoCodec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force: Option<bool>,
}

#[derive(Serialize)]
struct ProcessingOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    jobs: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    threads_per_job: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paths: Option<Vec<String>>,
}

/// nested view of the CLI flags, matching the shape of `Config` so figment
/// can deep-merge command-line overrides onto the yaml sections.
#[derive(Serialize)]
pub struct Overrides {
    transcode: TranscodeOverrides,
    processing: ProcessingOverrides,
}

#[derive(Parser, Debug)]
pub struct Options {
    /// the target codec to transcode container media into
    #[arg(short, long, value_enum)]
    target: Option<VideoCodec>,
    /// number of transcode jobs to perform concurrently
    #[arg(short, long)]
    jobs: Option<u8>,
    /// number of threads to utilize per concurrent job
    #[arg(long)]
    threads_per_job: Option<u8>,
    /// paths to scan for media containers
    #[arg(long)]
    paths: Option<Vec<String>>,
    /// allows re-transcoding a file that's already been
    /// through stampede, which causes additional quality loss
    #[arg(short, long)]
    force: Option<bool>,
}

impl Options {
    /// reshapes the flat CLI flags into the nested `Config` layout so
    /// figment can deep-merge them onto the yaml sections.
    pub fn overrides(&self) -> Overrides {
        Overrides {
            transcode: TranscodeOverrides {
                target: self.target,
                force: self.force,
            },
            processing: ProcessingOverrides {
                jobs: self.jobs,
                threads_per_job: self.threads_per_job,
                paths: self.paths.clone(),
            },
        }
    }

    pub fn run(self, figment: &Figment) -> anyhow::Result<ExitCode> {
        let config: Arc<Config> = Arc::new(figment.extract().context("failed to extract config")?);
        let closure_config = config.clone();

        let opts = Arc::new(
            get_codec_opts(&config, config.transcode.target)
                .cloned()
                .unwrap_or_default(),
        );

        process_media(&config.processing, move |path| {
            match transcode(&closure_config, &opts, path) {
                Ok(_) => {
                    log::info!(
                        "finished transcoding video stream to {}",
                        closure_config.transcode.target.codec_id()
                    );
                }
                Err(e) => {
                    log::error!("failed to transcode video stream(s): {}", e);
                }
            }
        });

        Ok(ExitCode::SUCCESS)
    }
}

fn get_codec_opts(config: &Config, codec: VideoCodec) -> Option<&HashMap<String, String>> {
    config.transcode.codecs.get(&codec)
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
