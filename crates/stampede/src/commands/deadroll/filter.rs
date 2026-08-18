use ffmpeg_next::filter;

pub fn audio_filter(filter_spec: &str) -> Result<filter::Graph, ffmpeg_next::Error> {
    Err(ffmpeg_next::Error::Unknown)
}

pub fn video_filter(filter_spec: &str) -> Result<filter::Graph, ffmpeg_next::Error> {
    Err(ffmpeg_next::Error::Unknown)
}
