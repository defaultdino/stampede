use std::{collections::HashMap, path::PathBuf};

use ffmpeg_next::{
    Dictionary, codec, encoder,
    format::{
        self,
        context::{Input, Output},
    },
    media::{self, Type},
};

use ::media::video_codec::VideoCodec;

use super::stream_route::{StreamRoute, StreamRoutingCtx};
use super::transcoder::{Transcoder, parse_codec_opts};

pub fn transcode(
    opts: &HashMap<String, String>,
    threads_per_job: usize,
    log_enabled: bool,
    path: PathBuf,
    target: VideoCodec,
) -> Result<(), ffmpeg_next::Error> {
    // transcodes video stream contents in media container,
    // copies over any other container content to new container

    ffmpeg_next::init().unwrap();
    let output_file_path = path.to_str().ok_or(ffmpeg_next::Error::External)?;

    let codec_opts = parse_codec_opts(opts);
    let mut input_ctx = format::input(&path).unwrap();
    let mut output_ctx = format::output(&path).unwrap();

    format::context::input::dump(&input_ctx, 0, path.to_str());

    let mut stream_routing_ctx = StreamRoutingCtx::new(target, input_ctx.nb_streams());

    setup_stream_mapping_and_transcoders(
        log_enabled,
        threads_per_job,
        codec_opts,
        &mut output_ctx,
        &input_ctx,
        &mut stream_routing_ctx,
    );
    write_output_header(
        &mut output_ctx,
        &input_ctx,
        output_file_path,
        &mut stream_routing_ctx,
    );

    transcode_and_remux_packets(&mut input_ctx, &mut output_ctx, &mut stream_routing_ctx);
    flush_codecs_write_trailer(&mut stream_routing_ctx, &mut output_ctx);

    Ok(())
}

fn eligible_input_stream_medium(input_stream_medium: &Type) -> bool {
    let eligible_input_stream_mediums = [
        media::Type::Video,
        media::Type::Audio,
        media::Type::Subtitle,
    ];
    eligible_input_stream_mediums.contains(input_stream_medium)
}

fn setup_stream_mapping_and_transcoders<'a>(
    log_enabled: bool,
    threads_per_job: usize,
    codec_opts: Dictionary<'a>,
    output_ctx: &mut Output,
    input_ctx: &Input,
    stream_routing_ctx: &mut StreamRoutingCtx,
) {
    let mut output_stream_idx = 0;
    for (input_stream_idx, input_stream) in input_ctx.streams().enumerate() {
        let medium = input_stream.parameters().medium();
        if !eligible_input_stream_medium(&medium) {
            continue;
        }

        stream_routing_ctx.routes[input_stream_idx] = if medium == media::Type::Video
            && input_stream.parameters().id() != stream_routing_ctx.target.codec_id()
        {
            let transcoder = Transcoder::new(
                &input_stream,
                threads_per_job,
                output_ctx,
                output_stream_idx as _,
                codec_opts.to_owned(),
                log_enabled,
                stream_routing_ctx.target,
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

fn write_output_header(
    output_ctx: &mut Output,
    input_ctx: &Input,
    output_file_path: &str,
    stream_routing_ctx: &mut StreamRoutingCtx,
) {
    output_ctx.set_metadata(input_ctx.metadata().to_owned());
    format::context::output::dump(output_ctx, 0, Some(output_file_path));
    output_ctx.write_header().unwrap();

    stream_routing_ctx.output_time_bases =
        output_ctx.streams().map(|ost| ost.time_base()).collect();
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
                packet.rescale_ts(stream.time_base(), transcoder.decoder.time_base());
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
