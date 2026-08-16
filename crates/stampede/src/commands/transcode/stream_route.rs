use super::transcoder::Transcoder;
use ffmpeg_next::Rational;

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
    pub routes: Vec<StreamRoute>,
    pub output_time_bases: Vec<Rational>,
}

impl StreamRoutingCtx {
    pub fn new(n_input_streams: u32) -> Self {
        Self {
            routes: (0..n_input_streams).map(|_| StreamRoute::Skip).collect(),
            output_time_bases: Vec::new(),
        }
    }
}
