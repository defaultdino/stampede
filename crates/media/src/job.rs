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

pub fn walk_media_containers<F>(config: &JobConfig, worker: F)
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

    let discovery_count = discovery_handles.len();
    for handle in discovery_handles {
        handle.join().expect("discovery thread panicked");
    }
    drop(sender);
    log::info!("{discovery_count} discovery threads finished");

    let worker_count = worker_handles.len();
    for handle in worker_handles {
        handle.join().expect("worker thread panicked");
    }
    drop(receiver);
    log::info!("{worker_count} worker threads finished")
}
