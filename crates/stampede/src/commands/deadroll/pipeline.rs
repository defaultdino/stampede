use std::path::PathBuf;

use ffmpeg_next::{codec, format, media};

use crate::{commands::deadroll::analysis::{build_audio_pipeline, build_video_pipeline}, config::Config};

#[derive(thiserror::Error, Debug)]
pub enum DeadrollError {
    #[error("media in this container has already been deadroll detected")]
    AlreadyDeadrolled,

    #[error(transparent)]
    Ffmpeg(#[from] ffmpeg_next::Error),
}

// AVFrame carries metadata holding lavfi.black_start and lavfi.black_end
pub fn deadroll(config: &Config, path: PathBuf) -> Result<(), DeadrollError> {

    ffmpeg_next::init().unwrap();

    // 1. decode frames of video/audio stream in media container
    // 2. pass through frames using VideoAnalysisPipeline/AudioAnalysisPipeline, 
    // letting libavfilter set lavfi.black_start/lavfi.black_end and lavfi.silence_start/lavfi.silence_end on frames
    // 3. copy over video/audio frames to same container format dismissing frames marked as silent/black checking metadata

    let input_ctx = format::input(&path).unwrap();
    let video_stream = input_ctx.streams().best(media::Type::Video).expect("could not find best video stream");
    let audio_stream = input_ctx.streams().best(media::Type::Audio).expect("could not find best audio stream");

    let video_filter_spec = "";
    let audio_filter_spec = "";

    let video_analysis_pipeline = build_video_pipeline(&input_ctx, video_stream.index(), video_filter_spec)?;
    let audio_analysis_pipeline = build_audio_pipeline(&input_ctx, audio_stream.index(), audio_filter_spec)?;

    

    Ok(())
}

fn read_frames() {

}