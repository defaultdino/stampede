use std::collections::HashMap;

use media::job::JobConfig;
use media::video_codec::VideoCodec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(flatten)]
    pub job: JobConfig,
    pub target: VideoCodec,
    #[serde(default)]
    pub codecs: HashMap<VideoCodec, HashMap<String, String>>,
}
