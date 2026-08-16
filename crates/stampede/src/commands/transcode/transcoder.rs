use std::{collections::HashMap, time::Instant};

use ffmpeg_next::{
    Dictionary, Packet, Rational, codec, decoder, encoder, format, frame, picture, threading,
};

use media::video_codec::VideoCodec;

// the resource used here for reference was
// https://github.com/zmwangx/rust-ffmpeg/blob/master/examples/transcode-x264.rs

pub struct Transcoder {
    pub output_stream_idx: usize,
    pub decoder: decoder::Video,
    pub input_time_base: Rational,
    pub encoder: encoder::Video,
    pub logging_enabled: bool,
    pub source_label: String,
    pub frame_count: usize,
    pub last_log_frame_count: usize,
    pub starting_time: Instant,
    pub last_log_time: Instant,
}

pub struct TranscodeJobConfig<'a> {
    pub threads_per_job: usize,
    pub target: VideoCodec,
    pub logging_enabled: bool,
    pub opts: Dictionary<'a>,
}

impl Transcoder {
    pub fn new(
        ist: &format::stream::Stream,
        output_ctx: &mut format::context::Output,
        output_stream_idx: usize,
        source_label: impl Into<String>,
        transcode_job_config: &TranscodeJobConfig,
    ) -> Result<Self, ffmpeg_next::Error> {
        let global_header = output_ctx
            .format()
            .flags()
            .contains(format::Flags::GLOBAL_HEADER);

        let threading_config = threading::Config {
            kind: threading::Type::Frame,
            count: transcode_job_config.threads_per_job,
        };

        let mut decoder_ctx =
            ffmpeg_next::codec::context::Context::from_parameters(ist.parameters())?;
        decoder_ctx.set_threading(threading_config);
        let decoder = decoder_ctx.decoder().video()?;

        let codec = encoder::find(transcode_job_config.target.codec_id());
        let mut ost = output_ctx.add_stream(codec)?;

        let mut encoder_ctx =
            codec::context::Context::new_with_codec(codec.ok_or(ffmpeg_next::Error::InvalidData)?);
        encoder_ctx.set_threading(threading_config);
        let mut encoder = encoder_ctx.encoder().video()?;

        ost.set_parameters(&encoder);

        encoder.set_height(decoder.height());
        encoder.set_width(decoder.width());
        encoder.set_aspect_ratio(decoder.aspect_ratio());
        encoder.set_format(decoder.format());
        encoder.set_frame_rate(decoder.frame_rate());
        encoder.set_time_base(ist.time_base());

        if global_header {
            encoder.set_flags(codec::Flags::GLOBAL_HEADER);
        }

        let opened_encoder = encoder
            .open_with(transcode_job_config.opts.clone())
            .expect("error opening codec with supplied settings");

        ost.set_parameters(&opened_encoder);

        Ok(Self {
            output_stream_idx,
            decoder,
            input_time_base: ist.time_base(),
            encoder: opened_encoder,
            logging_enabled: transcode_job_config.logging_enabled,
            source_label: source_label.into(),
            frame_count: 0,
            last_log_frame_count: 0,
            starting_time: Instant::now(),
            last_log_time: Instant::now(),
        })
    }

    pub fn send_packet_to_decoder(&mut self, packet: &Packet) {
        self.decoder.send_packet(packet).unwrap();
    }

    pub fn send_eof_to_decoder(&mut self) {
        self.decoder.send_eof().unwrap();
    }

    pub fn send_eof_to_encoder(&mut self) {
        self.encoder.send_eof().unwrap();
    }

    fn send_frame_to_encoder(&mut self, frame: &frame::Video) {
        self.encoder.send_frame(frame).unwrap();
    }

    pub fn receive_and_process_encoded_packets(
        &mut self,
        output_ctx: &mut format::context::Output,
        ost_time_base: Rational,
    ) {
        let mut encoded = Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(self.output_stream_idx);
            encoded.rescale_ts(self.input_time_base, ost_time_base);
            encoded.write_interleaved(output_ctx).unwrap();
        }
    }

    pub fn receive_and_process_decoded_frames(
        &mut self,
        octx: &mut format::context::Output,
        ost_time_base: Rational,
    ) {
        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.frame_count += 1;
            let timestamp = frame.timestamp();
            self.log(f64::from(
                Rational(timestamp.unwrap_or(0) as i32, 1) * self.decoder.time_base(),
            ));
            frame.set_pts(timestamp);
            frame.set_kind(picture::Type::None);
            self.send_frame_to_encoder(&frame);
            self.receive_and_process_encoded_packets(octx, ost_time_base);
        }
    }

    pub fn log(&mut self, timestamp: f64) {
        if !self.logging_enabled_and_eligible() {
            return;
        }

        let frames_since_last_log = self.frame_count - self.last_log_frame_count;
        let time_since_last_log = self.last_log_time.elapsed().as_secs_f64();
        let fps = frames_since_last_log as f64 / time_since_last_log;

        log::info!(
            "job={} frame={} fps={:.1} elapsed={:.1}s ts={:.2}",
            self.source_label,
            self.frame_count,
            fps,
            self.starting_time.elapsed().as_secs_f64(),
            timestamp
        );
        self.last_log_frame_count = self.frame_count;
        self.last_log_time = Instant::now();
    }

    fn logging_enabled_and_eligible(&mut self) -> bool {
        self.logging_enabled
            && (self.frame_count - self.last_log_frame_count >= 100
                || self.last_log_time.elapsed().as_secs_f64() >= 1.0)
    }
}

pub fn parse_codec_opts(opts: &HashMap<String, String>) -> Dictionary<'_> {
    let mut dict = Dictionary::new();
    for (k, v) in opts {
        dict.set(k, v);
    }
    dict
}
