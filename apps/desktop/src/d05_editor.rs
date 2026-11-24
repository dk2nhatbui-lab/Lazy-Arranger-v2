//! Roland D-05 sound editor state, parameter tables and live SysEx helpers.

use midi_devices::d05::{self, TEMP_PATCH_BYTES, TempPatch};

/// Active page inside the Roland D-05 editor window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum D05Page {
    /// Patch Common, Key Mode, Separate-mode MIDI and Reverb slots 1–32.
    #[default]
    Patch,
    /// Upper Tone Common (`00 01 00`) and Upper Partials 1–2 (`00 00 00` / `00 00 40`).
    UpperTone,
    /// Lower Tone Common (`00 02 40`) and Lower Partials 1–2 (`00 01 40` / `00 02 00`).
    LowerTone,
    /// Patch/Tone names (`Send Names`) and 8 Internal / 6 Preset bank browser (`Get Sound Names`).
    Names,
}

/// One slider parameter mapped directly to an offset inside its 64-byte D-05 block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D05Param {
    /// Short control label.
    pub label: &'static str,
    /// Offset within the 64-byte block (`0..64`).
    pub offset: usize,
    /// Minimum MIDI value.
    pub min: u8,
    /// Maximum MIDI value.
    pub max: u8,
}

/// Patch Common parameters (`00 03 00`, block base 384).
pub const PATCH_PARAMS: &[D05Param] = &[
    D05Param {
        label: "Split point (C2..C7)",
        offset: 0x13,
        min: 0,
        max: 60,
    },
    D05Param {
        label: "Portamento mode (0:U+L 1:U 2:L)",
        offset: 0x14,
        min: 0,
        max: 2,
    },
    D05Param {
        label: "Hold mode (0:U+L 1:U 2:L)",
        offset: 0x15,
        min: 0,
        max: 2,
    },
    D05Param {
        label: "Upper key shift (-24..+24)",
        offset: 0x16,
        min: 0,
        max: 48,
    },
    D05Param {
        label: "Lower key shift (-24..+24)",
        offset: 0x17,
        min: 0,
        max: 48,
    },
    D05Param {
        label: "Upper fine tune (-50..+50)",
        offset: 0x18,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Lower fine tune (-50..+50)",
        offset: 0x19,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Bender range",
        offset: 0x1a,
        min: 0,
        max: 12,
    },
    D05Param {
        label: "Aftertouch pitch bend",
        offset: 0x1b,
        min: 0,
        max: 24,
    },
    D05Param {
        label: "Portamento time",
        offset: 0x1c,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Output mode (0..3)",
        offset: 0x1d,
        min: 0,
        max: 3,
    },
    D05Param {
        label: "Reverb balance",
        offset: 0x1f,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Total volume",
        offset: 0x20,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Tone balance (U..L)",
        offset: 0x21,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Chase mode (0:U+L 1:U 2:L)",
        offset: 0x22,
        min: 0,
        max: 2,
    },
    D05Param {
        label: "Chase level",
        offset: 0x23,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Chase time",
        offset: 0x24,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "MIDI separate channel offset",
        offset: 0x25,
        min: 0,
        max: 15,
    },
];

/// Tone Common parameters (`00 01 00` Upper / `00 02 40` Lower).
pub const TONE_COMMON_PARAMS: &[D05Param] = &[
    D05Param {
        label: "Structure (1..7)",
        offset: 0x0a,
        min: 0,
        max: 6,
    },
    D05Param {
        label: "Pitch env vel range",
        offset: 0x0b,
        min: 0,
        max: 2,
    },
    D05Param {
        label: "Pitch env time KF",
        offset: 0x0c,
        min: 0,
        max: 4,
    },
    D05Param {
        label: "LFO 1 rate",
        offset: 0x1b,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "LFO 2 rate",
        offset: 0x1f,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "LFO 3 rate",
        offset: 0x23,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "EQ low freq",
        offset: 0x26,
        min: 0,
        max: 15,
    },
    D05Param {
        label: "EQ low gain",
        offset: 0x27,
        min: 0,
        max: 24,
    },
    D05Param {
        label: "EQ high freq",
        offset: 0x28,
        min: 0,
        max: 21,
    },
    D05Param {
        label: "EQ high Q",
        offset: 0x29,
        min: 0,
        max: 8,
    },
    D05Param {
        label: "EQ high gain",
        offset: 0x2a,
        min: 0,
        max: 24,
    },
    D05Param {
        label: "Chorus type (1..8)",
        offset: 0x2b,
        min: 0,
        max: 7,
    },
    D05Param {
        label: "Chorus rate",
        offset: 0x2c,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Chorus depth",
        offset: 0x2d,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Chorus balance",
        offset: 0x2e,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "Partial mute (bitmask 0..3)",
        offset: 0x2f,
        min: 0,
        max: 3,
    },
    D05Param {
        label: "Partial balance",
        offset: 0x30,
        min: 0,
        max: 100,
    },
];

/// Partial parameters (`00 00 00`, `00 00 40`, `00 01 40`, `00 02 00`).
pub const PARTIAL_PARAMS: &[D05Param] = &[
    D05Param {
        label: "WG pitch coarse",
        offset: 0x00,
        min: 0,
        max: 72,
    },
    D05Param {
        label: "WG pitch fine",
        offset: 0x01,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "WG pitch key follow",
        offset: 0x02,
        min: 0,
        max: 16,
    },
    D05Param {
        label: "WG waveform (0:Sq 1:Saw)",
        offset: 0x06,
        min: 0,
        max: 1,
    },
    D05Param {
        label: "WG PCM wave (0..99)",
        offset: 0x07,
        min: 0,
        max: 99,
    },
    D05Param {
        label: "WG pulse width",
        offset: 0x08,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "WG PW velocity",
        offset: 0x09,
        min: 0,
        max: 14,
    },
    D05Param {
        label: "TVF cutoff",
        offset: 0x0f,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "TVF resonance",
        offset: 0x10,
        min: 0,
        max: 30,
    },
    D05Param {
        label: "TVF key follow",
        offset: 0x11,
        min: 0,
        max: 14,
    },
    D05Param {
        label: "TVF env depth",
        offset: 0x15,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "TVA level",
        offset: 0x22,
        min: 0,
        max: 100,
    },
    D05Param {
        label: "TVA velocity",
        offset: 0x23,
        min: 0,
        max: 14,
    },
    D05Param {
        label: "TVA T1 (attack)",
        offset: 0x27,
        min: 0,
        max: 50,
    },
    D05Param {
        label: "TVA T2",
        offset: 0x28,
        min: 0,
        max: 50,
    },
    D05Param {
        label: "TVA T3",
        offset: 0x29,
        min: 0,
        max: 50,
    },
    D05Param {
        label: "TVA T4",
        offset: 0x2a,
        min: 0,
        max: 50,
    },
    D05Param {
        label: "TVA T5 (release)",
        offset: 0x2b,
        min: 0,
        max: 50,
    },
];

/// Mutable UI state for the Roland D-05 sound editor window.
#[derive(Clone, Debug)]
pub struct D05EditorState {
    /// Active 448-byte temporary patch buffer.
    pub patch: TempPatch,
    /// Roland SysEx device ID (`0..=31`; default `0`).
    pub device_id: u8,
    /// One-based MIDI channel (`1..=16`).
    pub channel: u8,
    /// Selected editor page.
    pub page: D05Page,
    /// Sub-tab inside Upper/Lower tone (`0` = Common, `1` = Partial 1, `2` = Partial 2).
    pub tone_sub: usize,
    /// Selected bank LSB (`0..=7` Internal 1–8, `8..=13` Preset 1–6).
    pub bank_lsb: u8,
    /// Fetched patch names for the selected bank.
    pub bank_names: Vec<String>,
    /// Editable patch name text buffer.
    pub patch_name: String,
    /// Editable upper tone name text buffer.
    pub upper_name: String,
    /// Editable lower tone name text buffer.
    pub lower_name: String,
    /// Path used by Save Dump / Load Dump.
    pub dump_path: String,
    /// Whether slider/combo edits send DT1 live.
    pub live_send: bool,
    /// Whether a background MIDI transfer is running.
    pub busy: bool,
    /// Short status message for the D-05 window.
    pub note: String,
}

impl Default for D05EditorState {
    fn default() -> Self {
        let patch = TempPatch::default();
        Self {
            patch_name: patch.patch_name(),
            upper_name: patch.upper_tone_name(),
            lower_name: patch.lower_tone_name(),
            patch,
            device_id: 0,
            channel: 1,
            page: D05Page::Patch,
            tone_sub: 0,
            bank_lsb: 0,
            bank_names: Vec::new(),
            dump_path: String::from("d05-patch.syx"),
            live_send: true,
            busy: false,
            note: String::new(),
        }
    }
}

impl D05EditorState {
    /// Syncs the editable name text fields from `self.patch`.
    pub fn sync_names_from_patch(&mut self) {
        self.patch_name = self.patch.patch_name();
        self.upper_name = self.patch.upper_tone_name();
        self.lower_name = self.patch.lower_tone_name();
    }

    /// Writes the edited name text fields back into `self.patch` and returns the
    /// three DT1 SysEx messages (`Send Names`).
    ///
    /// # Errors
    ///
    /// Returns an error if `device_id > 31`.
    pub fn commit_names_dt1(&mut self) -> Result<Vec<Vec<u8>>, String> {
        self.patch.set_patch_name(&self.patch_name);
        self.patch.set_upper_tone_name(&self.upper_name);
        self.patch.set_lower_tone_name(&self.lower_name);
        self.sync_names_from_patch();
        d05::send_names_dt1(self.device_id, &self.patch)
    }

    /// Returns the base byte offset (`0..448`) for a tone page and sub-tab.
    #[must_use]
    pub fn tone_block_base(upper: bool, tone_sub: usize) -> usize {
        match (upper, tone_sub) {
            (true, 1) => 0,    // Upper Partial 1 (00 00 00)
            (true, 2) => 64,   // Upper Partial 2 (00 00 40)
            (true, _) => 128,  // Upper Common    (00 01 00)
            (false, 1) => 192, // Lower Partial 1 (00 01 40)
            (false, 2) => 256, // Lower Partial 2 (00 02 00)
            (false, _) => 320, // Lower Common    (00 02 40)
        }
    }

    /// Updates a single byte in `self.patch` and returns its DT1 message.
    ///
    /// # Errors
    ///
    /// Returns an error if `absolute_offset >= TEMP_PATCH_BYTES` or `device_id > 31`.
    pub fn set_byte(&mut self, absolute_offset: usize, value: u8) -> Result<Vec<u8>, String> {
        if absolute_offset >= TEMP_PATCH_BYTES {
            return Err(format!("offset {absolute_offset} out of range"));
        }
        let clamped = value.min(127);
        self.patch.bytes[absolute_offset] = clamped;
        d05::parameter_dt1(self.device_id, absolute_offset, clamped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_parameter_tables_stay_within_their_64_byte_blocks() {
        for param in PATCH_PARAMS
            .iter()
            .chain(TONE_COMMON_PARAMS)
            .chain(PARTIAL_PARAMS)
        {
            assert!(param.offset < 64, "{} offset {}", param.label, param.offset);
            assert!(param.min <= param.max, "{} range", param.label);
            assert!(param.max <= 127, "{} max", param.label);
        }
    }

    #[test]
    fn tone_block_bases_match_d05_memory_map() {
        assert_eq!(D05EditorState::tone_block_base(true, 1), 0);
        assert_eq!(D05EditorState::tone_block_base(true, 2), 64);
        assert_eq!(D05EditorState::tone_block_base(true, 0), 128);
        assert_eq!(D05EditorState::tone_block_base(false, 1), 192);
        assert_eq!(D05EditorState::tone_block_base(false, 2), 256);
        assert_eq!(D05EditorState::tone_block_base(false, 0), 320);
    }

    #[test]
    fn commit_names_and_single_byte_edits_emit_valid_dt1() {
        let mut state = D05EditorState {
            patch_name: String::from("Neo Horizon"),
            upper_name: String::from("Bass Fx"),
            lower_name: String::from("SweepPad"),
            ..D05EditorState::default()
        };
        let msgs = state.commit_names_dt1().expect("names");
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            d05::decode_name(&d05::parse_dt1(&msgs[2]).expect("dt1").data),
            "Neo Horizon"
        );
        let rev = state.set_byte(384 + 0x1e, 23).expect("reverb");
        let parsed = d05::parse_dt1(&rev).expect("parse");
        assert_eq!(parsed.address, [0x00, 0x03, 0x1e]);
        assert_eq!(parsed.data, vec![23]);
    }
}
