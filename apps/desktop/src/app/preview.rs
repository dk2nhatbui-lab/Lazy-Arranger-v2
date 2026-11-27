//! One-note previews: a piano roll key, a newly added note, a drum hit.
//!
//! A preview plays on the track's own sound through the real-time engine.
//! The app keeps one preview stream open on the chosen output device and
//! loads each new note into it, so a click is heard at the next buffer
//! instead of after a device open.

use super::*;

/// Length of a piano roll or score preview note.
pub(super) const NOTE_PREVIEW: Tick = PPQ / 2;
/// Length of a drum hit preview.
pub(super) const DRUM_PREVIEW: Tick = PPQ / 4;

/// A one-track song that plays `pitch` once at the start on `source`'s
/// sound: its instrument, SoundFont, program, plug-ins, volume and pan. Its
/// notes, parts, audio and automation are left out, and it is never muted.
/// With no source the preview plays the default piano.
#[must_use]
pub(super) fn preview_song(
    source: Option<&project::song::Track>,
    pitch: u8,
    velocity: u8,
    duration: Tick,
    fallback: music_core::Channel,
) -> Song {
    let mut track = source
        .cloned()
        .unwrap_or_else(|| project::song::Track::new(TrackId::new(1).expect("valid"), "Preview"));
    // Drum tracks keep their notes on channel 10: play on the channel the
    // track's notes use, else on `fallback`.
    let channel = track.notes.first().map_or(fallback, |note| note.channel);
    track.notes.clear();
    track.clips.clear();
    track.slots.clear();
    track.automation.clear();
    track.audio = None;
    track.takes.clear();
    track.comps.clear();
    track.muted = false;
    track.soloed = false;
    // The preview song has no buses, so the track goes straight to master.
    track.bus = None;
    track.send = None;
    if let (Ok(pitch), Ok(velocity), Ok(id)) = (
        music_core::Pitch::new(pitch),
        music_core::Velocity::new(velocity.clamp(1, 127)),
        NoteId::new(1),
    ) {
        track.notes.push(project::song::Note {
            id,
            start: 0,
            duration: duration.max(1),
            pitch,
            velocity,
            channel,
        });
    }
    let mut song = Song::new("Preview", 120.0);
    song.tracks.push(track);
    song
}

impl ShellApp {
    /// Plays `pitch` on the sound of `track`, or of the piano roll's track.
    pub(super) fn preview_note_on(&mut self, track: Option<TrackId>, pitch: u8) {
        let id = track.or(self.roll.selected);
        let source = id.and_then(|id| self.song().tracks.iter().find(|track| track.id == id));
        let song = preview_song(
            source,
            pitch,
            100,
            NOTE_PREVIEW,
            music_core::Channel::first(),
        );
        self.play_preview(&song);
    }

    /// Starts a preview song from its first sample on the preview stream.
    pub(super) fn play_preview(&mut self, song: &Song) {
        let output = self.settings.audio_output.clone();
        let master = self.settings.master();
        // Reuse the open stream while it is on the chosen device. A load
        // queues the new graph; the mixer, seek and play land with it at the
        // next buffer. The first mix frees the graphs the engine handed back.
        if let Some((player, device)) = &self.preview_player
            && *device == output
        {
            player.apply_mix(song, master);
            if player
                .load(audio::live::render(song, audio::SAMPLE_RATE))
                .is_ok()
            {
                player.apply_mix(song, master);
                let _ = player.seek_seconds(0.0);
                player.play();
                return;
            }
            // The engine is still freeing earlier graphs: open afresh.
        }
        let render = audio::live::render(song, audio::SAMPLE_RATE);
        match audio::player::Player::new_live_on(render, output.as_deref(), None) {
            Ok(player) => {
                player.apply_mix(song, master);
                player.play();
                self.preview_player = Some((player, output));
            }
            Err(error) => self.status = Some(format!("preview: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    }

    fn organ_track() -> project::song::Track {
        let mut track = project::song::Track::new(TrackId::new(3).expect("valid"), "Organ");
        track.program = Some(19);
        track.muted = true;
        track.soloed = true;
        track.bus = Some(1);
        track.notes.push(project::song::Note {
            id: NoteId::new(1).expect("valid"),
            start: PPQ * 8,
            duration: PPQ,
            pitch: music_core::Pitch::new(40).expect("valid"),
            velocity: music_core::Velocity::new(90).expect("valid"),
            channel: music_core::Channel::new(2).expect("valid"),
        });
        track
    }

    #[test]
    fn a_preview_keeps_the_sound_and_plays_one_note_at_once() {
        let song = preview_song(
            Some(&organ_track()),
            64,
            100,
            NOTE_PREVIEW,
            music_core::Channel::first(),
        );
        let track = &song.tracks[0];
        assert_eq!(track.id, TrackId::new(3).expect("valid"));
        assert_eq!(track.program, Some(19), "the track's instrument");
        assert!(!track.muted && !track.soloed && track.bus.is_none());
        assert_eq!(track.notes.len(), 1);
        let note = &track.notes[0];
        assert_eq!((note.start, note.duration), (0, NOTE_PREVIEW));
        assert_eq!(note.pitch.as_u8(), 64);
        assert_eq!(
            note.channel,
            music_core::Channel::new(2).expect("valid"),
            "the track's own channel"
        );
    }

    #[test]
    fn an_empty_drum_track_previews_on_the_fallback_channel() {
        let drums = music_core::Channel::new(9).expect("valid");
        let song = preview_song(None, 36, 100, DRUM_PREVIEW, drums);
        assert_eq!(song.tracks[0].notes[0].channel, drums);
    }

    #[test]
    fn a_muted_tracks_preview_is_heard() {
        let song = preview_song(
            Some(&organ_track()),
            60,
            100,
            NOTE_PREVIEW,
            music_core::Channel::first(),
        );
        let audio = audio::live::render(&song, 8_000).mixdown();
        assert!(peak(&audio.samples) > 0.01, "the preview is silent");
    }

    #[test]
    fn a_second_preview_on_the_same_stream_plays_from_the_start() {
        let first = preview_song(None, 60, 100, NOTE_PREVIEW, music_core::Channel::first());
        let master = audio::master::Master::default();
        let mut playback = audio::player::Playback::new_live(audio::live::render(&first, 8_000));
        playback.apply_mix(&first, master);
        playback.play();
        let mut buffer = vec![0.0_f32; 512];
        // Run the first note out, past its release.
        for _ in 0..200 {
            playback.fill(&mut buffer);
        }
        // What `play_preview` does on a reused stream.
        let second = preview_song(None, 67, 100, NOTE_PREVIEW, music_core::Channel::first());
        playback.apply_mix(&second, master);
        playback
            .load(audio::live::render(&second, 8_000))
            .expect("the engine takes the graph");
        playback.apply_mix(&second, master);
        playback.seek_seconds(0.0);
        playback.play();
        let mut heard = 0.0_f32;
        for _ in 0..8 {
            playback.fill(&mut buffer);
            heard = heard.max(peak(&buffer));
        }
        assert!(heard > 0.01, "the second key is silent");
    }
}
