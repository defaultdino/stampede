use ffmpeg_next::{codec, filter};

pub fn audio_filter(
    filter_spec: &str,
    decoder: &codec::decoder::Audio,
) -> Result<filter::Graph, ffmpeg_next::Error> {
    let mut filter = filter::Graph::new();
    let args = format!(
        "time_base={}:sample_rate={}:sample_fmt={}:channel_layout=0x{:x}",
        decoder.time_base(),
        decoder.rate(),
        decoder.format().name(),
        decoder.channel_layout().bits()
    );

    filter.add(&filter::find("abuffer").unwrap(), "in", &args)?;
    filter.add(&filter::find("abuffersink").unwrap(), "out", "")?;

    filter
        .output("in", 0)?
        .input("out", 0)?
        .parse(filter_spec)?;

    filter.validate()?;

    Ok(filter)
}

pub fn video_filter(
    filter_spec: &str,
    decoder: &codec::decoder::Video,
) -> Result<filter::Graph, ffmpeg_next::Error> {
    let mut filter = filter::Graph::new();
    let args = format!(
        "video_size={}x{}:pix_fmt={}:time_base={}:pixel_aspect={}",
        decoder.width(),
        decoder.height(),
        decoder
            .format()
            .descriptor()
            .map(|d| d.name())
            .unwrap_or("none"),
        decoder.time_base(),
        decoder.aspect_ratio()
    );

    filter.add(&filter::find("buffer").unwrap(), "in", &args)?;
    filter.add(&filter::find("buffersink").unwrap(), "out", "")?;

    filter
        .output("in", 0)?
        .input("out", 0)?
        .parse(filter_spec)?;

    filter.validate()?;

    Ok(filter)
}
