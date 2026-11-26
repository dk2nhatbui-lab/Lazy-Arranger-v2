//! Keeps a slow or stuck plug-in from taking playback down with it.
//!
//! Live plug-ins run in the audio callback. Two rules protect playback
//! ("late is not dead"):
//!
//! - **Slow.** A plug-in that takes longer than its block's real-time length,
//!   block after block, is dropped: the chain passes its audio through dry
//!   from then on, and the app is told. One late block is not enough; a
//!   short spike (a preset load, a page fault) is forgiven.
//! - **Stuck.** A plug-in still inside one block after [`STALL`] has hung
//!   the audio thread. The callback cannot be freed from outside, so the app
//!   asks [`stalled`] each frame, abandons that player and starts playback
//!   again without the plug-in.
//!
//! A dropped or stuck plug-in is blocked for the rest of the session:
//! [`crate::inserts::Inserts`] skips it when it builds a chain.
//!
//! The audio thread only touches atomics here. The registry and the block
//! list are locked on control threads only.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use project::song::PluginSlot;

/// How long one block may run before the plug-in counts as stuck.
pub const STALL: Duration = Duration::from_secs(2);

/// Late blocks in a row, net of on-time ones, before a slow plug-in is
/// dropped. At 32 frames and 48 kHz this is about a third of a second of
/// audio that could not keep up.
const STRIKES: u32 = 500;

/// The key a plug-in is blocked by: its path and identifier.
#[must_use]
pub fn key(slot: &PluginSlot) -> String {
    format!("{}|{}", slot.path, slot.identifier)
}

/// A readable name for a blocked key.
#[must_use]
pub fn name(key: &str) -> String {
    let path = key.split('|').next().unwrap_or(key);
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(key)
        .to_string()
}

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Nanoseconds since the epoch, never 0.
fn now() -> u64 {
    u64::try_from(epoch().elapsed().as_nanos())
        .unwrap_or(u64::MAX)
        .max(1)
}

/// What the audio thread reports about one plug-in instance.
#[derive(Debug, Default)]
pub(crate) struct Beat {
    /// When the current block started, or 0 between blocks.
    since: AtomicU64,
    /// Set once the plug-in was dropped for being slow.
    dropped: AtomicBool,
}

#[derive(Debug)]
struct Entry {
    key: String,
    beat: Arc<Beat>,
}

fn registry() -> &'static Mutex<Vec<Entry>> {
    static REGISTRY: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn blocked_list() -> &'static Mutex<Vec<String>> {
    static BLOCKED: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    BLOCKED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Blocks a plug-in for the rest of the session.
pub fn block(key: &str) {
    if let Ok(mut list) = blocked_list().lock()
        && !list.iter().any(|entry| entry == key)
    {
        list.push(key.to_string());
    }
}

/// True when the slot's plug-in was dropped or hung earlier.
#[must_use]
pub fn is_blocked(slot: &PluginSlot) -> bool {
    let key = key(slot);
    blocked_list()
        .lock()
        .is_ok_and(|list| list.contains(&key))
}

/// Every blocked plug-in key.
#[must_use]
pub fn blocked() -> Vec<String> {
    blocked_list()
        .lock()
        .map(|list| list.clone())
        .unwrap_or_default()
}

/// Lets blocked plug-ins run again, for a user who wants to retry.
pub fn unblock_all() {
    if let Ok(mut list) = blocked_list().lock() {
        list.clear();
    }
}

/// A plug-in that hung or was dropped since the last call, by key. A hung
/// plug-in's player must be abandoned; a dropped one already plays dry.
/// Each one is reported once and blocked.
#[must_use]
pub fn stalled() -> Option<Stall> {
    stalled_after(STALL)
}

fn stalled_after(stall: Duration) -> Option<Stall> {
    let mut registry = registry().lock().ok()?;
    let now = now();
    let limit = u64::try_from(stall.as_nanos()).unwrap_or(u64::MAX);
    let mut found = None;
    registry.retain(|entry| {
        // Dropped from the chain since: nothing to watch.
        if Arc::strong_count(&entry.beat) == 1 {
            return false;
        }
        if found.is_some() {
            return true;
        }
        let since = entry.beat.since.load(Ordering::Acquire);
        if since != 0 && now.saturating_sub(since) > limit {
            found = Some(Stall {
                key: entry.key.clone(),
                hung: true,
            });
            return false;
        }
        if entry.beat.dropped.load(Ordering::Acquire) {
            found = Some(Stall {
                key: entry.key.clone(),
                hung: false,
            });
            return false;
        }
        true
    });
    drop(registry);
    if let Some(stall) = &found {
        block(&stall.key);
    }
    found
}

/// A plug-in that stopped keeping up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stall {
    /// The plug-in's key; see [`name`].
    pub key: String,
    /// True when it hung the audio thread; false when it was only too slow
    /// and now plays dry.
    pub hung: bool,
}

/// Watches one plug-in instance from the audio thread.
#[derive(Debug)]
pub(crate) struct Watch {
    beat: Arc<Beat>,
    /// Nanoseconds of audio per frame.
    frame_nanos: u64,
    strikes: u32,
    /// False while rendering faster than real time (an export), where a
    /// slow plug-in is fine.
    pub(crate) realtime: bool,
}

impl Watch {
    /// Registers a plug-in instance. Call on a control thread.
    pub(crate) fn new(slot: &PluginSlot, sample_rate: u32) -> Self {
        let _ = epoch();
        let beat = Arc::new(Beat::default());
        if let Ok(mut registry) = registry().lock() {
            registry.push(Entry {
                key: key(slot),
                beat: Arc::clone(&beat),
            });
        }
        Self {
            beat,
            frame_nanos: 1_000_000_000 / u64::from(sample_rate.max(1)),
            strikes: 0,
            realtime: true,
        }
    }

    /// True once the plug-in was dropped; the chain skips it.
    pub(crate) fn dropped(&self) -> bool {
        self.beat.dropped.load(Ordering::Relaxed)
    }

    /// Runs one block of `frames` frames through `process`, timing it.
    pub(crate) fn run<T>(&mut self, frames: usize, process: impl FnOnce() -> T) -> T {
        if !self.realtime {
            return process();
        }
        let start = now();
        self.beat.since.store(start, Ordering::Release);
        let result = process();
        let end = now();
        self.beat.since.store(0, Ordering::Release);
        {
            let budget = self.frame_nanos.saturating_mul(frames as u64);
            if end.saturating_sub(start) > budget {
                self.strikes += 1;
                if self.strikes >= STRIKES {
                    self.beat.dropped.store(true, Ordering::Release);
                }
            } else {
                self.strikes = self.strikes.saturating_sub(1);
            }
        }
        result
    }

    /// Leaves the plug-in "inside" a block, as a hung one would be.
    #[cfg(test)]
    fn hang(&self) {
        self.beat.since.store(now(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(path: &str) -> PluginSlot {
        PluginSlot {
            id: 1,
            format: project::song::PluginFormat::Vst3,
            name: String::from("Slow"),
            path: path.to_string(),
            identifier: String::from("0123"),
            bypassed: false,
            state: String::new(),
        }
    }

    #[test]
    fn a_slow_plugin_is_dropped_but_a_spike_is_forgiven() {
        let slot = slot("/watchdog/Slow.vst3");
        let mut watch = Watch::new(&slot, 48_000);
        // A few late blocks among on-time ones are forgiven.
        for _ in 0..10 {
            watch.run(32, || std::thread::sleep(Duration::from_millis(1)));
            watch.run(32, || ());
        }
        assert!(!watch.dropped());
        // Late block after late block is not.
        for _ in 0..STRIKES {
            watch.run(1, || std::thread::sleep(Duration::from_micros(100)));
        }
        assert!(watch.dropped());
        let stall = std::iter::from_fn(stalled)
            .find(|stall| stall.key == key(&slot))
            .expect("reported");
        assert!(!stall.hung);
        assert!(is_blocked(&slot));
        assert_eq!(name(&stall.key), "Slow");
    }

    #[test]
    fn an_export_never_drops_a_slow_plugin() {
        let slot = slot("/watchdog/Export.vst3");
        let mut watch = Watch::new(&slot, 48_000);
        watch.realtime = false;
        for _ in 0..STRIKES {
            watch.run(1, || std::thread::sleep(Duration::from_micros(50)));
        }
        assert!(!watch.dropped());
    }

    #[test]
    fn a_plugin_stuck_in_a_block_is_reported_as_hung() {
        let slot = slot("/watchdog/Stuck.vst3");
        let watch = Watch::new(&slot, 48_000);
        watch.hang();
        std::thread::sleep(Duration::from_millis(30));
        let stall = std::iter::from_fn(|| stalled_after(Duration::from_millis(20)))
            .find(|stall| stall.key == key(&slot))
            .expect("reported");
        assert!(stall.hung);
        assert!(is_blocked(&slot));
        drop(watch);
    }
}
