use std::path::PathBuf;

use ::media::io::{STAMPEDE_DEADROLL, open_media_ctx, stamp_and_write_output_header};
use ffmpeg_next::{
    Rational, codec, encoder,
    format::context::{Input, Output},
    media,
};

use crate::{
    commands::deadroll::analysis::{
        AnalysisJobConfig, AudioAnalysisPipeline, VideoAnalysisPipeline, build_audio_pipeline,
        build_video_pipeline,
    },
    config::Config,
};

#[derive(thiserror::Error, Debug)]
pub enum DeadrollError {
    #[error("media in this container has already been deadroll detected")]
    AlreadyDeadrolled,

    #[error(transparent)]
    Ffmpeg(#[from] ffmpeg_next::Error),
}

struct DeadrollStreamInfo {
    output_stream_idx: usize,
    input_time_base: Rational,
}

// AVFrame carries metadata holding lavfi.black_start and lavfi.black_end
pub fn deadroll(config: &Config, path: PathBuf) -> Result<(), DeadrollError> {
    open_media_ctx(
        &path,
        config.blackroll.force,
        STAMPEDE_DEADROLL,
        || DeadrollError::AlreadyDeadrolled,
        |mut input_ctx, mut output_ctx, _| {
            let video_stream = input_ctx
                .streams()
                .best(media::Type::Video)
                .expect("could not find best video stream");
            let audio_stream = input_ctx
                .streams()
                .best(media::Type::Audio)
                .expect("could not find best audio stream");

            let video_filter_spec = format!("blackdetect=d={}", config.blackroll.min_duration);
            let audio_filter_spec = format!(
                "silencedetect=n=-{}dB:d={}",
                config.blackroll.min_db, config.blackroll.min_duration
            );

            let analysis_job_config = AnalysisJobConfig {
                threads_per_job: config.processing.threads_per_job as usize,
            };

            let mut video_analysis_pipeline = build_video_pipeline(
                &input_ctx,
                &analysis_job_config,
                video_stream.index(),
                video_filter_spec.as_str(),
            )?;
            let mut audio_analysis_pipeline = build_audio_pipeline(
                &input_ctx,
                &analysis_job_config,
                audio_stream.index(),
                audio_filter_spec.as_str(),
            )?;

            let cut_ranges = calc_stream_ranges(
                &mut input_ctx,
                &mut video_analysis_pipeline,
                &mut audio_analysis_pipeline,
            );

            input_ctx.seek(0, ..)?;
            let mapping = setup_output_streams(&input_ctx, &mut output_ctx);

            let output_time_bases = stamp_and_write_output_header(
                STAMPEDE_DEADROLL,
                "1",
                &mut output_ctx,
                &input_ctx,
                None,
            );

            write_output_skipping_cuts(
                &mut input_ctx,
                &mut output_ctx,
                &cut_ranges,
                &output_time_bases,
                &mapping,
            );
            
            output_ctx.write_trailer().unwrap();

            Ok(())
        },
    )
}

fn write_output_skipping_cuts(
    input_ctx: &mut Input,
    output_ctx: &mut Output,
    cut_ranges: &[(f64, f64)],
    output_time_bases: &[Rational],
    mapping: &[Option<DeadrollStreamInfo>],
) {
    let mut cumulative_offset = 0.0_f64;
    let mut cut_iter = cut_ranges.iter().peekable();

    for (stream, mut packet) in input_ctx.packets() {
        let Some(info) = &mapping[stream.index()] else {
            continue;
        };

        let Some(pts) = packet.pts() else {
            continue;
        };

        let pts_seconds = f64::from(Rational(pts as i32, 1) * info.input_time_base);

        while let Some(&&(_, end)) = cut_iter.peek() {
            if pts_seconds >= end {
                let (start, end) = *cut_iter.next().unwrap();
                cumulative_offset += end - start;
            } else {
                break;
            }
        }

        if let Some(&&(start, end)) = cut_iter.peek()
            && pts_seconds >= start
            && pts_seconds < end
        {
            continue;
        }

        let output_stream_time_base = output_time_bases[info.output_stream_idx];
        packet.rescale_ts(info.input_time_base, output_stream_time_base);
        
        let offset_ticks = (cumulative_offset / f64::from(output_stream_time_base)) as i64;

        if let Some(pts) =  packet.pts() {
            packet.set_pts(Some(pts - offset_ticks));
        }

        if let Some(dts) = packet.dts() {
            packet.set_dts(Some(dts - offset_ticks));
        }

        packet.set_position(-1);
        packet.set_stream(info.output_stream_idx as _);
        packet.write_interleaved(output_ctx).unwrap();
    }
}

fn setup_output_streams(
    input_ctx: &Input,
    output_ctx: &mut Output,
) -> Vec<Option<DeadrollStreamInfo>> {
    let mut mapping: Vec<Option<DeadrollStreamInfo>> =
        (0..input_ctx.nb_streams()).map(|_| None).collect();
    let mut output_stream_idx = 0;

    for (input_stream_idx, input_stream) in input_ctx.streams().enumerate() {
        let medium = input_stream.parameters().medium();
        if !matches!(medium, media::Type::Video | media::Type::Audio) {
            continue;
        }

        let mut output_stream = output_ctx
            .add_stream(encoder::find(codec::Id::None))
            .unwrap();
        output_stream.set_parameters(input_stream.parameters());
        unsafe {
            (*output_stream.parameters().as_mut_ptr()).codec_tag = 0;
        }

        mapping[input_stream_idx] = Some(DeadrollStreamInfo {
            output_stream_idx,
            input_time_base: input_stream.time_base(),
        });
        output_stream_idx += 1;
    }

    mapping
}

fn calc_stream_ranges(
    input_ctx: &mut Input,
    video_analysis_pipeline: &mut VideoAnalysisPipeline,
    audio_analysis_pipeline: &mut AudioAnalysisPipeline,
) -> Vec<(f64, f64)> {
    for (stream, packet) in input_ctx.packets() {
        if stream.index() == video_analysis_pipeline.common.stream_index {
            video_analysis_pipeline.feed_packet(&packet);
        }
        if stream.index() == audio_analysis_pipeline.common.stream_index {
            audio_analysis_pipeline.feed_packet(&packet);
        }
    }

    video_analysis_pipeline.flush();
    audio_analysis_pipeline.flush();

    intersect_ranges(
        &video_analysis_pipeline.common.ranges,
        &audio_analysis_pipeline.common.ranges,
    )
}

fn intersect_ranges(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
    for &(a_start, a_end) in a {
        for &(b_start, b_end) in b {
            let start = a_start.max(b_start);
            let end = a_end.min(b_end);
            if start < end {
                result.push((start, end));
            }
        }
    }
    result
}
