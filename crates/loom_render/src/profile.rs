//! Optional inclusive wall-time measurements for actual application frames.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("LOOM_PROFILE_FRAMES").is_some())
}

#[derive(Default)]
struct Samples {
    count: u64,
    total: Duration,
    max: Duration,
}

struct Profile {
    started: Instant,
    samples: BTreeMap<&'static str, Samples>,
}

fn profile() -> &'static Mutex<Profile> {
    static PROFILE: OnceLock<Mutex<Profile>> = OnceLock::new();
    PROFILE.get_or_init(|| Mutex::new(Profile { started: Instant::now(), samples: BTreeMap::new() }))
}

/// Measures an inclusive scope only when `LOOM_PROFILE_FRAMES` is set.
/// Disabled scopes read the cached flag without locking or reading the clock.
pub struct Scope(Option<(&'static str, Instant)>);

impl Scope {
    pub fn new(name: &'static str) -> Self {
        Self(if enabled() { Some((name, Instant::now())) } else { None })
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let Some((name, started)) = self.0 else { return };
        let elapsed = started.elapsed();
        if let Ok(mut profile) = profile().lock() {
            let sample = profile.samples.entry(name).or_default();
            sample.count += 1;
            sample.total += elapsed;
            sample.max = sample.max.max(elapsed);
        }
    }
}

/// Prints each scope's count, average, maximum and total every five seconds.
/// Totals are inclusive; parallel scopes can sum to more than elapsed wall time.
pub fn report_if_due() {
    if !enabled() { return; }
    let Ok(mut profile) = profile().lock() else { return };
    if profile.started.elapsed() < Duration::from_secs(5) { return; }
    let samples = std::mem::take(&mut profile.samples);
    profile.started = Instant::now();
    drop(profile);
    for (name, sample) in samples {
        let total_ms = sample.total.as_secs_f64() * 1000.0;
        eprintln!("[loom_profile] {name} count={} avg_ms={:.3} max_ms={:.3} total_ms={total_ms:.3}", sample.count, total_ms / sample.count as f64, sample.max.as_secs_f64() * 1000.0);
    }
}
