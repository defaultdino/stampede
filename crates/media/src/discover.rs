use crossbeam_channel::Sender;
use std::{
    path::{Path, PathBuf},
};

const ALLOWED_MEDIA_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "webm", "m4v", "wmv", "flv", "ts", "m2ts",
];

fn visit(path: &Path, cb: &mut dyn FnMut(PathBuf)) {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("failed to read directory {}: {}", path.display(), e);
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                log::warn!("failed to read directory entry under {}: {}", path.display(), e);
                continue;
            }
        };

        let entry_path = entry.path();
        if entry_path.is_dir() {
            visit(&entry_path, cb);
        } else if is_allowed_media_file(&entry_path) {
            cb(entry_path);
        }
    }
}

/// Walks folder to discover media containers in config/cli args.
/// Each new job is broadcasted to shared channel that gets picked up by one
/// of the consumers doing the transcoding work.
pub fn discover_media_containers(s: &Sender<PathBuf>, media_path: PathBuf) {
    let path = Path::new(&media_path);
    if !Path::exists(path) {
        log::warn!("path {} does not exist in filesystem, skipping", path.display());
        return;
    }

    visit(path, &mut |p| match s.send(p.clone()) {
        Ok(_) => log::info!("added {} to queue", path.display()),
        Err(e) => log::info!("skipped adding {} to queue: {}", path.display(), e)
    });
}

fn is_allowed_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ALLOWED_MEDIA_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

