use std::path::PathBuf;

use crossbeam_channel::Receiver;

use crate::transcode::codec::VideoCodec;

pub mod codec;

pub fn transcode(r: &Receiver<PathBuf>, target: VideoCodec) {
    // transcodes video stream contents in media container,
    // copies over any other container content to new container

    
}