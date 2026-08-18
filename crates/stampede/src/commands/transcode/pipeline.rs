use std::{collections::HashMap, path::PathBuf};

use ffmpeg_next::{
    codec, encoder,
    format::{
        self,
        context::{Input, Output},
    },
    media::{self, Type},
};

use ::media::io::{STAMPEDE_TRANSCODE, open_media_ctx, stamp_and_write_output_header};

use crate::{commands::transcode::transcoder::TranscodeJobConfig, config::Config};

use super::stream_route::{StreamRoute, StreamRoutingCtx};
use super::transcoder::{Transcoder, parse_codec_opts};

#[derive(thiserror::Error, Debug)]
pub enum TranscodeError {
    #[error("video streams in this container have already been transcoded")]
    AlreadyTranscoded,

    #[error(transparent)]
    Ffmpeg(#[from] ffmpeg_next::Error),
}

pub fn transcode(
    config: &Config,
    opts: &HashMap<String, String>,
    path: PathBuf,
) -> Result<(), TranscodeError> {
    let codec_opts = parse_codec_opts(opts);

    open_media_ctx(
        &path,
        config.transcode.force,
        STAMPEDE_TRANSCODE,
        || TranscodeError::AlreadyTranscoded,
        |mut input_ctx, mut output_ctx, tmp_out_path| {
            let tmp_out_path_str = tmp_out_path.to_str().unwrap();

            format::context::input::dump(&input_ctx, 0, path.to_str());

            let mut stream_routing_ctx = StreamRoutingCtx::new(input_ctx.nb_streams());
            let transcode_job_config = TranscodeJobConfig {
                threads_per_job: config.processing.threads_per_job as usize,
                target: config.transcode.target,
                opts: codec_opts,
            };

            setup_stream_mapping_and_transcoders(
                tmp_out_path_str,
                &mut output_ctx,
                &input_ctx,
                &transcode_job_config,
                &mut stream_routing_ctx,
            );

            stream_routing_ctx.output_time_bases = stamp_and_write_output_header(
                STAMPEDE_TRANSCODE,
                config.transcode.target.as_str(),
                &mut output_ctx,
                &input_ctx,
                Some(tmp_out_path_str),
            );

            transcode_and_remux_packets(&mut input_ctx, &mut output_ctx, &mut stream_routing_ctx);
            flush_codecs_write_trailer(&mut stream_routing_ctx, &mut output_ctx);

            let did_nothing = stream_routing_ctx.routes.iter().any(
                |r| matches!(r, StreamRoute::Transcode { transcoder, .. } if transcoder.frame_count == 0),
            );

            if did_nothing {
                let _ = std::fs::remove_file(tmp_out_path);
                return Err(TranscodeError::Ffmpeg(ffmpeg_next::Error::InvalidData));
            }

            std::fs::rename(tmp_out_path, &path).map_err(|_| ffmpeg_next::Error::External)?;

            Ok(())
        },
    )
}

fn eligible_input_stream_medium(input_stream_medium: &Type) -> bool {
    matches!(
        input_stream_medium,
        Type::Video | Type::Audio | Type::Subtitle
    )
}

fn setup_stream_mapping_and_transcoders(
    source_label: &str,
    output_ctx: &mut Output,
    input_ctx: &Input,
    transcode_job_config: &TranscodeJobConfig,
    stream_routing_ctx: &mut StreamRoutingCtx,
) {
    let mut output_stream_idx = 0;
    for (input_stream_idx, input_stream) in input_ctx.streams().enumerate() {
        let medium = input_stream.parameters().medium();
        if !eligible_input_stream_medium(&medium) {
            continue;
        }

        stream_routing_ctx.routes[input_stream_idx] = if medium == media::Type::Video
            && input_stream.parameters().id() != transcode_job_config.target.codec_id()
        {
            let transcoder = Transcoder::new(
                &input_stream,
                output_ctx,
                output_stream_idx as _,
                source_label,
                transcode_job_config,
            )
            .unwrap();
            StreamRoute::Transcode {
                output_stream_idx,
                transcoder,
            }
        } else {
            let mut output_stream = output_ctx
                .add_stream(encoder::find(codec::Id::None))
                .unwrap();
            output_stream.set_parameters(input_stream.parameters());
            unsafe {
                (*output_stream.parameters().as_mut_ptr()).codec_tag = 0;
            }
            StreamRoute::Copy {
                output_stream_idx,
                input_stream_time_base: input_stream.time_base(),
            }
        };

        output_stream_idx += 1;
    }
}

fn flush_codecs_write_trailer(stream_routing_ctx: &mut StreamRoutingCtx, output_ctx: &mut Output) {
    for route in stream_routing_ctx.routes.iter_mut() {
        if let StreamRoute::Transcode {
            output_stream_idx,
            transcoder,
        } = route
        {
            let ost_time_base = stream_routing_ctx.output_time_bases[*output_stream_idx];
            transcoder.send_eof_to_decoder();
            transcoder.receive_and_process_decoded_frames(output_ctx, ost_time_base);
            transcoder.send_eof_to_encoder();
            transcoder.receive_and_process_encoded_packets(output_ctx, ost_time_base);
        }
    }
    output_ctx.write_trailer().unwrap();
}

fn transcode_and_remux_packets(
    input_ctx: &mut Input,
    output_ctx: &mut Output,
    stream_routing_ctx: &mut StreamRoutingCtx,
) {
    for (stream, mut packet) in input_ctx.packets() {
        match &mut stream_routing_ctx.routes[stream.index()] {
            StreamRoute::Skip => continue,
            StreamRoute::Transcode {
                output_stream_idx,
                transcoder,
            } => {
                let ost_time_base = stream_routing_ctx.output_time_bases[*output_stream_idx];
                transcoder.send_packet_to_decoder(&packet);
                transcoder.receive_and_process_decoded_frames(output_ctx, ost_time_base);
            }
            StreamRoute::Copy {
                output_stream_idx,
                input_stream_time_base,
            } => {
                let ost_time_base = stream_routing_ctx.output_time_bases[*output_stream_idx];
                packet.rescale_ts(*input_stream_time_base, ost_time_base);
                packet.set_position(-1);
                packet.set_stream(*output_stream_idx as _);
                packet.write_interleaved(output_ctx).unwrap();
            }
        }
    }
}
