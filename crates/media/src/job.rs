use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::unbounded;
use serde::{Deserialize, Serialize};

use crate::discover::discover_media_containers;

#[derive(Serialize, Deserialize, Debug)]
pub struct JobConfig {
    pub jobs: u8,
    pub threads_per_job: u8,
    pub paths: Vec<PathBuf>,
}

pub fn process_media<F>(config: &JobConfig, worker: F)
where
    F: Fn(PathBuf) + Send + Sync + 'static,
{
    let (sender, receiver) = unbounded::<PathBuf>();
    let worker = Arc::new(worker);

    let discovery_handles: Vec<_> = config
        .paths
        .iter()
        .cloned()
        .map(|path| {
            let sender = sender.clone();
            thread::spawn(move || discover_media_containers(&sender, path))
        })
        .collect();

    drop(sender);
    for handle in discovery_handles {
        handle.join().expect("discovery thread panicked");
    }

    let worker_handles: Vec<_> = (0..config.jobs)
        .map(|_| {
            let receiver = receiver.clone();
            let worker = Arc::clone(&worker);
            thread::spawn(move || {
                while let Ok(path) = receiver.recv() {
                    worker(path);
                }
            })
        })
        .collect();

    drop(receiver);
    for handle in worker_handles {
        handle.join().expect("worker thread panicked");
    }
}
