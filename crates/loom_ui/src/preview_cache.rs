//! Asynchronous viewport-only image previews. Original files remain render sources.

use loom_render::Bitmap;
use std::collections::{BTreeSet, HashMap};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc,
};

pub const DEFAULT_MAX_SIDE: u32 = 2048;
pub const DEFAULT_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// Worst-case square RGBA8 previews share this total visible-texture budget.
/// Zero means the budget cannot hold even one pixel per visible image.
pub fn preview_side(requested_side: u32, budget_bytes: u64, visible_count: usize) -> u32 {
    let pixels = budget_bytes / 4 / visible_count.max(1) as u64;
    let mut side = requested_side.min((pixels as f64).sqrt().floor().min(u32::MAX as f64) as u32);
    while u64::from(side) * u64::from(side) > pixels {
        side -= 1;
    }
    side
}

enum Entry {
    Pending(Arc<AtomicBool>),
    Ready(Arc<Bitmap>),
    Error(String),
}
impl Entry {
    fn cancel(&self) {
        if let Self::Pending(active) = self {
            active.store(false, Ordering::Relaxed);
        }
    }
}
struct Job {
    path: String,
    max_side: u32,
    epoch: u64,
    active: Arc<AtomicBool>,
}
struct Decoded {
    path: String,
    epoch: u64,
    active: Arc<AtomicBool>,
    result: Result<Arc<Bitmap>, String>,
}

/// One bounded decode worker; requests and completion polling never wait for image IO.
/// A policy change replaces the viewport tier, leaving original files untouched.
pub struct PreviewCache {
    jobs: mpsc::SyncSender<Job>,
    decoded: mpsc::Receiver<Decoded>,
    epoch: Arc<AtomicU64>,
    policy: Option<(u32, u64)>,
    entries: HashMap<String, Entry>,
}
impl Default for PreviewCache {
    fn default() -> Self {
        let (jobs, pending) = mpsc::sync_channel::<Job>(8);
        let (completed, decoded) = mpsc::channel();
        let epoch = Arc::new(AtomicU64::new(1));
        let worker_epoch = epoch.clone();
        std::thread::Builder::new()
            .name("loom-preview-decode".into())
            .spawn(move || {
                while let Ok(job) = pending.recv() {
                    let valid = || {
                        job.active.load(Ordering::Relaxed)
                            && worker_epoch.load(Ordering::Relaxed) == job.epoch
                    };
                    if !valid() {
                        continue;
                    }
                    let result = image::open(&job.path)
                        .map(|image| {
                            let image =
                                if image.width() > job.max_side || image.height() > job.max_side {
                                    image.thumbnail(job.max_side, job.max_side)
                                } else {
                                    image
                                };
                            let rgba = image.into_rgba8();
                            Arc::new(Bitmap {
                                width: rgba.width(),
                                height: rgba.height(),
                                rgba: rgba.into_raw(),
                            })
                        })
                        .map_err(|error| format!("{}: {error}", job.path));
                    if valid()
                        && completed
                            .send(Decoded {
                                path: job.path,
                                epoch: job.epoch,
                                active: job.active,
                                result,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("start viewport image decoder");
        Self {
            jobs,
            decoded,
            epoch,
            policy: None,
            entries: HashMap::new(),
        }
    }
}
impl PreviewCache {
    /// Returns cached pixels, or schedules a decode. Queue saturation simply
    /// leaves the path unrequested so the next viewport rebuild retries it.
    pub fn request(&mut self, path: &str, max_side: u32, generation: u64) -> Option<Arc<Bitmap>> {
        if self.policy != Some((max_side, generation)) {
            self.clear();
            self.policy = Some((max_side, generation));
        }
        if max_side == 0 {
            return None;
        }
        if let Some(entry) = self.entries.get(path) {
            return match entry {
                Entry::Ready(bitmap) => Some(bitmap.clone()),
                _ => None,
            };
        }
        let active = Arc::new(AtomicBool::new(true));
        let job = Job {
            path: path.into(),
            max_side,
            epoch: self.epoch.load(Ordering::Relaxed),
            active: active.clone(),
        };
        match self.jobs.try_send(job) {
            Ok(()) => {
                self.entries.insert(path.into(), Entry::Pending(active));
            }
            Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.entries.insert(
                    path.into(),
                    Entry::Error("viewport image decoder stopped".into()),
                );
            }
        }
        None
    }

    /// True only when a currently requested image completed (including errors).
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(decoded) = self.decoded.try_recv() {
            let current = decoded.epoch == self.epoch.load(Ordering::Relaxed)
                && self.entries.get(&decoded.path).is_some_and(|entry| matches!(entry, Entry::Pending(active) if Arc::ptr_eq(active, &decoded.active)));
            if !current {
                continue;
            }
            let entry = match decoded.result {
                Ok(bitmap) => Entry::Ready(bitmap),
                Err(error) => Entry::Error(error),
            };
            self.entries.insert(decoded.path, entry);
            changed = true;
        }
        changed
    }

    pub fn clear(&mut self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
        for entry in self.entries.values() {
            entry.cancel();
        }
        self.entries.clear();
        self.policy = None;
        // Discard ready stale images now, releasing their memory immediately.
        while self.decoded.try_recv().is_ok() {}
    }

    /// Evict hidden/out-of-scene images, including queued or in-flight requests.
    pub fn retain_paths(&mut self, paths: &BTreeSet<String>) {
        self.entries.retain(|path, entry| {
            let keep = paths.contains(path);
            if !keep {
                entry.cancel();
            }
            keep
        });
    }
    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| matches!(entry, Entry::Pending(_)))
            .count()
    }
    pub fn is_pending(&self, path: &str) -> bool {
        matches!(self.entries.get(path), Some(Entry::Pending(_)))
    }
    pub fn error(&self, path: &str) -> Option<&str> {
        match self.entries.get(path) {
            Some(Entry::Error(error)) => Some(error),
            _ => None,
        }
    }
}
impl Drop for PreviewCache {
    fn drop(&mut self) {
        // The detached worker observes cancellation, drains stale jobs, then
        // exits when its sender drops. Never join a decoder from the UI thread.
        self.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn image(name: &str, size: u32) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("loom-preview-{}-{name}.png", std::process::id()));
        image::RgbaImage::from_pixel(size, size, image::Rgba([20, 40, 60, 255]))
            .save(&path)
            .unwrap();
        path
    }
    fn wait(
        cache: &mut PreviewCache,
        path: &str,
        side: u32,
        generation: u64,
    ) -> std::sync::Arc<loom_render::Bitmap> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            cache.poll();
            if let Some(bitmap) = cache.request(path, side, generation) {
                return bitmap;
            }
            assert!(cache.error(path).is_none(), "{:?}", cache.error(path));
            assert!(Instant::now() < deadline, "preview decode timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn preview_dimensions_fit_the_aggregate_visible_budget() {
        assert_eq!(preview_side(2048, 16 * 1024 * 1024, 16), 512);
        assert_eq!(preview_side(2048, 256 * 1024 * 1024, 4), 2048);
        assert_eq!(preview_side(8192, 256 * 1024 * 1024, 28), 1548);
        assert_eq!(preview_side(2048, 0, 1), 0);
    }
    #[test]
    fn real_8k_png_decodes_in_worker_at_preview_size_without_changing_source() {
        let path = image("large", 8192);
        let source = std::fs::read(&path).unwrap();
        let path_str = path.to_str().unwrap();
        let mut cache = PreviewCache::default();
        assert!(cache.request(path_str, 1024, 0).is_none());
        assert!(cache.is_pending(path_str));
        assert_eq!(cache.pending_count(), 1);
        let bitmap = wait(&mut cache, path_str, 1024, 0);
        assert_eq!(
            (bitmap.width, bitmap.height, bitmap.rgba.len()),
            (1024, 1024, 4 * 1024 * 1024)
        );
        assert_eq!(&bitmap.rgba[..4], &[20, 40, 60, 255]);
        assert_eq!(cache.pending_count(), 0);
        assert!(std::sync::Arc::ptr_eq(
            &bitmap,
            &cache.request(path_str, 1024, 0).unwrap()
        ));
        assert_eq!(std::fs::read(&path).unwrap(), source);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn policy_change_discards_stale_preview_results() {
        let path = image("stale", 8192);
        let path_str = path.to_str().unwrap();
        let mut cache = PreviewCache::default();
        assert!(cache.request(path_str, 1024, 0).is_none());
        cache.clear();
        assert!(cache.request(path_str, 128, 1).is_none());
        let bitmap = wait(&mut cache, path_str, 128, 1);
        assert_eq!((bitmap.width, bitmap.height), (128, 128));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn decode_errors_are_reported_asynchronously_and_reload_retries() {
        let path =
            std::env::temp_dir().join(format!("loom-preview-missing-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().unwrap();
        let mut cache = PreviewCache::default();
        assert!(cache.request(path_str, 256, 0).is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while cache.error(path_str).is_none() {
            cache.poll();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        image::RgbaImage::from_pixel(16, 16, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .unwrap();
        cache.clear();
        let bitmap = wait(&mut cache, path_str, 256, 1);
        assert_eq!((bitmap.width, bitmap.height), (16, 16));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn retaining_visible_paths_cancels_hidden_jobs_and_releases_ready_images() {
        let old = image("evict-old", 32);
        let visible = image("evict-visible", 16);
        let mut cache = PreviewCache::default();
        assert!(cache.request(old.to_str().unwrap(), 256, 0).is_none());
        cache.retain_paths(&BTreeSet::new());
        wait(&mut cache, visible.to_str().unwrap(), 256, 0);
        cache.poll();
        assert!(
            cache.request(old.to_str().unwrap(), 256, 0).is_none(),
            "hidden completion must not repopulate the cache"
        );
        wait(&mut cache, old.to_str().unwrap(), 256, 0);
        cache.retain_paths(&[visible.to_string_lossy().into_owned()].into());
        assert!(
            cache.request(old.to_str().unwrap(), 256, 0).is_none(),
            "hidden ready image must be evicted"
        );
        std::fs::remove_file(old).unwrap();
        std::fs::remove_file(visible).unwrap();
    }
}
