use ffmpeg_next::{Packet, Rational, codec, decoder, filter, format, frame};

use crate::commands::deadroll::filter::{audio_filter, video_filter};

pub trait AnalysisDecoder {
    type Frame: EmptyFrame;

    fn decode_packet(&mut self, packet: &Packet) -> Result<(), ffmpeg_next::Error>;
    fn decode_frame(&mut self, frame: &mut Self::Frame) -> Result<(), ffmpeg_next::Error>;

    fn feed_packet(&mut self, filter: &mut filter::Graph, packet: &Packet) {
        if self.decode_packet(packet).is_err() {
            return;
        }
        let mut frame = Self::Frame::empty();
        while self.decode_frame(&mut frame).is_ok() {
            // push into filter, read metadata back out — this part can also be generic
            // if you give AnalysisDecoder a method for "push frame into a filter::graph::Source"
        }
    }
}

pub trait EmptyFrame {
    fn empty() -> Self;
}

impl EmptyFrame for frame::Video {
    fn empty() -> Self {
        frame::Video::empty()
    }
}

impl EmptyFrame for frame::Audio {
    fn empty() -> Self {
        frame::Audio::empty()
    }
}

impl AnalysisDecoder for decoder::Video {
    type Frame = frame::Video;

    fn decode_packet(&mut self, packet: &Packet) -> Result<(), ffmpeg_next::Error> {
        self.send_packet(packet)
    }

    fn decode_frame(&mut self, frame: &mut Self::Frame) -> Result<(), ffmpeg_next::Error> {
        self.receive_frame(frame)
    }
}

impl AnalysisDecoder for decoder::Audio {
    type Frame = frame::Audio;

    fn decode_packet(&mut self, packet: &Packet) -> Result<(), ffmpeg_next::Error> {
        self.send_packet(packet)
    }

    fn decode_frame(&mut self, frame: &mut Self::Frame) -> Result<(), ffmpeg_next::Error> {
        self.receive_frame(frame)
    }
}

pub struct FilterContext {
    pub stream_index: usize,
    pub filter: filter::Graph,
    pub in_time_base: Rational,
}

pub struct VideoAnalysisPipeline {
    decoder: decoder::Video,
    common: FilterContext,
}

pub struct AudioAnalysisPipeline {
    decoder: decoder::Audio,
    common: FilterContext,
}

pub struct AnalysisJobConfig {
    pub threads_per_job: usize,
    pub logging_enabled: bool,
}

fn build_video_pipeline(
    ictx: &format::context::Input,
    stream_index: usize,
    filter_spec: &str,
) -> Result<VideoAnalysisPipeline, ffmpeg_next::Error> {
    let stream = ictx
        .stream(stream_index)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let decoder = codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;
    // build filter::Graph with buffer/buffersink + filter_spec ...
    let filter = video_filter(filter_spec)?;

    Ok(VideoAnalysisPipeline {
        decoder,
        common: FilterContext {
            stream_index,
            filter,
            in_time_base: stream.time_base(),
        },
    })
}

fn build_audio_pipeline(
    ictx: &format::context::Input,
    stream_index: usize,
    filter_spec: &str,
) -> Result<AudioAnalysisPipeline, ffmpeg_next::Error> {
    let stream = ictx
        .stream(stream_index)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let decoder = codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .audio()?;
    // build filter::Graph with abuffer/abuffersink + filter_spec ...
    let filter = audio_filter(filter_spec)?;

    Ok(AudioAnalysisPipeline {
        decoder,
        common: FilterContext {
            stream_index,
            filter,
            in_time_base: stream.time_base(),
        },
    })
}
