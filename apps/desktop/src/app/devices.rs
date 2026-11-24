//! Device helpers: SysEx text, L6 mixer scenes, PSR registrations and MIDI
//! device profiles.

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
