use std::collections::HashMap;

use ffmpeg_next::{
    Dictionary, Packet, Rational, codec, decoder, encoder, format, frame, picture, threading,
};

use media::video_codec::VideoCodec;

// the resource used here for reference was
// https://github.com/zmwangx/rust-ffmpeg/blob/master/examples/transcode-x264.rs

#[derive(thiserror::Error, Debug)]
pub enum TranscoderSetupError {
    #[error("no encoder registered for codec {codec}")]
    NoEncoderForCodec { codec: String },
    #[error("failed to open {codec} encoder with the supplied options")]
    EncoderOpen {
        codec: String,
        #[source]
        source: ffmpeg_next::Error,
    },
    #[error("failed to open {codec} decoder")]
    DecoderOpen {
        codec: String,
        #[source]
        source: ffmpeg_next::Error,
    },
    #[error("the codec ({codec}) does not have the type video")]
    NotVideoCodec { codec: String },
    #[error(
        "failed to allocate codec context with stream parameters for {stream_idx} using {codec}"
    )]
    DecoderContext {
        stream_idx: usize,
        codec: String,
        #[source]
        source: ffmpeg_next::Error,
    },
    #[error("failed to add output stream to context (codec: {codec})")]
    AddStream {
        codec: String,
        #[source]
        source: ffmpeg_next::Error,
    },
}

pub struct Transcoder {
    pub output_stream_idx: usize,
    pub decoder: decoder::Video,
    pub input_time_base: Rational,
    pub encoder: encoder::Video,
    pub source_label: String,
    pub frame_count: usize,
}

pub struct TranscodeJobConfig<'a> {
    pub threads_per_job: usize,
    pub target: VideoCodec,
    pub opts: Dictionary<'a>,
}

impl Transcoder {
    pub fn new(
        input_stream: &format::stream::Stream,
        output_ctx: &mut format::context::Output,
        source_label: impl Into<String>,
        transcode_job_config: &TranscodeJobConfig,
    ) -> Result<Self, TranscoderSetupError> {
        let global_header = output_ctx
            .format()
            .flags()
            .contains(format::Flags::GLOBAL_HEADER);

        let threading_config = threading::Config {
            kind: threading::Type::Frame,
            count: transcode_job_config.threads_per_job,
        };

        let mut decoder_ctx =
            ffmpeg_next::codec::context::Context::from_parameters(input_stream.parameters())
                .map_err(|source| TranscoderSetupError::DecoderContext {
                    stream_idx: input_stream.index(),
                    codec: input_stream.parameters().id().to_string(),
                    source,
                })?;

        decoder_ctx.set_threading(threading_config);
        let decoder =
            decoder_ctx
                .decoder()
                .video()
                .map_err(|source| TranscoderSetupError::DecoderOpen {
                    codec: input_stream.parameters().id().to_string(),
                    source,
                })?;

        let codec = encoder::find(transcode_job_config.target.codec_id()).ok_or_else(||
            TranscoderSetupError::NoEncoderForCodec {
                codec: transcode_job_config.target.codec_id().to_string(),
            },
        )?;

        let mut encoder_ctx = codec::context::Context::new_with_codec(codec);

        encoder_ctx.set_threading(threading_config);
        let mut encoder =
            encoder_ctx
                .encoder()
                .video()
                .map_err(|_| TranscoderSetupError::NotVideoCodec {
                    codec: codec.id().to_string(),
                })?;

        encoder.set_height(decoder.height());
        encoder.set_width(decoder.width());
        encoder.set_aspect_ratio(decoder.aspect_ratio());
        encoder.set_format(decoder.format());
        encoder.set_frame_rate(decoder.frame_rate());
        encoder.set_time_base(input_stream.time_base());

        if global_header {
            encoder.set_flags(codec::Flags::GLOBAL_HEADER);
        }

        let opened_encoder = encoder
            .open_with(transcode_job_config.opts.clone())
            .map_err(|source| TranscoderSetupError::EncoderOpen {
                codec: codec.id().to_string(),
                source,
            })?;

        let mut ost =
            output_ctx
                .add_stream(codec)
                .map_err(|source| TranscoderSetupError::AddStream {
                    codec: codec.id().to_string(),
                    source,
                })?;
        ost.set_parameters(&opened_encoder);

        Ok(Self {
            output_stream_idx: ost.index(),
            decoder,
            input_time_base: input_stream.time_base(),
            encoder: opened_encoder,
            source_label: source_label.into(),
            frame_count: 0,
        })
    }

    pub fn send_packet_to_decoder(&mut self, packet: &Packet) {
        if let Err(e) = self.decoder.send_packet(packet) {
            log::warn!(
                "job={} skipping undecodable packet: {}",
                self.source_label,
                e
            );
        }
    }

    pub fn send_eof_to_decoder(&mut self) -> Result<(), ffmpeg_next::Error> {
        self.decoder.send_eof()
    }

    pub fn send_eof_to_encoder(&mut self) -> Result<(), ffmpeg_next::Error> {
        self.encoder.send_eof()
    }

    fn send_frame_to_encoder(&mut self, frame: &frame::Video) -> Result<(), ffmpeg_next::Error> {
        self.encoder.send_frame(frame)
    }

    pub fn receive_and_process_encoded_packets(
        &mut self,
        output_ctx: &mut format::context::Output,
        output_stream_time_base: Rational,
    ) -> Result<(), ffmpeg_next::Error> {
        let mut encoded = Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(self.output_stream_idx);
            encoded.rescale_ts(self.input_time_base, output_stream_time_base);
            encoded.write_interleaved(output_ctx)?;
        }
        Ok(())
    }

    pub fn receive_and_process_decoded_frames(
        &mut self,
        output_ctx: &mut format::context::Output,
        output_stream_time_base: Rational,
    ) -> Result<(), ffmpeg_next::Error> {
        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            self.frame_count += 1;
            let timestamp = frame.timestamp();
            frame.set_pts(timestamp);
            frame.set_kind(picture::Type::None);
            self.send_frame_to_encoder(&frame)?;
            self.receive_and_process_encoded_packets(output_ctx, output_stream_time_base)?;
        }
        Ok(())
    }
}

pub fn parse_codec_opts(opts: &HashMap<String, String>) -> Dictionary<'_> {
    let mut dict = Dictionary::new();
    for (k, v) in opts {
        dict.set(k, v);
    }
    dict
}
