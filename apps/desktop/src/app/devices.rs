//! Device helpers: SysEx text, L6 mixer scenes, PSR registrations and MIDI
//! device profiles, plus the MIDI, PSR, XG, L6 and D05 parts of
//! [`ShellApp`].

use super::*;

/// One row of the track list, snapshotted before drawing so the borrow ends.
#[derive(Clone, Debug, Default)]
pub(super) struct L6Scene {
    pub(super) channels: Vec<(u8, u8, bool, u8)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PsrRegistration {
    pub(super) channel: u8,
    pub(super) bank_msb: u8,
    pub(super) bank_lsb: u8,
    pub(super) program: u8,
    pub(super) volume: u8,
    pub(super) pan: u8,
    pub(super) reverb: u8,
    pub(super) chorus: u8,
}

impl PsrRegistration {
    pub(super) fn encode(self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.channel,
            self.bank_msb,
            self.bank_lsb,
            self.program,
            self.volume,
            self.pan,
            self.reverb,
            self.chorus
        )
    }

    pub(super) fn decode(line: &str) -> Option<Self> {
        let mut parts = line.trim().split(',');
        let mut next = || parts.next()?.trim().parse::<u8>().ok();
        let registration = Self {
            channel: next()?,
            bank_msb: next()?,
            bank_lsb: next()?,
            program: next()?,
            volume: next()?,
            pan: next()?,
            reverb: next()?,
            chorus: next()?,
        };
        (parts.next().is_none()).then_some(registration)
    }
}

pub(super) fn psr_regs_path() -> std::path::PathBuf {
    std::path::PathBuf::from("lazy-arranger-psr.txt")
}

pub(super) fn load_psr_regs() -> [PsrRegistration; 8] {
    let mut regs = [PsrRegistration::default(); 8];
    let Ok(text) = std::fs::read_to_string(psr_regs_path()) else {
        return regs;
    };
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .enumerate()
    {
        if let Some(reg) = PsrRegistration::decode(line) {
            regs[index] = reg;
        }
    }
    regs
}

pub(super) fn save_psr_regs(regs: &[PsrRegistration; 8]) {
    let text: String = regs
        .iter()
        .map(|reg| reg.encode())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(psr_regs_path(), format!("{text}\n"));
}

/// Formats bytes as space-separated uppercase hex.
#[must_use]
pub fn sysex_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_sysex(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for token in text.split_whitespace() {
        let token = token.trim_start_matches("0x").trim_start_matches("0X");
        let byte =
            u8::from_str_radix(token, 16).map_err(|_| format!("not a hex byte: {token:?}"))?;
        bytes.push(byte);
    }
    if bytes.is_empty() {
        return Err(String::from("enter at least one byte"));
    }
    if bytes.first() != Some(&0xF0) {
        bytes.insert(0, 0xF0);
    }
    if bytes.last() != Some(&0xF7) {
        bytes.push(0xF7);
    }
    Ok(bytes)
}

pub(super) fn profile_for_port(
    port: &str,
    profiles: &[midi_devices::profiles::LoadedProfile],
) -> Option<String> {
    let normalize = |text: &str| -> String {
        text.chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect()
    };
    let port = normalize(port);
    profiles
        .iter()
        .find(|profile| {
            let id = normalize(&profile.id);
            let name = normalize(&profile.name);
            (!id.is_empty() && port.contains(&id))
                || (!name.is_empty() && (port.contains(&name) || name.contains(&port)))
        })
        .map(|profile| profile.id.clone())
}

pub(super) fn l6_scene_path() -> std::path::PathBuf {
    crate::settings::Settings::data_dir().join("lazy-arranger-l6-scenes.txt")
}

pub(super) fn encode_l6_scenes(scenes: &[Option<L6Scene>; 4]) -> String {
    let mut text = String::new();
    for (index, scene) in scenes.iter().enumerate() {
        let Some(scene) = scene else {
            continue;
        };
        let values: Vec<String> = scene
            .channels
            .iter()
            .map(|(volume, pan, muted, send)| format!("{volume},{pan},{},{send}", u8::from(*muted)))
            .collect();
        text.push_str(&format!("{index}:{}\n", values.join("|")));
    }
    text
}

pub(super) fn decode_l6_scenes(text: &str) -> [Option<L6Scene>; 4] {
    let mut scenes: [Option<L6Scene>; 4] = [None, None, None, None];
    for line in text.lines() {
        let Some((index, values)) = line.split_once(':') else {
            continue;
        };
        let Ok(index) = index.parse::<usize>() else {
            continue;
        };
        if index >= 4 {
            continue;
        }
        let mut channels = Vec::new();
        for value in values.split('|').filter(|value| !value.is_empty()) {
            let mut parts = value.split(',');
            let (Some(volume), Some(pan), Some(muted), Some(send)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let (Ok(volume), Ok(pan), Ok(send)) =
                (volume.parse::<u8>(), pan.parse::<u8>(), send.parse::<u8>())
            {
                channels.push((volume.min(127), pan.min(127), muted == "1", send.min(127)));
            }
        }
        scenes[index] = Some(L6Scene { channels });
    }
    scenes
}

pub(super) fn load_l6_scenes() -> [Option<L6Scene>; 4] {
    std::fs::read_to_string(l6_scene_path())
        .map(|text| decode_l6_scenes(&text))
        .unwrap_or([None, None, None, None])
}

pub(super) fn save_l6_scenes(scenes: &[Option<L6Scene>; 4]) {
    let _ = std::fs::write(l6_scene_path(), encode_l6_scenes(scenes));
}

pub(super) fn seed_default_device_profiles(folder: &std::path::Path) {
    if std::fs::create_dir_all(folder).is_err() {
        return;
    }
    for (name, body) in [
        (
            "xiosynth.yaml",
            include_str!("../../assets/profiles/xiosynth.yaml"),
        ),
        ("d05.yaml", include_str!("../../assets/profiles/d05.yaml")),
        (
            "generic-gs.yaml",
            include_str!("../../assets/profiles/generic-gs.yaml"),
        ),
        (
            "generic-xg.yaml",
            include_str!("../../assets/profiles/generic-xg.yaml"),
        ),
        (
            "korg-pa.yaml",
            include_str!("../../assets/profiles/korg-pa.yaml"),
        ),
        (
            "psr-s970.yaml",
            include_str!("../../assets/profiles/psr-s970.yaml"),
        ),
    ] {
        let path = folder.join(name);
        if !path.exists() {
            let _ = std::fs::write(path, body);
        }
    }
}

pub(super) fn load_device_profiles() -> Vec<midi_devices::profiles::LoadedProfile> {
    let folder = crate::settings::Settings::path()
        .parent()
        .map(|path| path.join("profiles"))
        .unwrap_or_else(|| std::path::PathBuf::from("profiles"));
    seed_default_device_profiles(&folder);
    midi_devices::profiles::load_directory(&folder)
        .map(|(profiles, _)| profiles)
        .unwrap_or_default()
}

/// The PSR window: the part being edited and the eight registrations.
pub(super) struct PsrState {
    pub(super) open: bool,
    pub(super) channel: u8,
    pub(super) bank_msb: u8,
    pub(super) bank_lsb: u8,
    pub(super) program: u8,
    pub(super) volume: u8,
    pub(super) pan: u8,
    pub(super) reverb: u8,
    pub(super) chorus: u8,
    pub(super) regs: [PsrRegistration; 8],
}

/// The XG window: part settings for the 16 channels.
pub(super) struct XgState {
    pub(super) open: bool,
    pub(super) parts: [[u8; 4]; 16],
}

/// The L6 Max import, mixer and device windows.
pub(super) struct L6MaxState {
    pub(super) device_open: bool,
    pub(super) device: crate::l6max_device::DeviceState,
    pub(super) identity: String,
    pub(super) id_reply: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    pub(super) folder: String,
    pub(super) plan: Option<audio::l6max::Plan>,
    pub(super) error: Option<String>,
    pub(super) selected: Vec<bool>,
    pub(super) mixer_open: bool,
    pub(super) compact: bool,
    pub(super) scenes: [Option<L6Scene>; 4],
}

/// The D-05 editor window and its pending device reply.
pub(super) struct D05State {
    pub(super) open: bool,
    pub(super) editor: crate::d05_editor::D05EditorState,
    pub(super) reply: Option<std::sync::mpsc::Receiver<D05TaskResult>>,
}

/// Chord detection from an audio file.
pub(super) struct AudioChordsState {
    pub(super) open: bool,
    pub(super) path: String,
    pub(super) chords: Vec<audio::chords::AudioChord>,
    pub(super) pending:
        Option<std::sync::mpsc::Receiver<Result<Vec<audio::chords::AudioChord>, String>>>,
    pub(super) auto: bool,
}

/// The MIDI output window: port, connection and the message being built.
pub(super) struct MidiOutState {
    pub(super) open: bool,
    pub(super) port: Option<String>,
    pub(super) conn: Option<(
        String,
        std::sync::Arc<std::sync::Mutex<midi_devices::devices::MidiOutputConnection>>,
    )>,
    pub(super) channel: u8,
    pub(super) program: u8,
    pub(super) note: u8,
    pub(super) velocity: u8,
    pub(super) cc: u8,
    pub(super) cc_value: u8,
    pub(super) bank_msb: u8,
    pub(super) bank_lsb: u8,
}

impl ShellApp {
    /// Opens the configured keyboard once and applies mapped controls.
    pub(super) fn poll_midi(&mut self) {
        if self.settings.device_profile.is_none()
            && let Some(output) = self.settings.midi_output.clone()
            && let Some(profile) = profile_for_port(&output, &self.device_profiles)
        {
            self.status = Some(format!("device profile {profile} from {output}"));
            self.settings.device_profile = Some(profile);
            let _ = self.settings.save(&crate::settings::Settings::path());
        }
        if self.midi_input.is_none() {
            if self.settings.auto_inputs {
                match midi_devices::devices::LiveInputs::open_all() {
                    Ok(inputs) => {
                        self.status =
                            Some(format!("listening to {} MIDI input(s)", inputs.count()));
                        self.midi_input = Some(inputs);
                    }
                    Err(error) => self.status = Some(format!("MIDI inputs: {error}")),
                }
            } else if let Some(name) = self.settings.keyboard.clone() {
                match midi_devices::devices::LiveInputs::open_named(&name) {
                    Ok(input) => {
                        self.midi_input = Some(input);
                        if self.settings.device_profile.is_none()
                            && let Some(profile) = profile_for_port(&name, &self.device_profiles)
                        {
                            self.status = Some(format!("device profile {profile} from {name}"));
                            self.settings.device_profile = Some(profile);
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                    }
                    Err(error) => {
                        self.status = Some(format!("MIDI input {name}: {error}"));
                        self.settings.keyboard = None;
                    }
                }
            }
        }
        let mut actions = Vec::new();
        let mut learned = None;
        let mut midi_activity = false;
        let mut detected_chord = None;
        let mut preview_keys = Vec::new();
        let mut score_note_ons = Vec::new();
        let mut score_chord_done = false;
        if let Some(input) = &self.midi_input {
            while let Some(message) = input.next() {
                midi_activity = true;
                match &message {
                    music_core::MidiMessage::NoteOn { key, velocity, .. }
                        if velocity.as_u8() > 0 =>
                    {
                        self.style.held[usize::from(key.as_u8())] = true;
                        if self.settings.kids_mode && self.settings.kids_keys_play {
                            preview_keys.push(key.as_u8());
                        }
                        if self.view == View::Score && self.score.entry {
                            score_note_ons.push(key.as_u8());
                        }
                    }
                    music_core::MidiMessage::NoteOff { key, .. }
                    | music_core::MidiMessage::NoteOn { key, .. } => {
                        self.style.held[usize::from(key.as_u8())] = false;
                        if self.view == View::Score
                            && self.score.entry
                            && !self.style.held.iter().any(|held| *held)
                        {
                            score_chord_done = true;
                        }
                    }
                    _ => {}
                }
                self.midi_chord = held_chord_name(&self.style.held);
                if self.style.file_open
                    && let Some(name) = &self.midi_chord
                    && *name != self.style.last_chord
                {
                    detected_chord = Some(name.clone());
                }
                let label = midi_label(&message);
                self.midi_recent.push_front(label.clone());
                self.midi_recent.truncate(16);
                self.midi_log.push_front((std::time::Instant::now(), label));
                while self
                    .midi_log
                    .back()
                    .is_some_and(|(at, _)| at.elapsed().as_secs() > 120)
                {
                    self.midi_log.pop_back();
                }
                if let music_core::MidiMessage::ControlChange {
                    controller, value, ..
                } = &message
                {
                    self.midi_cc_values[usize::from(controller.as_u8())] = *value;
                }
                if let Some(learn) = self.midi_learn {
                    if let Some((control, action)) = learn.capture(&message) {
                        learned = Some((control, action));
                    }
                    continue;
                }
                if let Some(action) = self.midi_map.action_for(&message) {
                    let value = midi_devices::map::value_of(&message);
                    actions.push((action, value));
                }
            }
        }
        if !score_note_ons.is_empty() {
            let track = self
                .score
                .track
                .or(self.roll.selected)
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let length = entry_length(self.score.note_length.max(1), self.score.dotted);
                for pitch in score_note_ons {
                    if let Ok(pitch) = music_core::Pitch::new(pitch) {
                        let _ = self.editor.insert_length(
                            track,
                            self.score.cursor.max(0),
                            pitch,
                            length,
                        );
                    }
                }
            }
        }
        if score_chord_done {
            let length = entry_length(self.score.note_length.max(1), self.score.dotted);
            self.score.cursor += length;
        }
        for pitch in preview_keys {
            self.preview_pitch(pitch);
        }
        if let Some(name) = detected_chord {
            self.style.last_chord = name.clone();
            self.queue_style_chord(&name);
        }
        if midi_activity && self.settings.kids_mode && self.view == View::Perform {
            self.kids_last_activity = Some(std::time::Instant::now());
            if !self.playing {
                self.toggle_playback();
            }
        }
        if let Some((control, action)) = learned {
            self.midi_map.bind(control, action);
            let text = self.midi_map.to_text();
            if let Err(error) = std::fs::write("lazy-arranger-midi-map.txt", text) {
                self.status = Some(format!("midi map: {error}"));
            } else {
                self.status = Some(format!("bound {} -> {}", control.label(), action.label()));
            }
            self.midi_learn = None;
        }
        for (action, value) in actions {
            self.apply_midi_action(action, value);
        }
    }

    /// Runs one mapped action.
    pub(super) fn apply_midi_action(
        &mut self,
        action: midi_devices::map::Action,
        value: Option<u8>,
    ) {
        use midi_devices::map::Action;
        match action {
            Action::Play => {
                if !self.playing {
                    self.toggle_playback();
                }
            }
            Action::Stop => {
                if self.playing {
                    self.stop_playback();
                }
            }
            Action::Undo => {
                if !self.editor.undo() {
                    self.status = Some(String::from("nothing to undo"));
                }
            }
            Action::Scene(index) => {
                self.launcher.launch(index);
                self.status = Some(format!("scene {}", index + 1));
            }
            Action::Pad(note) => {
                let Some(pads) = &self.pad_state else {
                    self.status = Some(String::from("load a loop into Drum pads first"));
                    return;
                };
                let Some(pad) = pads.kit.pads.iter().find(|pad| pad.note == note).copied() else {
                    self.status = Some(format!("pad {note} is empty"));
                    return;
                };
                match audio::player::Player::new(pads.audio(pad)) {
                    Ok(player) => {
                        self.pad_player = Some(player);
                        self.status = Some(format!("pad {note}"));
                    }
                    Err(error) => self.status = Some(error),
                }
            }
            Action::TrackFader(track) => {
                let Some(value) = value else {
                    return;
                };
                let pan = self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .map(|entry| entry.pan)
                    .unwrap_or(64);
                if let Err(error) = self.editor.set_track_gain(track, value.min(127), pan) {
                    self.status = Some(error);
                }
            }
        }
    }

    pub(super) fn send_chord_notes(
        &mut self,
        chord: &music_core::Chord,
        channel: music_core::Channel,
        velocity: music_core::Velocity,
        hold_ms: u64,
    ) {
        let root = 60 + i32::from(chord.root.as_u8());
        let mut pitches = Vec::new();
        for interval in chord.quality.intervals() {
            if let Ok(pitch) =
                music_core::Pitch::new(u8::try_from(root + i32::from(*interval)).unwrap_or(60))
            {
                pitches.push(pitch);
            }
        }
        let Ok((port, connection)) = self.midi_output_connection() else {
            return;
        };
        if let Ok(mut guard) = connection.lock() {
            let mut failed = None;
            for &pitch in &pitches {
                let message = music_core::MidiMessage::note_on(channel, pitch, velocity);
                let bytes = message.to_bytes();
                if let Err(error) = guard.send(&bytes[..message.encoded_len()]) {
                    failed = Some(error.to_string());
                    break;
                }
            }
            if let Some(error) = failed {
                drop(guard);
                self.midi_out.conn = None;
                self.status = Some(format!("{port}: {error}"));
                return;
            }
            self.status = Some(format!("sent to {port}"));
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            if let Ok(release) = music_core::Velocity::new(0)
                && let Ok(mut guard) = connection.lock()
            {
                for pitch in pitches {
                    let bytes = music_core::MidiMessage::note_off(channel, pitch, release);
                    let _ = guard.send(&bytes.to_bytes()[..3]);
                }
            }
        });
    }

    pub(super) fn psr_send_state(&mut self) {
        let Ok(channel) = music_core::Channel::from_one_based(self.psr.channel) else {
            return;
        };
        if let (Ok(msb), Ok(lsb)) = (
            music_core::ControlNumber::new(0),
            music_core::ControlNumber::new(32),
        ) {
            self.send_midi(music_core::MidiMessage::ControlChange {
                channel,
                controller: msb,
                value: self.psr.bank_msb,
            });
            self.send_midi(music_core::MidiMessage::ControlChange {
                channel,
                controller: lsb,
                value: self.psr.bank_lsb,
            });
        }
        self.send_midi(music_core::MidiMessage::ProgramChange {
            channel,
            program: self.psr.program,
        });
        for (value, controller) in [
            (self.psr.volume, 7_u8),
            (self.psr.pan, 10),
            (self.psr.reverb, 91),
            (self.psr.chorus, 93),
        ] {
            if let Ok(control) = music_core::ControlNumber::new(controller) {
                self.send_midi(music_core::MidiMessage::ControlChange {
                    channel,
                    controller: control,
                    value,
                });
            }
        }
    }

    pub(super) fn psr_store(&mut self, index: usize) {
        if index >= self.psr.regs.len() {
            return;
        }
        self.psr.regs[index] = PsrRegistration {
            channel: self.psr.channel,
            bank_msb: self.psr.bank_msb,
            bank_lsb: self.psr.bank_lsb,
            program: self.psr.program,
            volume: self.psr.volume,
            pan: self.psr.pan,
            reverb: self.psr.reverb,
            chorus: self.psr.chorus,
        };
        save_psr_regs(&self.psr.regs);
        self.status = Some(format!("PSR registration {}", index + 1));
    }

    pub(super) fn psr_recall(&mut self, index: usize) {
        let Some(reg) = self.psr.regs.get(index).copied() else {
            return;
        };
        self.psr.channel = reg.channel.max(1);
        self.psr.bank_msb = reg.bank_msb;
        self.psr.bank_lsb = reg.bank_lsb;
        self.psr.program = reg.program;
        self.psr.volume = reg.volume;
        self.psr.pan = reg.pan;
        self.psr.reverb = reg.reverb;
        self.psr.chorus = reg.chorus;
        self.psr_send_state();
    }

    pub(super) fn xg_window(&mut self, context: &Context) {
        if !self.xg.open {
            return;
        }
        let mut open = self.xg.open;
        let mut send_all = false;
        let mut reset = false;
        let mut sends: Vec<(usize, u8, u8)> = Vec::new();
        egui::Window::new("XG part mixer")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new(
                        "16 MIDI parts · volume / pan / reverb / chorus · uses the selected MIDI output",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Send all").clicked() {
                        send_all = true;
                    }
                    if ui.button("Reset").clicked() {
                        reset = true;
                    }
                });
                for (index, values) in self.xg.parts.iter_mut().enumerate() {
                    let mut current = *values;
                    ui.horizontal(|ui| {
                        ui.label(format!("Part {:>2}", index + 1));
                        for (label, slot, cc) in [
                            ("vol", 0, 7_u8),
                            ("pan", 1, 10),
                            ("rev", 2, 91),
                            ("cho", 3, 93),
                        ] {
                            let mut value = current[slot];
                            if ui
                                .add(egui::Slider::new(&mut value, 0..=127).text(label))
                                .changed()
                            {
                                current[slot] = value;
                                sends.push((index, cc, value));
                            }
                        }
                    });
                    *values = current;
                }
            });
        for (part, cc, value) in sends {
            if let (Ok(channel), Ok(controller)) = (
                music_core::Channel::from_one_based(u8::try_from(part + 1).unwrap_or(1)),
                music_core::ControlNumber::new(cc),
            ) {
                self.send_midi(music_core::MidiMessage::ControlChange {
                    channel,
                    controller,
                    value,
                });
            }
        }
        if reset {
            self.xg.parts = [[100, 64, 40, 0]; 16];
        }
        if send_all {
            let parts = self.xg.parts;
            for (index, values) in parts.iter().enumerate() {
                let Ok(channel) =
                    music_core::Channel::from_one_based(u8::try_from(index + 1).unwrap_or(1))
                else {
                    continue;
                };
                for (value, cc) in values.iter().zip([7_u8, 10, 91, 93]) {
                    if let Ok(controller) = music_core::ControlNumber::new(cc) {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller,
                            value: *value,
                        });
                    }
                }
            }
            self.status = Some(String::from("XG part mixer sent"));
        }
        self.xg.open = open;
    }

    pub(super) fn psr_window(&mut self, context: &Context) {
        if !self.psr.open {
            return;
        }
        let mut open = self.psr.open;
        egui::Window::new("PSR-S970 remote")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new("Yamaha PSR-S970 · MIDI channel 1")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.psr.bank_msb).range(0..=127));
                    ui.label("/");
                    ui.add(egui::DragValue::new(&mut self.psr.bank_lsb).range(0..=127));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.psr.program).range(0..=127));
                    if ui.button("Send voice").clicked()
                        && let Ok(channel) = music_core::Channel::from_one_based(self.psr.channel)
                        && let Ok(msb) = music_core::ControlNumber::new(0)
                        && let Ok(lsb) = music_core::ControlNumber::new(32)
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: msb,
                            value: self.psr.bank_msb,
                        });
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: lsb,
                            value: self.psr.bank_lsb,
                        });
                        self.send_midi(music_core::MidiMessage::ProgramChange {
                            channel,
                            program: self.psr.program,
                        });
                    }
                });
                for (label, controller, current) in [
                    ("Volume", 7_u8, self.psr.volume),
                    ("Pan", 10, self.psr.pan),
                    ("Reverb", 91, self.psr.reverb),
                    ("Chorus", 93, self.psr.chorus),
                ] {
                    let mut value = current;
                    let changed = ui
                        .horizontal(|ui| {
                            ui.label(label);
                            ui.add(egui::Slider::new(&mut value, 0..=127).show_value(true))
                                .changed()
                        })
                        .inner;
                    if changed {
                        match controller {
                            7 => self.psr.volume = value,
                            10 => self.psr.pan = value,
                            91 => self.psr.reverb = value,
                            _ => self.psr.chorus = value,
                        }
                        if let (Ok(channel), Ok(control)) = (
                            music_core::Channel::from_one_based(self.psr.channel),
                            music_core::ControlNumber::new(controller),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller: control,
                                value,
                            });
                        }
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new("Registrations").strong());
                ui.horizontal(|ui| {
                    for index in 0..8 {
                        let response =
                            ui.add_sized([34.0, 30.0], egui::Button::new((index + 1).to_string()));
                        if response.clicked() {
                            self.psr_recall(index);
                        }
                        if response.secondary_clicked() {
                            self.psr_store(index);
                        }
                        response.on_hover_text("Click to recall; right-click to store");
                    }
                });
                if ui
                    .button("Store current in slot 1")
                    .on_hover_text("Right-click a number above to store in that slot")
                    .clicked()
                {
                    self.psr_store(0);
                }
                ui.separator();
                ui.label(egui::RichText::new("Chord audition").strong());
                for row in [["C", "Dm", "Em", "F"], ["G", "Am", "G7", "D7"]] {
                    ui.horizontal(|ui| {
                        for name in row {
                            if ui
                                .add_sized([70.0, 34.0], egui::Button::new(name))
                                .clicked()
                                && let Ok(chord) = music_core::Chord::parse(name)
                                && let Ok(channel) =
                                    music_core::Channel::from_one_based(self.psr.channel)
                                && let Ok(velocity) = music_core::Velocity::new(100)
                            {
                                self.send_chord_notes(&chord, channel, velocity, 500);
                            }
                        }
                    });
                }
                ui.label(
                    egui::RichText::new("Choose an output in Windows > MIDI out.")
                        .size(9.0)
                        .color(theme::text_dim()),
                );
            });
        self.psr.open = open;
    }

    pub(super) fn midi_output_connection(
        &mut self,
    ) -> Result<
        (
            String,
            std::sync::Arc<std::sync::Mutex<midi_devices::devices::MidiOutputConnection>>,
        ),
        String,
    > {
        let Some(port) = self.midi_out.port.clone() else {
            self.midi_out.conn = None;
            let message = String::from("choose a MIDI output first");
            self.status = Some(message.clone());
            return Err(message);
        };
        if let Some((cached_port, connection)) = &self.midi_out.conn
            && cached_port == &port
        {
            return Ok((port, std::sync::Arc::clone(connection)));
        }
        self.midi_out.conn = None;
        match midi_devices::devices::open_output(&port) {
            Ok(connection) => {
                let shared = std::sync::Arc::new(std::sync::Mutex::new(connection));
                self.midi_out.conn = Some((port.clone(), std::sync::Arc::clone(&shared)));
                Ok((port, shared))
            }
            Err(error) => {
                let message = error.to_string();
                self.status = Some(message.clone());
                Err(message)
            }
        }
    }

    pub(super) fn send_raw(&mut self, bytes: &[u8]) {
        let Ok((port, connection)) = self.midi_output_connection() else {
            return;
        };
        let result = connection
            .lock()
            .map_err(|_| String::from("MIDI output lock poisoned"))
            .and_then(|mut guard| guard.send(bytes).map_err(|error| error.to_string()));
        match result {
            Ok(()) => self.status = Some(format!("sent {} byte(s) to {port}", bytes.len())),
            Err(error) => {
                self.midi_out.conn = None;
                self.status = Some(format!("{port}: {error}"));
            }
        }
    }

    pub(super) fn send_midi(&mut self, message: music_core::MidiMessage) {
        let Ok((port, connection)) = self.midi_output_connection() else {
            return;
        };
        let bytes = message.to_bytes();
        let result = connection
            .lock()
            .map_err(|_| String::from("MIDI output lock poisoned"))
            .and_then(|mut guard| {
                guard
                    .send(&bytes[..message.encoded_len()])
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => self.status = Some(format!("sent to {port}")),
            Err(error) => {
                self.midi_out.conn = None;
                self.status = Some(format!("{port}: {error}"));
            }
        }
    }

    pub(super) fn midi_monitor_window(&mut self, context: &Context) {
        if !self.midi_monitor_open {
            return;
        }
        let mut open = self.midi_monitor_open;
        egui::Window::new("MIDI monitor")
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(context, |ui| {
                if ui.button("Clear").clicked() {
                    self.midi_log.clear();
                    self.midi_recent.clear();
                }
                ui.label(
                    egui::RichText::new("Last two minutes of input messages")
                        .size(9.0)
                        .color(theme::text_dim()),
                );
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        if self.midi_log.is_empty() {
                            ui.label("No MIDI messages yet.");
                        }
                        for (at, label) in &self.midi_log {
                            ui.label(format!("{:>6.1}s  {label}", at.elapsed().as_secs_f64()));
                        }
                    });
            });
        self.midi_monitor_open = open;
    }

    pub(super) fn midi_out_window(&mut self, context: &Context) {
        if !self.midi_out.open {
            return;
        }
        let mut open = self.midi_out.open;
        egui::Window::new("MIDI out")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if self.midi_ports.is_none()
                    && let Ok(ports) = midi_devices::devices::list_ports()
                {
                    self.midi_ports = Some((ports.inputs, ports.outputs));
                }
                let outputs = self
                    .midi_ports
                    .as_ref()
                    .map(|(_, outputs)| outputs.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("midi-out-port")
                    .selected_text(self.midi_out.port.as_deref().unwrap_or("Choose output"))
                    .show_ui(ui, |ui| {
                        for name in &outputs {
                            ui.selectable_value(&mut self.midi_out.port, Some(name.clone()), name);
                        }
                    });
                if ui
                    .button("Channel check")
                    .on_hover_text("Play a short note on each of the 16 channels")
                    .clicked()
                    && let Ok((port, connection)) = self.midi_output_connection()
                {
                    self.status = Some(format!("channel check on {port}"));
                    std::thread::spawn(move || {
                        if let Ok(mut guard) = connection.lock() {
                            for channel in 0..16 {
                                if let Ok(messages) =
                                    midi_devices::devices::channel_check_messages(channel, 60)
                                {
                                    for bytes in messages {
                                        let _ = guard.send(&bytes);
                                    }
                                }
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("channel");
                    ui.add(egui::DragValue::new(&mut self.midi_out.channel).range(1..=16));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.midi_out.program).range(0..=127));
                    if ui.button("Send program").clicked()
                        && let Ok(channel) =
                            music_core::Channel::from_one_based(self.midi_out.channel)
                    {
                        self.send_midi(music_core::MidiMessage::ProgramChange {
                            channel,
                            program: self.midi_out.program,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.midi_out.bank_msb).range(0..=127));
                    ui.label("/");
                    ui.add(egui::DragValue::new(&mut self.midi_out.bank_lsb).range(0..=127));
                    if ui.button("Send bank").clicked()
                        && let (Ok(channel), Ok(msb), Ok(lsb)) = (
                            music_core::Channel::from_one_based(self.midi_out.channel),
                            music_core::ControlNumber::new(0),
                            music_core::ControlNumber::new(32),
                        )
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: msb,
                            value: self.midi_out.bank_msb,
                        });
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: lsb,
                            value: self.midi_out.bank_lsb,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("CC");
                    ui.add(egui::DragValue::new(&mut self.midi_out.cc).range(0..=127));
                    ui.add(egui::DragValue::new(&mut self.midi_out.cc_value).range(0..=127));
                    if ui.button("Send CC").clicked()
                        && let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(self.midi_out.channel),
                            music_core::ControlNumber::new(self.midi_out.cc),
                        )
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller,
                            value: self.midi_out.cc_value,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("note");
                    ui.add(egui::DragValue::new(&mut self.midi_out.note).range(0..=127));
                    ui.label("velocity");
                    ui.add(egui::DragValue::new(&mut self.midi_out.velocity).range(1..=127));
                    if ui.button("Send note").clicked()
                        && let (Ok(channel), Ok(pitch), Ok(velocity)) = (
                            music_core::Channel::from_one_based(self.midi_out.channel),
                            music_core::Pitch::new(self.midi_out.note),
                            music_core::Velocity::new(self.midi_out.velocity),
                        )
                        && let Ok((port, connection)) = self.midi_output_connection()
                    {
                        let message = music_core::MidiMessage::note_on(channel, pitch, velocity);
                        let bytes = message.to_bytes();
                        let sent = connection
                            .lock()
                            .map_err(|_| String::from("MIDI output lock poisoned"))
                            .and_then(|mut guard| {
                                guard
                                    .send(&bytes[..message.encoded_len()])
                                    .map_err(|error| error.to_string())
                            });
                        match sent {
                            Ok(()) => {
                                self.status = Some(format!("sent to {port}"));
                                std::thread::spawn(move || {
                                    std::thread::sleep(std::time::Duration::from_millis(350));
                                    if let Ok(mut guard) = connection.lock() {
                                        let bytes = music_core::MidiMessage::note_off(
                                            channel,
                                            pitch,
                                            music_core::Velocity::new(0).unwrap_or(velocity),
                                        );
                                        let _ = guard.send(&bytes.to_bytes()[..3]);
                                    }
                                });
                            }
                            Err(error) => {
                                self.midi_out.conn = None;
                                self.status = Some(format!("{port}: {error}"));
                            }
                        }
                    }
                    if ui.button("Send chord").clicked()
                        && let Some(chord) = self.song().chord_at(self.position).cloned()
                        && let (Ok(channel), Ok(velocity)) = (
                            music_core::Channel::from_one_based(self.midi_out.channel),
                            music_core::Velocity::new(self.midi_out.velocity),
                        )
                    {
                        self.send_chord_notes(&chord, channel, velocity, 500);
                    }
                    if ui.button("Panic").clicked()
                        && let Some(port) = self.midi_out.port.clone()
                    {
                        self.midi_out.conn = None;
                        match midi_devices::devices::panic(&port) {
                            Ok(sent) => self.status = Some(format!("panic sent {sent}")),
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                    if ui.button("Start").clicked() {
                        self.send_raw(&[0xFA]);
                    }
                    if ui.button("Continue").clicked() {
                        self.send_raw(&[0xFB]);
                    }
                    if ui.button("Stop").clicked() {
                        self.send_raw(&[0xFC]);
                    }
                });
                if self.midi_out.port.is_none() {
                    ui.label(
                        egui::RichText::new("No output selected. Use Settings to list ports.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
            });
        self.midi_out.open = open;
    }

    pub(super) fn send_l6max_transport(&mut self, action: &str) {
        let Some(message) = crate::l6max_device::transport_byte(action) else {
            return;
        };
        if self.midi_ports.is_none()
            && let Ok(ports) = midi_devices::devices::list_ports()
        {
            self.midi_ports = Some((ports.inputs, ports.outputs));
        }
        let port = self.midi_ports.as_ref().and_then(|(_, outputs)| {
            outputs
                .iter()
                .find(|name| name.contains("L6max MIDI I/O"))
                .cloned()
        });
        let Some(port) = port else {
            self.status = Some(String::from("connect the L6max MIDI I/O port"));
            return;
        };
        match midi_devices::devices::open_output(&port) {
            Ok(mut connection) => match connection.send(&[message]) {
                Ok(()) => self.status = Some(format!("L6max transport: {action}")),
                Err(error) => self.status = Some(format!("{port}: {error}")),
            },
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    pub(super) fn l6max_control_port(&mut self) -> Option<String> {
        if let Some(port) = &self.midi_out.port
            && port.contains("L6max Mixer Control")
        {
            return Some(port.clone());
        }
        if self.midi_ports.is_none()
            && let Ok(ports) = midi_devices::devices::list_ports()
        {
            self.midi_ports = Some((ports.inputs, ports.outputs));
        }
        if let Some((_, outputs)) = &self.midi_ports
            && let Some(found) = outputs
                .iter()
                .find(|name| name.contains("L6max Mixer Control"))
                .cloned()
        {
            self.midi_out.port = Some(found.clone());
            return Some(found);
        }
        self.midi_out.port.clone()
    }

    pub(super) fn send_l6max_changes(&mut self, changes: &[(u8, u8)], scene: Option<u8>) {
        if changes.is_empty() && scene.is_none() {
            return;
        }
        let Some(port) = self.l6max_control_port() else {
            self.status = Some(String::from("connect the L6max Mixer Control port"));
            return;
        };
        self.release_midi_out_if(&port);
        let channel = self.l6max.device.channel.clamp(1, 16) - 1;
        let Ok(mut connection) = midi_devices::devices::open_output(&port) else {
            self.status = Some(format!("could not open {port}"));
            return;
        };
        for (cc, value) in changes {
            let bytes = [0xB0 | channel, *cc & 0x7F, *value & 0x7F];
            let _ = connection.send(&bytes);
        }
        if let Some(scene) = scene {
            let _ = connection.send(&[0xC0 | channel, scene & 0x7F]);
        }
        self.status = Some(format!(
            "L6max: {} change(s){}",
            changes.len(),
            scene.map_or(String::new(), |scene| format!(", scene {}", scene + 1))
        ));
    }

    pub(super) fn l6max_device_window(&mut self, context: &Context) {
        if !self.l6max.device_open {
            return;
        }
        let port = self.l6max_control_port();
        if let Some(receiver) = &self.l6max.id_reply
            && let Ok(result) = receiver.try_recv()
        {
            match result {
                Ok(info) => {
                    self.l6max.identity = info.clone();
                    self.status = Some(info);
                }
                Err(error) => {
                    self.l6max.identity = error.clone();
                    self.status = Some(error);
                }
            }
            self.l6max.id_reply = None;
        }
        let mut open = self.l6max.device_open;
        let mut state = self.l6max.device.clone();
        let mut changes: Vec<(u8, u8)> = Vec::new();
        let mut scene = None;
        let mut reset = false;
        let mut transport = None;
        let mut identify = false;
        egui::Window::new("L6max device mixer")
            .open(&mut open)
            .resizable(true)
            .default_width(1200.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Zoom LiveTrak L6max").strong());
                    ui.label(
                        egui::RichText::new(port.as_deref().unwrap_or("not connected"))
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    ui.label("channel");
                    ui.add(egui::DragValue::new(&mut state.channel).range(1..=16));
                    if ui.button("Reset all").clicked() {
                        reset = true;
                    }
                    if ui
                        .add_enabled(self.l6max.id_reply.is_none(), egui::Button::new("Identify"))
                        .on_hover_text("Query L6max firmware over L6 Editor Port (SysEx Identity)")
                        .clicked()
                    {
                        identify = true;
                    }
                    if !self.l6max.identity.is_empty() {
                        ui.label(
                            egui::RichText::new(&self.l6max.identity)
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    }
                });
                ui.separator();
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    egui::Grid::new("l6max-device-grid")
                        .spacing([6.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("");
                            for channel in 0..8 {
                                ui.label(
                                    egui::RichText::new(format!("CH{}", channel + 1)).strong(),
                                );
                            }
                            ui.end_row();
                            for (row, label) in crate::l6max_device::PARAMETERS.iter().enumerate() {
                                ui.label(
                                    egui::RichText::new(*label)
                                        .size(9.0)
                                        .color(theme::text_muted()),
                                );
                                for (channel, values) in state.values.iter_mut().enumerate() {
                                    let mut value = values[row];
                                    if ui
                                        .add_sized(
                                            [62.0, 18.0],
                                            egui::Slider::new(&mut value, 0..=127),
                                        )
                                        .changed()
                                    {
                                        values[row] = value;
                                        if let Some(cc) =
                                            crate::l6max_device::parameter_cc(row, channel)
                                        {
                                            changes.push((cc, value));
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                            ui.label(
                                egui::RichText::new("MUTE")
                                    .size(9.0)
                                    .color(theme::text_muted()),
                            );
                            for (channel, muted) in state.mutes.iter_mut().enumerate() {
                                if ui.checkbox(muted, "").changed() {
                                    changes.push((
                                        crate::l6max_device::MUTES[channel],
                                        if *muted { 127 } else { 0 },
                                    ));
                                }
                            }
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("MONO")
                                    .size(9.0)
                                    .color(theme::text_muted()),
                            );
                            for (channel, on) in state.modes.iter_mut().enumerate() {
                                if let Some((_, cc)) = crate::l6max_device::MONO
                                    .iter()
                                    .find(|(index, _)| *index == channel)
                                {
                                    if ui.checkbox(on, "").changed() {
                                        changes.push((*cc, if *on { 127 } else { 0 }));
                                    }
                                } else {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("USB")
                                    .size(9.0)
                                    .color(theme::text_muted()),
                            );
                            for (channel, on) in state.modes.iter_mut().enumerate() {
                                if let Some((_, cc)) = crate::l6max_device::USB
                                    .iter()
                                    .find(|(index, _)| *index == channel)
                                {
                                    if ui.checkbox(on, "").changed() {
                                        changes.push((*cc, if *on { 127 } else { 0 }));
                                    }
                                } else {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Transport");
                    for (action, label) in [
                        ("start", "Start"),
                        ("continue", "Continue"),
                        ("stop", "Stop"),
                    ] {
                        if ui.button(label).clicked() {
                            transport = Some(action);
                        }
                    }
                    ui.label(
                        egui::RichText::new("MIDI I/O port")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Effect");
                    egui::ComboBox::from_id_salt("l6max-efx")
                        .selected_text(crate::l6max_device::EFX_NAMES[state.efx.min(5)])
                        .show_ui(ui, |ui| {
                            for (index, name) in crate::l6max_device::EFX_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut state.efx, index, *name).clicked()
                                    && state.efx == index
                                {
                                    changes.push((
                                        crate::l6max_device::EFX_TYPE,
                                        crate::l6max_device::EFX_VALUES[index],
                                    ));
                                }
                            }
                        });
                    let mut compressor = state.compressor;
                    if ui.checkbox(&mut compressor, "Compressor").changed() {
                        state.compressor = compressor;
                        changes.push((
                            crate::l6max_device::COMPRESSOR,
                            if compressor { 127 } else { 0 },
                        ));
                    }
                    for (index, label) in ["A", "B", "C", "D"].iter().enumerate() {
                        if ui.small_button(format!("Scene {label}")).clicked() {
                            scene = Some(u8::try_from(index).unwrap_or(0));
                        }
                    }
                });
            });
        if reset {
            state = crate::l6max_device::DeviceState {
                channel: state.channel,
                ..crate::l6max_device::DeviceState::default()
            };
            changes.clear();
            for (channel, values) in state.values.iter().enumerate() {
                for (row, value) in values.iter().enumerate() {
                    if let Some(cc) = crate::l6max_device::parameter_cc(row, channel) {
                        changes.push((cc, *value));
                    }
                }
                changes.push((
                    crate::l6max_device::MUTES[channel],
                    if state.mutes[channel] { 127 } else { 0 },
                ));
                if let Some((_, cc)) = crate::l6max_device::MONO
                    .iter()
                    .find(|(index, _)| *index == channel)
                {
                    changes.push((*cc, if state.modes[channel] { 127 } else { 0 }));
                }
                if let Some((_, cc)) = crate::l6max_device::USB
                    .iter()
                    .find(|(index, _)| *index == channel)
                {
                    changes.push((*cc, if state.modes[channel] { 127 } else { 0 }));
                }
            }
            changes.push((crate::l6max_device::EFX_TYPE, 0));
            changes.push((crate::l6max_device::COMPRESSOR, 0));
        }
        if self.l6max.device != state {
            crate::l6max_device::save(
                &crate::settings::Settings::data_dir().join(crate::l6max_device::STATE_PATH),
                &state,
            );
            self.l6max.device = state;
        }
        self.l6max.device_open = open;
        if let Some(action) = transport {
            self.send_l6max_transport(action);
        }
        if identify {
            let editor_port = midi_devices::list_ports().ok().and_then(|ports| {
                ports
                    .outputs
                    .into_iter()
                    .find(|name| name.contains("L6 Editor"))
            });
            if let Some(editor_port) = editor_port {
                let (tx, rx) = std::sync::mpsc::channel();
                self.l6max.id_reply = Some(rx);
                self.l6max.identity = String::from("querying...");
                std::thread::spawn(move || {
                    let result = midi_devices::devices::sysex_exchange(
                        &editor_port,
                        &editor_port,
                        &midi_devices::d05::IDENTITY_REQUEST,
                        Duration::from_millis(1200),
                        Duration::from_millis(200),
                    )
                    .map_err(|err| err.to_string())
                    .and_then(|replies| {
                        replies
                            .iter()
                            .find_map(|reply| midi_devices::d05::IdentityReply::parse(reply))
                            .map(|id| {
                                format!(
                                    "Zoom L6max fw {} (mfr {:02X} fam {:02X}{:02X})",
                                    id.revision_text(),
                                    id.manufacturer.first().copied().unwrap_or(0),
                                    id.family[0],
                                    id.family[1]
                                )
                            })
                            .ok_or_else(|| String::from("no identity reply from L6 Editor Port"))
                    });
                    let _ = tx.send(result);
                });
            } else {
                self.l6max.identity = String::from("L6 Editor Port not connected");
            }
        }
        self.send_l6max_changes(&changes, scene);
    }

    pub(super) fn d05_port_name(&self) -> Option<String> {
        if let Some(port) = &self.midi_out.port
            && port.contains("D-05")
        {
            return Some(port.clone());
        }
        midi_devices::list_ports()
            .ok()
            .and_then(|ports| ports.outputs.into_iter().find(|name| name.contains("D-05")))
    }

    pub(super) fn release_midi_out_if(&mut self, port: &str) {
        if self
            .midi_out
            .conn
            .as_ref()
            .is_some_and(|(cached, _)| cached == port)
        {
            self.midi_out.conn = None;
        }
    }

    pub(super) fn send_d05_raw(&mut self, bytes: &[u8]) {
        let port = self.d05_port_name().or_else(|| self.midi_out.port.clone());
        let Some(port) = port else {
            self.d05.editor.note = String::from("connect Roland D-05 MIDI port");
            return;
        };
        self.release_midi_out_if(&port);
        if let Err(error) = midi_devices::devices::send_raw(&port, bytes) {
            self.d05.editor.note = error.to_string();
        }
    }

    pub(super) fn start_d05_read(&mut self) {
        let Some(port) = self.d05_port_name() else {
            self.d05.editor.note = String::from("Roland D-05 MIDI port not connected");
            return;
        };
        self.release_midi_out_if(&port);
        let device = self.d05.editor.device_id;
        let (tx, rx) = std::sync::mpsc::channel();
        self.d05.reply = Some(rx);
        self.d05.editor.busy = true;
        self.d05.editor.note = String::from("reading temporary patch from D-05...");
        std::thread::spawn(move || {
            let result = midi_devices::d05::read_temp_patch(&port, &port, device)
                .map(|(patch, raw)| (Box::new(patch), raw.len()));
            let _ = tx.send(D05TaskResult::TempPatch(result));
        });
    }

    pub(super) fn d05_editor_window(&mut self, context: &Context) {
        if let Some(receiver) = &self.d05.reply
            && let Ok(result) = receiver.try_recv()
        {
            self.d05.editor.busy = false;
            self.d05.reply = None;
            match result {
                D05TaskResult::TempPatch(Ok((patch, raw_len))) => {
                    let name = patch.patch_name();
                    self.d05.editor.patch = *patch;
                    self.d05.editor.sync_names_from_patch();
                    self.d05.editor.note = format!("read {:?} ({raw_len} B)", name);
                    self.status = Some(format!("D-05: read {:?}", name));
                }
                D05TaskResult::TempPatch(Err(error)) => {
                    self.d05.editor.note = error.clone();
                    self.status = Some(error);
                }
                D05TaskResult::BankNames(bank_lsb, Ok(names)) => {
                    let count = names.len();
                    self.d05.editor.bank_names = names;
                    let label = midi_devices::d05::BANK_NAMES
                        .get(usize::from(bank_lsb))
                        .copied()
                        .unwrap_or("Bank");
                    self.d05.editor.note = format!("{label}: loaded {count} patch name(s)");
                    self.status = Some(format!("D-05: loaded {count} names from {label}"));
                }
                D05TaskResult::BankNames(_, Err(error)) => {
                    self.d05.editor.note = error.clone();
                    self.status = Some(error);
                }
            }
        }
        if !self.d05.open {
            return;
        }
        let mut open = self.d05.open;
        let mut pending_sysex: Vec<Vec<u8>> = Vec::new();
        let mut request_read = false;
        let mut request_send_all = false;
        let mut request_bank_scan = false;
        let mut select_program: Option<u8> = None;
        let port_label = self
            .d05_port_name()
            .unwrap_or_else(|| String::from("not connected"));
        egui::Window::new("Roland D-05 sound editor")
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(context, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Roland D-05").strong());
                    ui.label(
                        egui::RichText::new(&port_label)
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    ui.label("Dev");
                    ui.add(egui::DragValue::new(&mut self.d05.editor.device_id).range(0..=31));
                    ui.label("Ch");
                    ui.add(egui::DragValue::new(&mut self.d05.editor.channel).range(1..=16));
                    let key_mode = self.d05.editor.patch.patch_common(0x12);
                    if midi_devices::d05::is_separate_mode(key_mode) {
                        let lower_ch =
                            midi_devices::d05::separate_lower_channel(self.d05.editor.channel);
                        ui.label(
                            egui::RichText::new(format!("Lower ch {lower_ch}"))
                                .size(9.5)
                                .color(theme::accent()),
                        );
                    }
                    ui.checkbox(&mut self.d05.editor.live_send, "Live");
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!self.d05.editor.busy, egui::Button::new("Read from Synth"))
                        .clicked()
                    {
                        request_read = true;
                    }
                    if ui
                        .add_enabled(!self.d05.editor.busy, egui::Button::new("Send All"))
                        .clicked()
                    {
                        request_send_all = true;
                    }
                    ui.label("Dump");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.d05.editor.dump_path)
                            .desired_width(140.0),
                    );
                    if ui.button("Save Dump").clicked() {
                        match self
                            .d05.editor
                            .patch
                            .to_syx_bytes(self.d05.editor.device_id)
                        {
                            Ok(bytes) => {
                                match std::fs::write(&self.d05.editor.dump_path, &bytes) {
                                    Ok(()) => {
                                        self.d05.editor.note = format!(
                                            "saved {} B -> {}",
                                            bytes.len(),
                                            self.d05.editor.dump_path
                                        );
                                    }
                                    Err(error) => self.d05.editor.note = error.to_string(),
                                }
                            }
                            Err(error) => self.d05.editor.note = error,
                        }
                    }
                    if ui.button("Load Dump").clicked() {
                        match std::fs::read(&self.d05.editor.dump_path)
                            .map_err(|error| error.to_string())
                            .and_then(|bytes| midi_devices::d05::TempPatch::from_syx_bytes(&bytes))
                        {
                            Ok(patch) => {
                                self.d05.editor.patch = patch;
                                self.d05.editor.sync_names_from_patch();
                                self.d05.editor.note =
                                    format!("loaded {:?}", self.d05.editor.patch.patch_name());
                                if self.d05.editor.live_send {
                                    request_send_all = true;
                                }
                            }
                            Err(error) => self.d05.editor.note = error,
                        }
                    }
                });
                if !self.d05.editor.note.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.d05.editor.note)
                            .size(9.5)
                            .color(theme::text_dim()),
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.d05.editor.page,
                        crate::d05_editor::D05Page::Patch,
                        "Patch",
                    );
                    ui.selectable_value(
                        &mut self.d05.editor.page,
                        crate::d05_editor::D05Page::UpperTone,
                        "Upper Tone",
                    );
                    ui.selectable_value(
                        &mut self.d05.editor.page,
                        crate::d05_editor::D05Page::LowerTone,
                        "Lower Tone",
                    );
                    ui.selectable_value(
                        &mut self.d05.editor.page,
                        crate::d05_editor::D05Page::Names,
                        "Names & Banks",
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| match self.d05.editor.page {
                        crate::d05_editor::D05Page::Patch => {
                            let mut key_mode =
                                usize::from(self.d05.editor.patch.patch_common(0x12)).min(8);
                            ui.horizontal(|ui| {
                                ui.label("Key mode");
                                egui::ComboBox::from_id_salt("d05-key-mode")
                                    .selected_text(midi_devices::d05::KEY_MODES[key_mode])
                                    .show_ui(ui, |ui| {
                                        for (index, label) in
                                            midi_devices::d05::KEY_MODES.iter().enumerate()
                                        {
                                            if ui
                                                .selectable_value(&mut key_mode, index, *label)
                                                .clicked()
                                                && let Ok(msg) = self
                                                    .d05.editor
                                                    .set_byte(384 + 0x12, index as u8)
                                                && self.d05.editor.live_send
                                            {
                                                pending_sysex.push(msg);
                                            }
                                        }
                                    });
                                let mut reverb = self.d05.editor.patch.patch_common(0x1e).min(31);
                                ui.label("Reverb");
                                egui::ComboBox::from_id_salt("d05-reverb-slot")
                                    .selected_text(midi_devices::d05::reverb_slot_label(reverb))
                                    .show_ui(ui, |ui| {
                                        for slot in 0..32_u8 {
                                            if ui
                                                .selectable_value(
                                                    &mut reverb,
                                                    slot,
                                                    midi_devices::d05::reverb_slot_label(slot),
                                                )
                                                .clicked()
                                                && let Ok(msg) =
                                                    self.d05.editor.set_byte(384 + 0x1e, slot)
                                                && self.d05.editor.live_send
                                            {
                                                pending_sysex.push(msg);
                                            }
                                        }
                                    });
                            });
                            for param in crate::d05_editor::PATCH_PARAMS {
                                let abs = 384 + param.offset;
                                let mut value =
                                    self.d05.editor.patch.bytes[abs].clamp(param.min, param.max);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut value, param.min..=param.max)
                                            .text(param.label),
                                    )
                                    .changed()
                                    && let Ok(msg) = self.d05.editor.set_byte(abs, value)
                                    && self.d05.editor.live_send
                                {
                                    pending_sysex.push(msg);
                                }
                            }
                        }
                        crate::d05_editor::D05Page::UpperTone
                        | crate::d05_editor::D05Page::LowerTone => {
                            let upper = self.d05.editor.page
                                == crate::d05_editor::D05Page::UpperTone;
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut self.d05.editor.tone_sub, 0, "Common");
                                ui.selectable_value(&mut self.d05.editor.tone_sub, 1, "Partial 1");
                                ui.selectable_value(&mut self.d05.editor.tone_sub, 2, "Partial 2");
                            });
                            let base = crate::d05_editor::D05EditorState::tone_block_base(
                                upper,
                                self.d05.editor.tone_sub,
                            );
                            let params = if self.d05.editor.tone_sub == 0 {
                                crate::d05_editor::TONE_COMMON_PARAMS
                            } else {
                                crate::d05_editor::PARTIAL_PARAMS
                            };
                            for param in params {
                                let abs = base + param.offset;
                                let mut value =
                                    self.d05.editor.patch.bytes[abs].clamp(param.min, param.max);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut value, param.min..=param.max)
                                            .text(param.label),
                                    )
                                    .changed()
                                    && let Ok(msg) = self.d05.editor.set_byte(abs, value)
                                    && self.d05.editor.live_send
                                {
                                    pending_sysex.push(msg);
                                }
                            }
                        }
                        crate::d05_editor::D05Page::Names => {
                            ui.horizontal(|ui| {
                                ui.label("Patch name (18)");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.d05.editor.patch_name)
                                        .char_limit(18)
                                        .desired_width(180.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Upper tone (10)");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.d05.editor.upper_name)
                                        .char_limit(10)
                                        .desired_width(120.0),
                                );
                                ui.label("Lower tone (10)");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.d05.editor.lower_name)
                                        .char_limit(10)
                                        .desired_width(120.0),
                                );
                            });
                            if ui.button("Send Names").clicked() {
                                match self.d05.editor.commit_names_dt1() {
                                    Ok(msgs) => {
                                        pending_sysex.extend(msgs);
                                        self.d05.editor.note = String::from("sent patch & tone names");
                                    }
                                    Err(error) => self.d05.editor.note = error,
                                }
                            }
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Bank");
                                let current = usize::from(self.d05.editor.bank_lsb).min(13);
                                egui::ComboBox::from_id_salt("d05-bank")
                                    .selected_text(midi_devices::d05::BANK_NAMES[current])
                                    .show_ui(ui, |ui| {
                                        for (index, label) in
                                            midi_devices::d05::BANK_NAMES.iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut self.d05.editor.bank_lsb,
                                                index as u8,
                                                *label,
                                            );
                                        }
                                    });
                                if ui
                                    .add_enabled(
                                        !self.d05.editor.busy,
                                        egui::Button::new("Get Sound Names"),
                                    )
                                    .on_hover_text(
                                        "Scan all 64 patch names in this bank and restore the active patch",
                                    )
                                    .clicked()
                                {
                                    request_bank_scan = true;
                                }
                            });
                            if !self.d05.editor.bank_names.is_empty() {
                                egui::Grid::new("d05-bank-names")
                                    .num_columns(2)
                                    .spacing([12.0, 4.0])
                                    .show(ui, |ui| {
                                        for (index, name) in
                                            self.d05.editor.bank_names.iter().enumerate()
                                        {
                                            let group = index / 8 + 1;
                                            let slot = index % 8 + 1;
                                            if ui
                                                .small_button(format!("{group}-{slot}  {name}"))
                                                .clicked()
                                            {
                                                select_program = Some(index as u8);
                                            }
                                            if index % 2 == 1 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            }
                        }
                    });
            });
        self.d05.open = open;
        for msg in pending_sysex {
            self.send_d05_raw(&msg);
        }
        if request_send_all {
            if let Some(port) = self.d05_port_name().or_else(|| self.midi_out.port.clone()) {
                match midi_devices::d05::write_temp_patch(
                    &port,
                    self.d05.editor.device_id,
                    &self.d05.editor.patch,
                ) {
                    Ok(packets) => {
                        self.d05.editor.note = format!("sent full patch ({packets} DT1 packets)");
                    }
                    Err(error) => self.d05.editor.note = error,
                }
            } else {
                self.d05.editor.note = String::from("Roland D-05 MIDI port not connected");
            }
        }
        if let Some(program) = select_program {
            let ch = self.d05.editor.channel.clamp(1, 16) - 1;
            let bank = self.d05.editor.bank_lsb;
            for msg in midi_devices::d05::bank_select_messages(ch, bank) {
                self.send_d05_raw(&msg);
            }
            self.send_d05_raw(&midi_devices::d05::program_change_message(ch, program));
            request_read = true;
        }
        if request_read {
            self.start_d05_read();
        }
        if request_bank_scan {
            if let Some(port) = self.d05_port_name() {
                let device = self.d05.editor.device_id;
                let ch = self.d05.editor.channel.clamp(1, 16) - 1;
                let bank = self.d05.editor.bank_lsb;
                let (tx, rx) = std::sync::mpsc::channel();
                self.d05.reply = Some(rx);
                self.d05.editor.busy = true;
                self.d05.editor.note = String::from("scanning 64 patch names from D-05...");
                std::thread::spawn(move || {
                    let result =
                        midi_devices::d05::read_bank_names(&port, &port, device, ch, bank, 64);
                    let _ = tx.send(D05TaskResult::BankNames(bank, result));
                });
            } else {
                self.d05.editor.note = String::from("Roland D-05 MIDI port not connected");
            }
        }
    }

    pub(super) fn l6max_mixer_window(&mut self, context: &Context) {
        if !self.l6max.mixer_open {
            return;
        }
        let mut open = self.l6max.mixer_open;
        let compact = self.l6max.compact;
        let mut edits = Vec::new();
        let mut scene_save = None;
        let mut scene_recall = None;
        egui::Window::new("L6max mixer")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .show(context, |ui| {
                let tracks: Vec<(TrackId, String, u8, u8, bool, u8)> = self
                    .song()
                    .tracks
                    .iter()
                    .filter(|track| track.audio.is_some())
                    .take(8)
                    .map(|track| {
                        (
                            track.id,
                            track.name.clone(),
                            track.volume,
                            track.pan.clamp(0, 127),
                            track.muted,
                            track.send_level,
                        )
                    })
                    .collect();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.l6max.compact, "Compact layout");
                    for (index, label) in ["A", "B", "C", "D"].iter().enumerate() {
                        if ui.small_button(format!("Save {label}")).clicked() {
                            scene_save = Some(index);
                        }
                        if ui.small_button(*label).clicked() {
                            scene_recall = Some(index);
                        }
                    }
                    if ui.button("Reset").clicked() {
                        for (id, _, _, _, _, _) in &tracks {
                            edits.push((*id, 100, 64, false, 0));
                        }
                    }
                });
                if tracks.is_empty() {
                    ui.label("No imported L6max audio tracks yet.");
                    return;
                }
                for (id, name, volume, pan, muted, send) in &tracks {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        let mut level = *volume;
                        if ui
                            .add(egui::Slider::new(&mut level, 0..=127).text("level"))
                            .changed()
                        {
                            edits.push((*id, level, *pan, *muted, *send));
                        }
                        let mut pan = *pan;
                        if ui
                            .add(egui::Slider::new(&mut pan, 0..=127).text("pan"))
                            .changed()
                        {
                            edits.push((*id, *volume, pan, *muted, *send));
                        }
                        let mut send_value = *send;
                        if ui
                            .add(egui::Slider::new(&mut send_value, 0..=127).text("send"))
                            .changed()
                        {
                            edits.push((*id, *volume, pan, *muted, send_value));
                        }
                        let mut mute = *muted;
                        if ui.checkbox(&mut mute, "Mute").changed() {
                            edits.push((*id, *volume, pan, mute, *send));
                        }
                    });
                }
                ui.label(
                    egui::RichText::new(
                        "L6max EQ, MONO, USB routing, effects and compressor need device project data.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                if compact {
                    ui.label(
                        egui::RichText::new("Compact rows use the same controls.")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                }
            });
        if let Some(index) = scene_save {
            let scene = L6Scene {
                channels: self
                    .song()
                    .tracks
                    .iter()
                    .filter(|track| track.audio.is_some())
                    .take(8)
                    .map(|track| {
                        (
                            track.volume,
                            track.pan.clamp(0, 127),
                            track.muted,
                            track.send_level,
                        )
                    })
                    .collect(),
            };
            self.l6max.scenes[index] = Some(scene);
            save_l6_scenes(&self.l6max.scenes);
            self.status = Some(format!("saved L6max scene {}", ["A", "B", "C", "D"][index]));
        }
        if let Some(index) = scene_recall
            && let Some(scene) = self.l6max.scenes[index].clone()
        {
            let audio_tracks: Vec<TrackId> = self
                .song()
                .tracks
                .iter()
                .filter(|track| track.audio.is_some())
                .take(8)
                .map(|track| track.id)
                .collect();
            for (track, (volume, pan, muted, send)) in audio_tracks.iter().zip(&scene.channels) {
                edits.push((*track, *volume, *pan, *muted, *send));
            }
            self.status = Some(format!(
                "recalled L6max scene {}",
                ["A", "B", "C", "D"][index]
            ));
        }
        for (id, volume, pan, muted, send) in edits {
            let _ = self.editor.set_track_gain(id, volume, pan);
            let info = self
                .song()
                .tracks
                .iter()
                .find(|track| track.id == id)
                .map(|track| (track.soloed, track.armed, track.bus, track.send));
            if let Some((soloed, armed, bus, has_send)) = info {
                let _ = self.editor.set_track_flags(id, muted, soloed, armed);
                let _ = self.editor.set_track_routing(id, bus, has_send, send);
            }
        }
        self.l6max.mixer_open = open;
    }

    pub(super) fn controller_window(&mut self, context: &egui::Context) {
        if !self.controller_open {
            return;
        }
        let tracks: Vec<(TrackId, String)> = self
            .song()
            .tracks
            .iter()
            .map(|track| (track.id, track.name.clone()))
            .collect();
        let actions: Vec<midi_devices::map::Action> = vec![
            midi_devices::map::Action::Play,
            midi_devices::map::Action::Stop,
            midi_devices::map::Action::Undo,
            midi_devices::map::Action::Scene(0),
            midi_devices::map::Action::TrackFader(
                tracks
                    .first()
                    .map_or_else(|| TrackId::new(1).expect("valid"), |(id, _)| *id),
            ),
            midi_devices::map::Action::Pad(38),
        ];
        let mut open = self.controller_open;
        egui::Window::new("Controllers")
            .open(&mut open)
            .default_width(340.0)
            .show(context, |ui| {
                let choice = self.controller_action.min(actions.len() - 1);
                let mut chosen = choice;
                egui::ComboBox::from_id_salt("controller-action")
                    .selected_text(actions[choice].label())
                    .show_ui(ui, |ui| {
                        for (index, action) in actions.iter().enumerate() {
                            ui.selectable_value(&mut chosen, index, action.label());
                        }
                    });
                self.controller_action = chosen;
                if ui.button("Learn a controller").clicked() {
                    self.midi_learn = Some(midi_devices::map::Learn::begin(actions[chosen]));
                    self.status = Some(String::from("waiting for a note or CC"));
                }
                if ui
                    .button("Load Launchkey mixer preset")
                    .on_hover_text("CC 21-28 set faders for tracks 1-8")
                    .clicked()
                {
                    self.midi_map = midi_devices::map::launchkey_mixer_map();
                    let text = self.midi_map.to_text();
                    match std::fs::write("lazy-arranger-midi-map.txt", text) {
                        Ok(()) => {
                            self.status = Some(String::from("loaded Launchkey mixer preset"));
                        }
                        Err(error) => self.status = Some(format!("midi map: {error}")),
                    }
                }
                if self.midi_learn.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the next note or control change…")
                            .color(theme::accent()),
                    );
                }
                ui.separator();
                ui.label(egui::RichText::new("Recent MIDI").strong());
                if self.midi_recent.is_empty() {
                    ui.label(
                        egui::RichText::new("no messages yet")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for line in self.midi_recent.iter().take(8) {
                    ui.label(
                        egui::RichText::new(line)
                            .monospace()
                            .size(9.0)
                            .color(theme::text_muted()),
                    );
                }
                ui.separator();
                for (number, action) in self.midi_map.controller_bindings() {
                    let value = self.midi_cc_values[usize::from(number)];
                    ui.horizontal(|ui| {
                        ui.label(format!("CC {number}"));
                        ui.add(
                            egui::ProgressBar::new(f32::from(value) / 127.0)
                                .desired_width(120.0)
                                .text(format!("{value}")),
                        );
                        ui.label(action.label());
                    });
                }
                if self.midi_map.controller_bindings().is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no controller bindings — choose an action and press Learn",
                        )
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                }
                if ui.button("Clear all bindings").clicked() {
                    self.midi_map.bindings.clear();
                    let text = self.midi_map.to_text();
                    match std::fs::write("lazy-arranger-midi-map.txt", text) {
                        Ok(()) => self.status = Some(String::from("MIDI bindings cleared")),
                        Err(error) => self.status = Some(format!("midi map: {error}")),
                    }
                }
            });
        self.controller_open = open;
    }

    pub(super) fn midi_map_window(&mut self, context: &egui::Context) {
        if !self.midi_map_open {
            return;
        }
        let tracks: Vec<(TrackId, String)> = self
            .song()
            .tracks
            .iter()
            .map(|track| (track.id, track.name.clone()))
            .collect();
        let mut actions: Vec<(midi_devices::map::Action, String)> = vec![
            (midi_devices::map::Action::Play, String::from("play")),
            (midi_devices::map::Action::Stop, String::from("stop")),
            (midi_devices::map::Action::Undo, String::from("undo")),
            (midi_devices::map::Action::Scene(0), String::from("scene 0")),
            (midi_devices::map::Action::Pad(38), String::from("pad 38")),
        ];
        if tracks.is_empty() {
            let fallback = TrackId::new(1).expect("valid");
            actions.push((
                midi_devices::map::Action::TrackFader(fallback),
                format!("fader {fallback}"),
            ));
        } else {
            for (id, name) in &tracks {
                actions.push((
                    midi_devices::map::Action::TrackFader(*id),
                    format!("fader: {name}"),
                ));
            }
        }
        let mut open = self.midi_map_open;
        let mut remove_control: Option<midi_devices::map::Control> = None;
        egui::Window::new("MIDI map")
            .open(&mut open)
            .default_width(360.0)
            .show(context, |ui| {
                let choice = self.midi_learn_choice.min(actions.len() - 1);
                let mut chosen = choice;
                egui::ComboBox::from_id_salt("midi-map-action")
                    .selected_text(&actions[choice].1)
                    .show_ui(ui, |ui| {
                        for (index, (_, label)) in actions.iter().enumerate() {
                            ui.selectable_value(&mut chosen, index, label);
                        }
                    });
                self.midi_learn_choice = chosen;
                ui.horizontal(|ui| {
                    if ui.button("Learn").clicked() {
                        self.midi_learn = Some(midi_devices::map::Learn::begin(actions[chosen].0));
                        self.status = Some(String::from("waiting for a note or CC"));
                    }
                    if self.midi_learn.is_some() && ui.button("Cancel").clicked() {
                        self.midi_learn = None;
                        self.status = Some(String::from("learn cancelled"));
                    }
                    if ui.button("Clear all").clicked() {
                        self.midi_map = midi_devices::map::MidiMap::default();
                        let _ =
                            std::fs::write("lazy-arranger-midi-map.txt", self.midi_map.to_text());
                        self.status = Some(String::from("MIDI map cleared"));
                    }
                    if ui
                        .button("Launchkey pads")
                        .on_hover_text("Notes 36-51 audition the matching drum pad")
                        .clicked()
                    {
                        self.midi_map = midi_devices::map::launchkey_pad_map();
                        let _ =
                            std::fs::write("lazy-arranger-midi-map.txt", self.midi_map.to_text());
                        self.status = Some(String::from("Launchkey pad map loaded"));
                    }
                });
                if self.midi_learn.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the next note or control change…")
                            .color(theme::accent()),
                    );
                }
                ui.separator();
                if self.midi_map.bindings.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no bindings — choose an action, press Learn, then move a control",
                        )
                        .color(theme::text_dim()),
                    );
                }
                for binding in &self.midi_map.bindings {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} = {}",
                            binding.control_label(),
                            binding.action.label()
                        ));
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove this binding")
                            .clicked()
                        {
                            remove_control = Some(binding.control);
                        }
                    });
                }
                ui.label(
                    egui::RichText::new("Learn saves the map automatically.")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
            });
        if let Some(control) = remove_control {
            self.midi_map.unbind(control);
            let _ = std::fs::write("lazy-arranger-midi-map.txt", self.midi_map.to_text());
            self.status = Some(String::from("MIDI binding removed"));
        }
        self.midi_map_open = open;
    }

    pub(super) fn device_editor_window(&mut self, context: &egui::Context) {
        if !self.device_editor_open {
            return;
        }
        let mut open = self.device_editor_open;
        egui::Window::new("Device editor")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if self.device_profiles.is_empty() {
                    ui.label("No device profiles found. Add YAML files to the profiles folder.");
                    return;
                }
                let selected = self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .map(|profile| profile.name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("device-editor-profile")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, profile) in self.device_profiles.iter().enumerate() {
                            if ui
                                .selectable_value(
                                    &mut self.device_editor_profile,
                                    index,
                                    &profile.name,
                                )
                                .clicked()
                            {
                                self.device_editor_values.clear();
                                self.device_editor_voice = 0;
                                self.device_editor_kit = 0;
                            }
                        }
                    });
                if self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .is_some_and(|profile| profile.id == "d05")
                    && ui.button("Open Roland D-05 sound editor...").clicked()
                {
                    self.d05.open = true;
                }
                let controls = self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .map(|profile| profile.controls.clone())
                    .unwrap_or_default();
                if self.device_editor_values.len() != controls.len() {
                    self.device_editor_values =
                        controls.iter().map(|control| control.min).collect();
                }
                let profile = self.device_profiles.get(self.device_editor_profile);
                let voices = profile
                    .map(|profile| profile.voices.clone())
                    .unwrap_or_default();
                let bank = profile.and_then(|profile| profile.banks.first().cloned());
                if !voices.is_empty() {
                    let mut voice = self.device_editor_voice.min(voices.len() - 1);
                    ui.horizontal(|ui| {
                        ui.label("Voice");
                        egui::ComboBox::from_id_salt("device-editor-voice")
                            .selected_text(&voices[voice].0)
                            .show_ui(ui, |ui| {
                                for (index, (name, _)) in voices.iter().enumerate() {
                                    ui.selectable_value(&mut voice, index, name);
                                }
                            });
                        if ui.button("Send voice").clicked() {
                            let (_, program) = voices[voice];

                            for message in midi_devices::profiles::voice_messages(
                                bank.as_ref(),
                                program,
                                music_core::Channel::first(),
                            ) {
                                self.send_midi(message);
                            }
                            self.status = Some(format!("sent voice {program}"));
                        }
                        if let Some(track) = self.roll.selected
                            && ui.button("Use on track").clicked()
                        {
                            let program = voices[voice].1;
                            let bank = bank.as_ref().and_then(|bank| {
                                (bank.msb.is_some() || bank.lsb.is_some()).then_some(
                                    project::song::BankSelect {
                                        msb: bank.msb.unwrap_or(0),
                                        lsb: bank.lsb.unwrap_or(0),
                                    },
                                )
                            });
                            match self.editor.set_track_voice(track, Some(program), bank) {
                                Ok(()) => {
                                    self.status =
                                        Some(format!("track {} voice {program}", track.get()));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    });
                    self.device_editor_voice = voice;
                }
                let kits = self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .map(|profile| profile.drum_kits.clone())
                    .unwrap_or_default();
                if !kits.is_empty() {
                    let mut kit = self.device_editor_kit.min(kits.len() - 1);
                    ui.horizontal(|ui| {
                        ui.label("Kit");
                        egui::ComboBox::from_id_salt("device-editor-kit")
                            .selected_text(&kits[kit].name)
                            .show_ui(ui, |ui| {
                                for (index, entry) in kits.iter().enumerate() {
                                    ui.selectable_value(&mut kit, index, &entry.name);
                                }
                            });
                        let kit_data = kits[kit].clone();
                        let bank = (kit_data.msb.is_some() || kit_data.lsb.is_some()).then_some(
                            project::song::BankSelect {
                                msb: kit_data.msb.unwrap_or(0),
                                lsb: kit_data.lsb.unwrap_or(0),
                            },
                        );
                        let profile_bank = midi_devices::profiles::Bank {
                            name: kit_data.name.clone(),
                            program: kit_data.program,
                            msb: kit_data.msb,
                            lsb: kit_data.lsb,
                        };
                        if ui.button("Send kit").clicked() {
                            if let Ok(channel) = music_core::Channel::new(9) {
                                for message in midi_devices::profiles::voice_messages(
                                    Some(&profile_bank),
                                    kit_data.program,
                                    channel,
                                ) {
                                    self.send_midi(message);
                                }
                            }
                            self.status = Some(format!("sent kit {}", kit_data.program));
                        }
                        if let Some(track) = self.roll.selected
                            && ui.button("Use kit").clicked()
                        {
                            match self
                                .editor
                                .set_track_voice(track, Some(kit_data.program), bank)
                            {
                                Ok(()) => {
                                    self.status = Some(format!(
                                        "track {} kit {}",
                                        track.get(),
                                        kit_data.program
                                    ));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    });
                    self.device_editor_kit = kit;
                }
                for (index, control) in controls.iter().enumerate() {
                    let name = control.name.clone();
                    let cc = control.cc;
                    let channel_number = control.channel;
                    let min = control.min;
                    let max = control.max;
                    let Some(value) = self.device_editor_values.get(index).copied() else {
                        continue;
                    };
                    let mut current = value;
                    if ui
                        .add(egui::Slider::new(&mut current, min..=max).text(name))
                        .changed()
                    {
                        if let Some(slot) = self.device_editor_values.get_mut(index) {
                            *slot = current;
                        }
                        if let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(channel_number),
                            music_core::ControlNumber::new(cc),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller,
                                value: current,
                            });
                        }
                    }
                }
                if controls.is_empty() {
                    ui.label("This profile has no controls.");
                }
                if ui
                    .button("Send all")
                    .on_hover_text("Send every control's current value")
                    .clicked()
                {
                    let sends: Vec<(u8, u8, u8)> = controls
                        .iter()
                        .zip(&self.device_editor_values)
                        .map(|(control, value)| (control.channel, control.cc, *value))
                        .collect();
                    for (channel_number, cc, value) in sends {
                        if let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(channel_number),
                            music_core::ControlNumber::new(cc),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller,
                                value,
                            });
                        }
                    }
                }
                ui.separator();
                ui.label("System exclusive (hex)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.device_sysex)
                        .hint_text("F0 7E ... F7")
                        .desired_width(280.0),
                );
                if ui.button("Send SysEx").clicked() {
                    match parse_sysex(&self.device_sysex) {
                        Ok(bytes) => self.send_raw(&bytes),
                        Err(error) => self.status = Some(error),
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Dump file");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.device_dump_path).desired_width(200.0),
                    );
                    if ui.button("Save dump").clicked() {
                        match parse_sysex(&self.device_sysex) {
                            Ok(bytes) => match std::fs::write(&self.device_dump_path, &bytes) {
                                Ok(()) => {
                                    self.status = Some(format!(
                                        "wrote {} byte(s) to {}",
                                        bytes.len(),
                                        self.device_dump_path
                                    ));
                                }
                                Err(error) => self.status = Some(error.to_string()),
                            },
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Load dump").clicked() {
                        match std::fs::read(&self.device_dump_path) {
                            Ok(bytes) => {
                                self.device_sysex = sysex_hex(&bytes);
                                self.send_raw(&bytes);
                                self.status =
                                    Some(format!("loaded and sent {} byte(s)", bytes.len()));
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                    if ui.button("Send file").clicked() {
                        match std::fs::read(&self.device_dump_path) {
                            Ok(bytes) => {
                                self.send_raw(&bytes);
                                self.status = Some(format!("sent {} byte(s)", bytes.len()));
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                });
            });
        self.device_editor_open = open;
    }

    pub(super) fn start_midi_recording(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let Some(keyboard) = self.settings.keyboard.clone() else {
            self.status = Some(String::from("choose a MIDI keyboard in Settings first"));
            return;
        };
        let tempo = self.song().tempo_map.clone();
        let seconds = self.record_seconds;
        let count_in = self.settings.record_count_in;
        let bpm = tempo.bpm_at(0).max(1.0);
        let meter = self.song().time_signature;
        let delay =
            f64::from(count_in) * meter.bar_ticks() as f64 / music_core::PPQ as f64 * 60.0 / bpm;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if delay > 0.0 {
                if let Ok(player) = audio::player::Player::new(count_in_audio(bpm, count_in, meter))
                {
                    player.play();
                    std::thread::sleep(std::time::Duration::from_secs_f64(delay));
                    player.release();
                } else {
                    std::thread::sleep(std::time::Duration::from_secs_f64(delay));
                }
            }
            let result =
                midi_devices::devices::record_to_song(&keyboard, seconds, &tempo, "Recorded")
                    .map(|(song, _, _)| song)
                    .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.record_target = self
            .song()
            .tracks
            .iter()
            .find(|track| track.armed)
            .map(|track| track.id);
        self.recording = Some(receiver);
        self.status = Some(if count_in == 0 {
            format!("recording {seconds} s")
        } else {
            format!("count-in {count_in} bar(s), then {seconds} s")
        });
    }
}
