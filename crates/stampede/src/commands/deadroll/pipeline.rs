use std::path::PathBuf;

use ::media::io::{STAMPEDE_DEADROLL, open_media_ctx, stamp_and_write_output_header};
use ffmpeg_next::{
    Packet, Rational, codec, encoder,
    ffi::{AV_NOPTS_VALUE, AV_TIME_BASE},
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

const EPSILON: f64 = 1.0;
const TRIM_WARN_SECS: f64 = 1.0;

#[derive(thiserror::Error, Debug)]
pub enum DeadrollError {
    #[error("media in this container has already been deadroll detected")]
    AlreadyDeadrolled,

    #[error("detection cut every packet; refusing to overwrite {path:?} with an empty file")]
    EmptyOutput { path: PathBuf },

    #[error(transparent)]
    Ffmpeg(#[from] ffmpeg_next::Error),

    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

struct DeadrollStreamInfo {
    output_stream_idx: usize,
    input_time_base: Rational,
    last_output_dts: Option<i64>,
}

pub fn deadroll(config: &Config, path: PathBuf) -> Result<(), DeadrollError> {
    open_media_ctx(
        &path,
        config.deadroll.force,
        STAMPEDE_DEADROLL,
        || DeadrollError::AlreadyDeadrolled,
        |mut input_ctx, mut output_ctx, tmp_out_path| {
            let video_stream = input_ctx
                .streams()
                .best(media::Type::Video)
                .ok_or(ffmpeg_next::Error::StreamNotFound)?;

            let audio_stream = input_ctx
                .streams()
                .best(media::Type::Audio)
                .ok_or(ffmpeg_next::Error::StreamNotFound)?;

            let video_filter_spec = format!("blackdetect=d={}", config.deadroll.min_duration);
            let audio_filter_spec = format!(
                "silencedetect=n=-{}dB:d={}",
                config.deadroll.min_db, config.deadroll.min_duration
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

            let duration_secs = match input_ctx.duration() {
                AV_NOPTS_VALUE => f64::INFINITY,
                d => d as f64 / f64::from(AV_TIME_BASE),
            };

            let snapped_keyframes = snap_to_keyframes(&cut_ranges, &keyframe_ts, duration_secs);

            log::info!(
                "detected {} black∩silence range(s): {:?}; {} keyframe(s); snapped cuts: {:?}",
                cut_ranges.len(),
                cut_ranges,
                keyframe_ts.len(),
                snapped_keyframes,
            );

            input_ctx.seek(0, ..)?;
            let mut mapping = setup_output_streams(&input_ctx, &mut output_ctx)?;

            let output_time_bases = stamp_and_write_output_header(
                STAMPEDE_DEADROLL,
                "1",
                &mut output_ctx,
                &input_ctx,
                None,
            )?;

            if !keyframe_ts.is_empty() {
                let kept = write_output_skipping_cuts(
                    &mut input_ctx,
                    &mut output_ctx,
                    &snapped_keyframes,
                    &output_time_bases,
                    &mut mapping,
                );
                if kept == 0 {
                    std::fs::remove_file(tmp_out_path)?;
                    return Err(DeadrollError::EmptyOutput { path: path.clone() });
                }
                output_ctx.write_trailer()?;
                std::fs::rename(tmp_out_path, &path).map_err(|_| ffmpeg_next::Error::External)?;
            } else {
                log::info!("no key frames to cut");
                std::fs::remove_file(tmp_out_path)?;
            }
            Ok(())
        },
    )
}

/// Snaps timestamps where blackdetect/silencedetect
/// detected black/silent sections to nearest keyframes (necessary
/// for things like b-frames)
fn snap_to_keyframes(
    cut_ranges: &[(f64, f64)],
    keyframes: &[f64],
    duration_secs: f64,
) -> Vec<(f64, f64)> {
    let mut snapped_keyframes = Vec::<(f64, f64)>::new();
    for (start, end) in cut_ranges {
        let l_part_idx = keyframes.partition_point(|&k| k <= *start);
        let s_part_idx = keyframes.partition_point(|&k| k < *end);

        let snapped_start = if l_part_idx == 0 {
            *start
        } else {
            keyframes[l_part_idx - 1]
        };

        let trimmed = *start - snapped_start;
        if trimmed > TRIM_WARN_SECS {
            log::warn!(
                "cut at {start:.3}s snapped back to keyframe {snapped_start:.3}s, discarding {trimmed:.3}s of content before the dead roll",
            );
        }

        if s_part_idx == keyframes.len() {
            if *end >= duration_secs - EPSILON {
                snapped_keyframes.push((snapped_start, f64::INFINITY));
            } else {
                log::warn!("skipping cut at {end:.3}s: no keyframe after it to resume on");
            }
        } else {
            snapped_keyframes.push((snapped_start, keyframes[s_part_idx]));
        }
    }

    normalize_ranges(snapped_keyframes)
}

fn cut_seconds_before(t: f64, ranges: &[(f64, f64)]) -> f64 {
    ranges
        .iter()
        .take_while(|(_, end)| *end <= t)
        .map(|(start, end)| end - start)
        .sum()
}

fn is_within_cut(t: f64, ranges: &[(f64, f64)]) -> bool {
    ranges.iter().any(|&(start, end)| t >= start && t < end)
}

fn stamp_shifted(
    packet: &mut Packet,
    info: &mut DeadrollStreamInfo,
    output_time_base: Rational,
    cut_ranges: &[(f64, f64)],
) {
    let Some(anchor) = packet.pts().or_else(|| packet.dts()) else {
        return;
    };
    let anchor_secs = anchor as f64 * f64::from(info.input_time_base);
    let shift_secs = cut_seconds_before(anchor_secs, cut_ranges);

    packet.rescale_ts(info.input_time_base, output_time_base);
    let shift_ticks = (shift_secs / f64::from(output_time_base)) as i64;

    if let Some(pts) = packet.pts() {
        packet.set_pts(Some(pts - shift_ticks));
    }

    if let Some(dts) = packet.dts() {
        let mut shifted = dts - shift_ticks;
        if let Some(last) = info.last_output_dts
            && shifted <= last
        {
            shifted = last + 1;
        }
        info.last_output_dts = Some(shifted);
        packet.set_dts(Some(shifted));
    }
}

fn write_output_skipping_cuts(
    input_ctx: &mut Input,
    output_ctx: &mut Output,
    cut_ranges: &[(f64, f64)],
    output_time_bases: &[Rational],
    mapping: &mut [Option<DeadrollStreamInfo>],
) -> u64 {
    let mut kept = 0u64;
    let mut dropped = 0u64;
    for (stream, mut packet) in input_ctx.packets() {
        let Some(info) = &mut mapping[stream.index()] else {
            continue;
        };

        if let Some(pts) = packet.pts() {
            let pts_secs = pts as f64 * f64::from(info.input_time_base);
            if is_within_cut(pts_secs, cut_ranges) {
                dropped += 1;
                continue;
            }
        }

        let output_time_base = output_time_bases[info.output_stream_idx];
        stamp_shifted(&mut packet, info, output_time_base, cut_ranges);

        packet.set_position(-1);
        packet.set_stream(info.output_stream_idx as _);
        if let Err(e) = packet.write_interleaved(output_ctx) {
            log::error!("failed to write interleaved packet: {e}")
        } else {
            kept += 1;
        }
    }

    log::info!("kept {kept} packet(s), dropped {dropped} packet(s)");
    kept
}

fn setup_output_streams(
    input_ctx: &Input,
    output_ctx: &mut Output,
) -> Result<Vec<Option<DeadrollStreamInfo>>, ffmpeg_next::Error> {
    let mut mapping: Vec<Option<DeadrollStreamInfo>> =
        (0..input_ctx.nb_streams()).map(|_| None).collect();

    for (output_stream_idx, (input_stream_idx, input_stream)) in
        input_ctx.streams().enumerate().enumerate()
    {
        let mut output_stream = output_ctx.add_stream(encoder::find(codec::Id::None))?;

        output_stream.set_parameters(input_stream.parameters());
        unsafe {
            (*output_stream.parameters().as_mut_ptr()).codec_tag = 0;
        }

        mapping[input_stream_idx] = Some(DeadrollStreamInfo {
            output_stream_idx,
            input_time_base: input_stream.time_base(),
            last_output_dts: None,
        });
    }

    Ok(mapping)
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
                let packet_pts_seconds = pts as f64 * f64::from(time_base);
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

fn normalize_ranges(mut ranges: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    ranges.retain(|(start, end)| end > start);
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn intersect_ranges(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let a = normalize_ranges(a.to_vec());
    let b = normalize_ranges(b.to_vec());

    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let start = a[i].0.max(b[j].0);
        let end = a[i].1.min(b[j].1);
        if start < end {
            result.push((start, end));
        }
        if a[i].1 <= b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_sorts_and_merges_overlapping_and_touching() {
        let got = normalize_ranges(vec![
            (3.0, 6.0),
            (0.0, 2.0),
            (1.0, 4.0),
            (6.0, 8.0),
            (9.0, 9.0),
        ]);
        assert_eq!(got, vec![(0.0, 8.0)]);
    }

    #[test]
    fn intersect_normalizes_messy_inputs() {
        let got = intersect_ranges(&[(0.0, 10.0)], &[(2.0, 5.0), (4.0, 8.0)]);
        assert_eq!(got, vec![(2.0, 8.0)]);
    }

    #[test]
    fn intersect_keeps_only_common_spans() {
        let got = intersect_ranges(&[(0.0, 3.0), (5.0, 9.0)], &[(2.0, 6.0), (8.0, 10.0)]);
        assert_eq!(got, vec![(2.0, 3.0), (5.0, 6.0), (8.0, 9.0)]);
    }

    #[test]
    fn membership_is_half_open() {
        let ranges = [(10.0, 20.0), (30.0, 40.0)];
        assert!(!is_within_cut(5.0, &ranges));
        assert!(is_within_cut(10.0, &ranges));
        assert!(is_within_cut(15.0, &ranges));
        assert!(!is_within_cut(20.0, &ranges));
        assert!(!is_within_cut(25.0, &ranges));
        assert!(is_within_cut(35.0, &ranges));
    }

    #[test]
    fn shift_sums_only_cuts_that_end_before_t() {
        let ranges = [(10.0, 20.0), (30.0, 40.0)];
        assert_eq!(cut_seconds_before(5.0, &ranges), 0.0);
        assert_eq!(cut_seconds_before(25.0, &ranges), 10.0);
        assert_eq!(cut_seconds_before(45.0, &ranges), 20.0);
    }

    #[test]
    fn snap_widens_cut_to_surrounding_keyframes() {
        let keyframes = [0.0, 10.0, 20.0, 30.0, 40.0];
        let got = snap_to_keyframes(&[(12.0, 18.0)], &keyframes, 45.0);
        assert_eq!(got, vec![(10.0, 20.0)]);
    }

    #[test]
    fn snap_extends_trailing_deadroll_to_eof() {
        let keyframes = [0.0, 10.0, 20.0, 30.0, 40.0];
        let got = snap_to_keyframes(&[(42.0, 45.0)], &keyframes, 45.0);
        assert_eq!(got, vec![(40.0, f64::INFINITY)]);
    }

    #[test]
    fn snap_skips_mid_file_cut_with_no_following_keyframe() {
        let got = snap_to_keyframes(&[(50.0, 60.0)], &[0.0], 200.0);
        assert!(got.is_empty());
    }
}
