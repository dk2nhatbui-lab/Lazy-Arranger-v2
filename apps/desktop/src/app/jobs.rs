//! Slow work (rendering, ffmpeg, printing, transcription) runs on worker
//! threads so the window keeps drawing. Each job hands back a closure that
//! applies its result on the UI thread.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use super::ShellApp;

type Finish = Box<dyn FnOnce(&mut ShellApp) + Send>;

struct Job {
    label: String,
    started: Instant,
    receiver: Receiver<Finish>,
}

/// The jobs still running, in start order.
#[derive(Default)]
pub(crate) struct Jobs {
    running: Vec<Job>,
}

impl Jobs {
    /// Runs `work` on a worker thread, then `finish` on the UI thread with its
    /// result. `label` shows in the status bar while it runs, like
    /// "Exporting WAV".
    pub(crate) fn spawn<T, W, F>(&mut self, label: impl Into<String>, work: W, finish: F)
    where
        T: Send + 'static,
        W: FnOnce() -> T + Send + 'static,
        F: FnOnce(&mut ShellApp, T) + Send + 'static,
    {
        let label = label.into();
        let (sender, receiver) = channel::<Finish>();
        let spawned = std::thread::Builder::new()
            .name(format!("job: {label}"))
            .spawn(move || {
                let result = work();
                let _ = sender.send(Box::new(move |app: &mut ShellApp| finish(app, result)));
            });
        if spawned.is_ok() {
            self.running.push(Job {
                label,
                started: Instant::now(),
                receiver,
            });
        }
    }

    /// True while a job with this label runs, so a second click can wait.
    pub(crate) fn is_running(&self, label: &str) -> bool {
        self.running.iter().any(|job| job.label == label)
    }

    /// True while any job runs.
    pub(crate) fn busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// What the status bar shows, such as "Exporting WAV… 4 s".
    pub(crate) fn progress_note(&self) -> Option<String> {
        let first = self.running.first()?;
        let seconds = first.started.elapsed().as_secs();
        let more = self.running.len() - 1;
        let mut note = format!("{}\u{2026} {seconds} s", first.label);
        if more > 0 {
            note.push_str(&format!(" (+{more} more)"));
        }
        Some(note)
    }

    /// Removes finished jobs: their result closures, and labels of jobs
    /// whose worker died without one.
    fn take_finished(&mut self) -> (Vec<Finish>, Vec<String>) {
        let mut done = Vec::new();
        let mut failed = Vec::new();
        self.running.retain(|job| match job.receiver.try_recv() {
            Ok(finish) => {
                done.push(finish);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                failed.push(job.label.clone());
                false
            }
        });
        (done, failed)
    }
}

impl ShellApp {
    /// Applies finished job results and keeps the frame ticking while jobs
    /// run, so the progress note counts up.
    pub(super) fn poll_jobs(&mut self, context: &egui::Context) {
        let (done, failed) = self.jobs.take_finished();
        for finish in done {
            finish(self);
        }
        if let Some(label) = failed.first() {
            self.load_error = Some(format!("{label} stopped unexpectedly"));
        }
        if self.jobs.busy() {
            context.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Starts a job unless one with the same label already runs; returns
    /// false (and says so) when it was already running.
    pub(super) fn start_job<T, W, F>(&mut self, label: &str, work: W, finish: F) -> bool
    where
        T: Send + 'static,
        W: FnOnce() -> T + Send + 'static,
        F: FnOnce(&mut ShellApp, T) + Send + 'static,
    {
        if self.jobs.is_running(label) {
            self.status = Some(format!("{label} is still running"));
            return false;
        }
        self.jobs.spawn(label, work, finish);
        true
    }
}

/// Which player an audition replaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuditionSlot {
    /// The style preview, which the section queue follows.
    Style,
    /// The general audition player (grooves, proposals, takes).
    Main,
}

/// The label of the render job for a slot, so a second click waits.
pub(crate) const fn audition_label(slot: AuditionSlot) -> &'static str {
    match slot {
        AuditionSlot::Style => "Rendering style",
        AuditionSlot::Main => "Rendering audition",
    }
}

impl ShellApp {
    /// Renders `song` off the UI thread, then plays it in `slot` and shows
    /// `message`. The audio device is opened on the UI thread.
    pub(super) fn audition_in_background(
        &mut self,
        song: project::Song,
        slot: AuditionSlot,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.start_job(
            audition_label(slot),
            move || audio::live::render(&song, audio::SAMPLE_RATE),
            move |app, rendered| match audio::player::Player::new_live(rendered) {
                Ok(player) => {
                    player.play();
                    match slot {
                        AuditionSlot::Style => app.style.preview = Some(player),
                        AuditionSlot::Main => app.audition_player = Some(player),
                    }
                    app.status = Some(message);
                }
                Err(error) => app.status = Some(error.to_string()),
            },
        );
    }
}
