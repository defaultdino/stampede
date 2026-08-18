use std::path::PathBuf;

use crate::config::Config;

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

    

    Ok(())
}

fn read_frames() {

}