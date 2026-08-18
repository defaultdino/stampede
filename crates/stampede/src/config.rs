use std::collections::HashMap;

use media::job::JobConfig;
use media::video_codec::VideoCodec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TranscodeConfig {
    pub target: VideoCodec,
    /// allows re-transcoding a file that's already been
    /// through stampede, which causes additional quality loss
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub codecs: HashMap<VideoCodec, HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BlackrollConfig {
    pub min_duration: usize,
    pub min_db: usize,
    /// allows re-detecting dead roll on a file that's already
    /// been through stampede
    #[serde(default)]
    pub force: bool,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub transcode: TranscodeConfig,
    pub blackroll: BlackrollConfig,
    pub processing: JobConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}
