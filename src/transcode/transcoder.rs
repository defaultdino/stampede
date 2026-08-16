use std::{collections::HashMap, time::Instant};

use ffmpeg_next::{Dictionary, Packet, Rational, codec, decoder, encoder, format};

use crate::transcode::video_codec::VideoCodec;

// the resource used here for reference was
// https://github.com/zmwangx/rust-ffmpeg/blob/master/examples/transcode-x264.rs

pub struct Transcoder {
    ost_index: usize,
    decoder: decoder::Video,
    input_time_base: Rational,
    encoder: encoder::Video,
    logging_enabled: bool,
    frame_count: usize,
    last_log_frame_count: usize,
    starting_time: Instant,
    last_log_time: Instant,
}

impl Transcoder {
    pub fn new(
        ist: &format::stream::Stream,
        output_ctx: &mut format::context::Output,
        ost_index: usize,
        opts: Dictionary,
        enable_logging: bool,
        target: VideoCodec,
    ) -> Result<Self, ffmpeg_next::Error> {
        let global_header = output_ctx
            .format()
            .flags()
            .contains(format::Flags::GLOBAL_HEADER);
        let decoder = ffmpeg_next::codec::context::Context::from_parameters(ist.parameters())?
            .decoder()
            .video()?;

        let codec = encoder::find(target.codec_id());
        let mut ost = output_ctx.add_stream(codec)?;

        let mut encoder =
            codec::context::Context::new_with_codec(codec.ok_or(ffmpeg_next::Error::InvalidData)?)
                .encoder()
                .video()?;

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
            .open_with(opts)
            .expect("error opening codec with supplied settings");

        ost.set_parameters(&opened_encoder);

        Ok(Self {
            ost_index,
            decoder,
            input_time_base: ist.time_base(),
            encoder: opened_encoder,
            logging_enabled: enable_logging,
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

    pub fn receive_and_process_encoded_packets(
        &mut self,
        output_ctx: &mut format::context::Output,
        ost_time_base: Rational,
    ) {
        let mut encoded = Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(self.ost_index);
            encoded.rescale_ts(self.input_time_base, ost_time_base);
            encoded.write_interleaved(output_ctx).unwrap();
        }
    }

    pub fn log(&mut self, timestamp: f64) {
        if !self.logging_enabled_and_eligible() {
            return;
        }

        log::info!(
            "time elapsed: \t{:8.2}\tframe count: {:8}\ttimestamp: {:8.2}",
            self.starting_time.elapsed().as_secs_f64(),
            self.frame_count,
            timestamp
        );
        self.last_log_frame_count = self.frame_count;
        self.last_log_time = Instant::now();
    }

    fn logging_enabled_and_eligible(&mut self) -> bool {
        self.logging_enabled
            || (self.frame_count - self.last_log_frame_count < 100
                && self.last_log_time.elapsed().as_secs_f64() < 1.0)
    }
}

pub fn parse_codec_opts(opts: &HashMap<String, String>) -> Dictionary<'_> {
    let mut dict = Dictionary::new();
    for (k, v) in opts {
        dict.set(k, v);
    }
    dict
}
