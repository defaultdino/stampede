use std::collections::HashMap;

use media::job::JobConfig;
use media::video_codec::VideoCodec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TranscodeConfig {
    pub target: VideoCodec,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub codecs: HashMap<VideoCodec, HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub transcode: TranscodeConfig,
    pub processing: JobConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}
