use std::path::PathBuf;

use ::media::io::{STAMPEDE_DEADROLL, open_media_ctx, stamp_and_write_output_header};
use ffmpeg_next::{
    Packet, Rational, codec, encoder,
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
    cumulative_shift_secs: f64,
    next_range_idx: usize,
}

// AVFrame carries metadata holding lavfi.black_start and lavfi.black_end
pub fn deadroll(config: &Config, path: PathBuf) -> Result<(), DeadrollError> {
    open_media_ctx(
        &path,
        config.blackroll.force,
        STAMPEDE_DEADROLL,
        || DeadrollError::AlreadyDeadrolled,
        |mut input_ctx, mut output_ctx, tmp_out_path| {
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

            let (cut_ranges, keyframe_ts) = get_stream_ts_with_keyframes(
                &mut input_ctx,
                &mut video_analysis_pipeline,
                &mut audio_analysis_pipeline,
            );

            let snapped_keyframes = snap_to_keyframes(&cut_ranges, &keyframe_ts);

            input_ctx.seek(0, ..)?;
            let mut mapping = setup_output_streams(&input_ctx, &mut output_ctx);

            let output_time_bases = stamp_and_write_output_header(
                STAMPEDE_DEADROLL,
                "1",
                &mut output_ctx,
                &input_ctx,
                None,
            );

            if !keyframe_ts.is_empty() {
                write_output_skipping_cuts(
                    &mut input_ctx,
                    &mut output_ctx,
                    &snapped_keyframes,
                    &output_time_bases,
                    &mut mapping,
                );
            } else {
                log::info!("no key frames to cut");
            }

            output_ctx.write_trailer().unwrap();

            std::fs::rename(tmp_out_path, &path).map_err(|_| ffmpeg_next::Error::External)?;

            Ok(())
        },
    )
}

/// Snaps timestamps where blackdetect/silencedetect
/// detected black/silent sections to nearest keyframes (necessary
/// for things like b-frames)
fn snap_to_keyframes(cut_ranges: &[(f64, f64)], keyframes: &[f64]) -> Vec<(f64, f64)> {
    let mut snapped_keyframes = Vec::<(f64, f64)>::new();
    for (start, end) in cut_ranges {
        let l_part_idx = keyframes.partition_point(|&k| k <= *start);
        let s_part_idx = keyframes.partition_point(|&k| k < *end);

        let snapped_start = if l_part_idx == 0 {
            *start
        } else {
            keyframes[l_part_idx - 1]
        };

        let snapped_end = if s_part_idx == keyframes.len() {
            f64::INFINITY
        } else {
            keyframes[s_part_idx]
        };
        snapped_keyframes.push((snapped_start, snapped_end));
    }

    snapped_keyframes
}

fn write_timed_packet(
    packet: &mut Packet,
    pts: i64,
    info: &mut DeadrollStreamInfo,
    cut_ranges: &[(f64, f64)],
    output_time_bases: &[Rational],
) -> bool {
    let pts_seconds = f64::from(Rational(pts as i32, 1) * info.input_time_base);

    while info.next_range_idx < cut_ranges.len() && pts_seconds >= cut_ranges[info.next_range_idx].1
    {
        let (start, end) = cut_ranges[info.next_range_idx];
        info.cumulative_shift_secs += end - start;
        info.next_range_idx += 1;
    }

    if let Some(&(start, end)) = cut_ranges.get(info.next_range_idx)
        && pts_seconds >= start
        && pts_seconds < end
    {
        return false;
    }

    let output_stream_time_base = output_time_bases[info.output_stream_idx];
    packet.rescale_ts(info.input_time_base, output_stream_time_base);

    let had_real_dts = packet.dts().is_some();

    // per-packet shift from the streams history
    let raw_dts = packet
        .dts()
        .or(packet.pts())
        .expect("packet has no dts or pts");

    let shift_ticks = (info.cumulative_shift_secs / f64::from(output_stream_time_base)) as i64;
    let new_dts = raw_dts - shift_ticks;

    if let Some(pts) = packet.pts() {
        packet.set_pts(Some(pts - shift_ticks));
    }

    if had_real_dts {
        packet.set_dts(Some(new_dts));
    } else {
        packet.set_dts(None);
    }

    true
}

/// A DeadrollStreamInfo holds which cut range this stream is currently up to
/// and total seconds worth of cut content so far for this stream
fn write_output_skipping_cuts(
    input_ctx: &mut Input,
    output_ctx: &mut Output,
    cut_ranges: &[(f64, f64)],
    output_time_bases: &[Rational],
    mapping: &mut [Option<DeadrollStreamInfo>],
) {
    for (stream, mut packet) in input_ctx.packets() {
        let Some(info) = &mut mapping[stream.index()] else {
            continue;
        };

        let keep = match packet.pts() {
            Some(pts) => write_timed_packet(&mut packet, pts, info, cut_ranges, output_time_bases),
            None => true,
        };

        if !keep {
            continue;
        }

        packet.set_position(-1);
        packet.set_stream(info.output_stream_idx as _);
        if let Err(e) = packet.write_interleaved(output_ctx) {
            log::error!("failed to write interleaved packet: {e}",)
        }
    }
}

fn setup_output_streams(
    input_ctx: &Input,
    output_ctx: &mut Output,
) -> Vec<Option<DeadrollStreamInfo>> {
    let mut mapping: Vec<Option<DeadrollStreamInfo>> =
        (0..input_ctx.nb_streams()).map(|_| None).collect();

    for (output_stream_idx, (input_stream_idx, input_stream)) in
        input_ctx.streams().enumerate().enumerate()
    {
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
            cumulative_shift_secs: 0.0,
            next_range_idx: 0,
        });
    }

    mapping
}

fn get_stream_ts_with_keyframes(
    input_ctx: &mut Input,
    video_analysis_pipeline: &mut VideoAnalysisPipeline,
    audio_analysis_pipeline: &mut AudioAnalysisPipeline,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    let mut keyframe_ts = Vec::<f64>::new();

    for (stream, packet) in input_ctx.packets() {
        if stream.index() == video_analysis_pipeline.common.stream_index {
            video_analysis_pipeline.feed_packet(&packet);
            if packet.is_key()
                && let Some(pts) = packet.pts()
            {
                // calculate accumulator
                let time_base = stream.time_base();
                let packet_pts_seconds = f64::from(Rational(pts as i32, 1) * time_base);
                keyframe_ts.push(packet_pts_seconds);
            }
        }
        if stream.index() == audio_analysis_pipeline.common.stream_index {
            audio_analysis_pipeline.feed_packet(&packet);
        }
    }

    video_analysis_pipeline.flush();
    audio_analysis_pipeline.flush();

    (
        intersect_ranges(
            &video_analysis_pipeline.common.ranges,
            &audio_analysis_pipeline.common.ranges,
        ),
        keyframe_ts,
    )
}

/// Intersects timestamp ranges where both blackdetect
/// and silentdetect determined there was no content
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
