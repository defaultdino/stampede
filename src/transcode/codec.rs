use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    Vp9,
}

impl VideoCodec {
    /// maps to ffmpeg-next's codec::Id for encoder/decoder lookups
    pub fn codec_id(self) -> ffmpeg_next::codec::Id {
        use ffmpeg_next::codec::Id;
        match self {
            VideoCodec::H264 => Id::H264,
            VideoCodec::H265 => Id::HEVC,
            VideoCodec::Av1 => Id::AV1,
            VideoCodec::Vp9 => Id::VP9,
        }
    }
}

impl fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "h265",
            VideoCodec::Av1 => "av1",
            VideoCodec::Vp9 => "vp9",
        };
        write!(f, "{s}")
    }
}

impl FromStr for VideoCodec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "h264" | "avc" => Ok(VideoCodec::H264),
            "h265" | "hevc" => Ok(VideoCodec::H265),
            "av1" => Ok(VideoCodec::Av1),
            "vp9" => Ok(VideoCodec::Vp9),
            other => Err(format!("unrecognized codec: {other}")),
        }
    }
}
