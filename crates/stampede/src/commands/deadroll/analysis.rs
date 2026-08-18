use ffmpeg_next::{Frame, Packet, Rational, codec, decoder, filter, format, frame, threading};

use crate::commands::deadroll::filter::{audio_filter, video_filter};

pub struct DetectionKeys {
    pub start_key: &'static str,
    pub end_key: &'static str,
}

const BLACK_KEYS: DetectionKeys = DetectionKeys {
    start_key: "lavfi.black_start",
    end_key: "lavfi.black_end",
};
const SILENCE_KEYS: DetectionKeys = DetectionKeys {
    start_key: "lavfi.silence_start",
    end_key: "lavfi.silence_end",
};

pub struct FilterContext {
    pub stream_index: usize,
    pub filter: filter::Graph,
    pub ranges: Vec<(f64, f64)>,
    pub pending_start: Option<f64>,
}

pub struct VideoAnalysisPipeline {
    pub decoder: decoder::Video,
    pub common: FilterContext,
    pub keys: DetectionKeys,

    last_seen_ts: i64,
}

impl VideoAnalysisPipeline {
    fn check_metadata(&mut self, filtered: &Frame) {
        if let Some(start) = filtered.metadata().get(self.keys.start_key) {
            self.common.pending_start = start.parse().ok();
        }
        if let Some(end) = filtered.metadata().get(self.keys.end_key)
            && let (Some(start), Ok(end)) = (self.common.pending_start.take(), end.parse())
        {
            self.common.ranges.push((start, end));
        }
    }

    fn has_frames(&mut self, frame: &mut Frame) -> bool {
        self.common
            .filter
            .get("out")
            .unwrap()
            .sink()
            .frame(frame)
            .is_ok()
    }

    fn process_decoded_frame(&mut self, frame: &frame::Video) {
        if let Some(ts) = frame.timestamp() {
            self.last_seen_ts = ts;
        }
        self.common
            .filter
            .get("in")
            .unwrap()
            .source()
            .add(frame)
            .unwrap();

        let mut filtered = frame::Video::empty();
        while self.has_frames(&mut filtered) {
            self.check_metadata(&filtered);
        }
    }

    pub fn flush(&mut self) {
        let _ = self.decoder.send_eof();

        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.process_decoded_frame(&frame);
        }

        let _ = self.common.filter.get("in").unwrap().source().flush();
        let mut filtered = frame::Video::empty();
        while self.has_frames(&mut filtered) {
            self.check_metadata(&filtered);
        }

        if let Some(start) = self.common.pending_start.take() {
            let end_secs =
                f64::from(Rational(self.last_seen_ts as i32, 1) * self.decoder.time_base());
            self.common.ranges.push((start, end_secs));
        }
    }

    pub fn feed_packet(&mut self, packet: &Packet) {
        if self.decoder.send_packet(packet).is_err() {
            return;
        }

        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.process_decoded_frame(&frame);
        }
    }
}

pub struct AudioAnalysisPipeline {
    pub decoder: decoder::Audio,
    pub common: FilterContext,
    pub keys: DetectionKeys,
    last_seen_ts: i64,
}

impl AudioAnalysisPipeline {
    fn check_metadata(&mut self, filtered: &Frame) {
        if let Some(start) = filtered.metadata().get(self.keys.start_key) {
            self.common.pending_start = start.parse().ok();
        }
        if let Some(end) = filtered.metadata().get(self.keys.end_key)
            && let (Some(start), Ok(end)) = (self.common.pending_start.take(), end.parse())
        {
            self.common.ranges.push((start, end));
        }
    }

    fn has_frames(&mut self, frame: &mut Frame) -> bool {
        self.common
            .filter
            .get("out")
            .unwrap()
            .sink()
            .frame(frame)
            .is_ok()
    }

    fn process_decoded_frame(&mut self, frame: &frame::Video) {
        if let Some(ts) = frame.timestamp() {
            self.last_seen_ts = ts;
        }
        self.common
            .filter
            .get("in")
            .unwrap()
            .source()
            .add(frame)
            .unwrap();

        let mut filtered = frame::Video::empty();
        while self.has_frames(&mut filtered) {
            self.check_metadata(&filtered);
        }
    }

    pub fn flush(&mut self) {
        let _ = self.decoder.send_eof();

        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.process_decoded_frame(&frame);
        }

        let _ = self.common.filter.get("in").unwrap().source().flush();
        let mut filtered = frame::Video::empty();
        while self.has_frames(&mut filtered) {
            self.check_metadata(&filtered);
        }

        if let Some(start) = self.common.pending_start.take() {
            let end_secs =
                f64::from(Rational(self.last_seen_ts as i32, 1) * self.decoder.time_base());
            self.common.ranges.push((start, end_secs));
        }
    }

    pub fn feed_packet(&mut self, packet: &Packet) {
        if self.decoder.send_packet(packet).is_err() {
            return;
        }

        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.process_decoded_frame(&frame);
        }
    }
}

pub struct AnalysisJobConfig {
    pub threads_per_job: usize,
}

pub fn build_video_pipeline(
    ictx: &format::context::Input,
    analysis_job_config: &AnalysisJobConfig,
    stream_index: usize,
    filter_spec: &str,
) -> Result<VideoAnalysisPipeline, ffmpeg_next::Error> {
    let stream = ictx
        .stream(stream_index)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let mut decoder_ctx = codec::context::Context::from_parameters(stream.parameters())?;
    decoder_ctx.set_time_base(stream.time_base());
    decoder_ctx.set_threading(threading::Config {
        kind: threading::Type::Frame,
        count: analysis_job_config.threads_per_job,
    });

    let decoder = decoder_ctx.decoder().video()?;

    let filter = video_filter(filter_spec, &decoder)?;

    Ok(VideoAnalysisPipeline {
        decoder,
        common: FilterContext {
            stream_index,
            filter,
            ranges: Vec::new(),
            pending_start: None,
        },
        last_seen_ts: 0,
        keys: BLACK_KEYS,
    })
}

pub fn build_audio_pipeline(
    ictx: &format::context::Input,
    analysis_job_config: &AnalysisJobConfig,
    stream_index: usize,
    filter_spec: &str,
) -> Result<AudioAnalysisPipeline, ffmpeg_next::Error> {
    let stream = ictx
        .stream(stream_index)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let mut decoder_ctx = codec::context::Context::from_parameters(stream.parameters())?;
    decoder_ctx.set_time_base(stream.time_base());
    decoder_ctx.set_threading(threading::Config {
        kind: threading::Type::Frame,
        count: analysis_job_config.threads_per_job,
    });

    let decoder = decoder_ctx.decoder().audio()?;

    let filter = audio_filter(filter_spec, &decoder)?;

    Ok(AudioAnalysisPipeline {
        decoder,
        common: FilterContext {
            stream_index,
            filter,
            ranges: Vec::new(),
            pending_start: None,
        },
        last_seen_ts: 0,
        keys: SILENCE_KEYS,
    })
}
