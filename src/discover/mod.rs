use crossbeam_channel::Sender;
use log;
use std::path::{Path, PathBuf};

/// Walks folder to discover media containers in config/cli args.
/// Each new job is broadcasted to shared channel that gets picked up by one
/// of the consumers doing the transcoding work.
pub fn discover_media_containers(s: &Sender<PathBuf>, media_path: PathBuf) {
    let path = Path::new(&media_path);
    if !Path::exists(path) {
        log::warn!(
            "path {} does not exist on this filesystem, skipping",
            path.display()
        );
        return;
    }
    visit(path, &mut |p| match s.send(p.clone()) {
        Ok(_) => {
            log::info!("added {} to transcoding queue", path.display());
        }
        Err(e) => {
            log::info!(
                "skipped adding {} to transcoding queue due to error: {}",
                path.display(),
                e
            );
        }
    })
    .unwrap();
}

fn visit(path: &Path, cb: &mut dyn FnMut(PathBuf)) -> anyhow::Result<()> {
    for e in std::fs::read_dir(path)? {
        let e = e?;
        let path = e.path();
        if path.is_dir() {
            visit(&path, cb)?;
        } else if path.is_file() {
            cb(path);
        }
    }
    Ok(())
}
