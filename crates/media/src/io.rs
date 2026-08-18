use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use ffmpeg_next::Rational;
use ffmpeg_next::format::{
    self,
    context::{Input, Output},
};

pub const STAMPEDE_DEADROLL: &str = "STAMPEDE_DEADROLL";
pub const STAMPEDE_TRANSCODE: &str = "STAMPEDE_TRANSCODE";

pub fn open_media_ctx<T, E>(
    path: &Path,
    force: bool,
    meta_key: &str,
    already_processed: impl FnOnce() -> E,
    callback: impl FnOnce(Input, Output, &Path) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<ffmpeg_next::Error>,
{
    ffmpeg_next::init()?;

    let input_ctx = format::input(path)?;

    if !force && input_ctx.metadata().get(meta_key).is_some() {
        return Err(already_processed());
    }

    let tmp_out_path = tmp_output_path(path);
    let output_ctx = format::output(&tmp_out_path)?;

    callback(input_ctx, output_ctx, &tmp_out_path)
}

pub fn stamp_and_write_output_header(
    metadata_key: &str,
    metadata_value: &str,
    output_ctx: &mut Output,
    input_ctx: &Input,
    output_file_path: Option<&str>,
) -> Vec<Rational> {
    let mut metadata = input_ctx.metadata().to_owned();
    metadata.set(metadata_key, metadata_value);
    output_ctx.set_metadata(metadata);

    if let Some(output) = output_file_path {
        format::context::output::dump(output_ctx, 0, Some(output));
    }

    output_ctx.write_header().unwrap();

    
    output_ctx.streams().map(|ost| ost.time_base()).collect()
}

fn tmp_output_path(path: &Path) -> PathBuf {
    let extension = path.extension().unwrap_or(OsStr::new(""));
    let file_stem = path.file_stem().unwrap_or(OsStr::new(""));
    path.with_file_name(format!(
        "{}.tmp.{}",
        file_stem.to_string_lossy(),
        extension.to_string_lossy()
    ))
}
