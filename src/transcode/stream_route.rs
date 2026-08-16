use ffmpeg_next::Rational;

use crate::transcode::{transcoder::Transcoder, video_codec::VideoCodec};

pub enum StreamRoute {
    Skip,
    Copy {
        output_stream_idx: usize,
        input_stream_time_base: Rational,
    },
    Transcode {
        output_stream_idx: usize,
        transcoder: Transcoder,
    },
}

pub struct StreamRoutingCtx {
    pub target: VideoCodec,
    pub routes: Vec<StreamRoute>,
    pub output_time_bases: Vec<Rational>,
}

impl StreamRoutingCtx {
    pub fn new(target: VideoCodec, n_input_streams: u32) -> Self {
        Self {
            target,
            routes: (0..n_input_streams).map(|_| StreamRoute::Skip).collect(),
            output_time_bases: Vec::new(),
        }
    }
}