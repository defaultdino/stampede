use std::{collections::HashMap, path::PathBuf};

use crossbeam_channel::Receiver;
use ffmpeg_next::{
    Dictionary, Rational, codec, encoder, format::{
        self,
        context::{Input, Output},
    }, media::{self, Type},
};

use crate::transcode::{
    transcoder::{Transcoder, parse_codec_opts},
    video_codec::VideoCodec,
};

mod transcoder;
pub mod video_codec;

struct TranscodingCtx {
    target: VideoCodec,
    stream_mapping: Vec<isize>,
    input_stream_time_bases: Vec<Rational>,
    output_stream_time_bases: Vec<Rational>,
    transcoders: HashMap<usize, Transcoder>,
}

impl TranscodingCtx {
    fn new(
        target: VideoCodec,
        stream_mapping: Vec<isize>,
        input_stream_time_bases: Vec<Rational>,
        output_stream_time_bases: Vec<Rational>,
        transcoders: HashMap<usize, Transcoder>,
    ) -> Self {
        Self {
            target,
            stream_mapping,
            input_stream_time_bases,
            output_stream_time_bases,
            transcoders,
        }
    }
}

pub fn transcode(opts: &HashMap<String, String>, path: PathBuf, target: VideoCodec) {
    // transcodes video stream contents in media container,
    // copies over any other container content to new container

    ffmpeg_next::init().unwrap();

    let codec_opts = parse_codec_opts(opts);
    let mut input_ctx = format::input(&path).unwrap();
    let mut output_ctx = format::output(&path).unwrap();
    format::context::input::dump(&input_ctx, 0, path.to_str());

    let mut transcoding_ctx = TranscodingCtx::new(
        target,
        vec![0; input_ctx.nb_streams() as _],
        vec![Rational(0, 0); input_ctx.nb_streams() as _],
        vec![Rational(0, 0); input_ctx.nb_streams() as _],
        HashMap::<usize, Transcoder>::new(),
    );

    let video_stream_idx = input_ctx
        .streams()
        .best(media::Type::Video)
        .map(|stream| stream.index());
}

fn eligible_input_stream_medium(input_stream_medium: &Type) -> bool {
    let eligible_input_stream_mediums = [
        media::Type::Video,
        media::Type::Audio,
        media::Type::Subtitle,
    ];
    eligible_input_stream_mediums.contains(input_stream_medium)
}

fn mark_input_stream_ineligible(stream_mapping: &mut [isize], input_stream_idx: usize) {
    stream_mapping[input_stream_idx] = -1;
}

fn iterate_streams<'a>(
    codec_opts: Dictionary<'a>,
    video_stream_idx: Option<usize>,
    output_ctx: &mut Output,
    input_ctx: &Input,
    transcoding_ctx: &mut TranscodingCtx,
) {
    let mut output_stream_idx = 0;
    for (input_stream_idx, input_stream) in input_ctx.streams().enumerate() {
        let input_stream_medium = input_stream.parameters().medium();
        if !eligible_input_stream_medium(&input_stream_medium) {
            mark_input_stream_ineligible(&mut transcoding_ctx.stream_mapping, input_stream_idx);
        }

        transcoding_ctx.stream_mapping[input_stream_idx] = output_stream_idx;
        transcoding_ctx.input_stream_time_bases[input_stream_idx] = input_stream.time_base();

        if input_stream_medium == media::Type::Video {
            transcoding_ctx.transcoders.insert(
                input_stream_idx,
                Transcoder::new(
                    &input_stream,
                    output_ctx,
                    output_stream_idx as _,
                    codec_opts.to_owned(),
                    Some(input_stream_idx) == video_stream_idx,
                    transcoding_ctx.target,
                )
                .unwrap(),
            );
        } else {
            let mut output_stream = output_ctx.add_stream(encoder::find(codec::Id::None)).unwrap();
            output_stream.set_parameters(input_stream.parameters());
            unsafe {
                (*output_stream.parameters().as_mut_ptr()).codec_tag = 0;
            }
        }
        output_stream_idx += 1;
    }
}
