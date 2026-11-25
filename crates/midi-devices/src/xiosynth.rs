//! Novation XioSynth 25/49 dedicated sound editor model, CC/NRPN encoders, and
//! factory Bank A / Bank B patch catalog.
//!
//! Parameters, CC numbers, NRPN numbers (MSB 0), signed offsets, discrete
//! switch values, and 200 factory patch names are taken directly from the
//! authentic XioSynth MIDI chart and factory bank dumps documented in C++ v1
//! `resources/devices/novation-xiosynth.yaml`.

use music_core::{Channel, ControlNumber, MidiMessage};

/// How a XioSynth parameter is addressed over MIDI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Standard MIDI Control Change (`0..=127`).
    Cc(u8),
    /// Novation NRPN with MSB 0 (`CC 99 = 0`, `CC 98 = lsb`, `CC 6 = value`).
    Nrpn(u8),
}

impl Target {
    /// Compact label for UI badges (e.g. `"CC 105"` or `"NRPN 50"`).
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Cc(cc) => format!("CC {cc}"),
            Self::Nrpn(nrpn) => format!("NRPN {nrpn}"),
        }
    }
}

/// One named option for a discrete or switch parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    /// Human-readable option label.
    pub label: &'static str,
    /// Raw MIDI value (`0..=127`) sent for this choice.
    pub value: u8,
}

/// Specification of one Novation XioSynth synth parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    /// Parameter name.
    pub name: &'static str,
    /// CC or NRPN target.
    pub target: Target,
    /// Minimum raw MIDI value.
    pub min: u8,
    /// Maximum raw MIDI value.
    pub max: u8,
    /// Display offset added to raw value (e.g. `-64` for signed bipolar controls).
    pub offset: i16,
    /// Default raw MIDI value.
    pub default: u8,
    /// Optional unit suffix.
    pub unit: &'static str,
    /// Discrete choices; empty for continuous sliders.
    pub choices: &'static [Choice],
}

impl Param {
    const fn cc(name: &'static str, cc: u8, default: u8) -> Self {
        Self {
            name,
            target: Target::Cc(cc),
            min: 0,
            max: 127,
            offset: 0,
            default,
            unit: "",
            choices: &[],
        }
    }

    const fn cc_signed(name: &'static str, cc: u8) -> Self {
        Self {
            name,
            target: Target::Cc(cc),
            min: 0,
            max: 127,
            offset: -64,
            default: 64,
            unit: "",
            choices: &[],
        }
    }

    const fn cc_range(
        name: &'static str,
        cc: u8,
        min: u8,
        max: u8,
        offset: i16,
        default: u8,
        unit: &'static str,
    ) -> Self {
        Self {
            name,
            target: Target::Cc(cc),
            min,
            max,
            offset,
            default,
            unit,
            choices: &[],
        }
    }

    const fn cc_choices(name: &'static str, cc: u8, choices: &'static [Choice]) -> Self {
        Self {
            name,
            target: Target::Cc(cc),
            min: choices[0].value,
            max: choices[choices.len() - 1].value,
            offset: 0,
            default: choices[0].value,
            unit: "",
            choices,
        }
    }

    const fn nrpn(name: &'static str, nrpn: u8, max: u8, default: u8) -> Self {
        Self {
            name,
            target: Target::Nrpn(nrpn),
            min: 0,
            max,
            offset: 0,
            default,
            unit: "",
            choices: &[],
        }
    }

    const fn nrpn_signed(name: &'static str, nrpn: u8) -> Self {
        Self {
            name,
            target: Target::Nrpn(nrpn),
            min: 0,
            max: 127,
            offset: -64,
            default: 64,
            unit: "",
            choices: &[],
        }
    }

    const fn nrpn_choices(name: &'static str, nrpn: u8, choices: &'static [Choice]) -> Self {
        Self {
            name,
            target: Target::Nrpn(nrpn),
            min: choices[0].value,
            max: choices[choices.len() - 1].value,
            offset: 0,
            default: choices[0].value,
            unit: "",
            choices,
        }
    }

    /// Formats a raw MIDI value for display.
    #[must_use]
    pub fn format_value(&self, raw: u8) -> String {
        if let Some(choice) = self.choices.iter().find(|entry| entry.value == raw) {
            return String::from(choice.label);
        }
        let shown = i16::from(raw) + self.offset;
        if self.unit.is_empty() {
            if self.offset < 0 && shown > 0 {
                format!("+{shown}")
            } else {
                shown.to_string()
            }
        } else if self.offset < 0 && shown > 0 {
            format!("+{shown} {}", self.unit)
        } else {
            format!("{shown} {}", self.unit)
        }
    }
}

/// One parameter page of the XioSynth editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    /// Page title.
    pub name: &'static str,
    /// Ordered parameters on this page.
    pub params: &'static [Param],
}

const OSC1_OCTAVE: &[Choice] = &[
    Choice {
        label: "-1",
        value: 16,
    },
    Choice {
        label: "0",
        value: 17,
    },
    Choice {
        label: "+1",
        value: 18,
    },
    Choice {
        label: "+2",
        value: 19,
    },
];
const OSC2_OCTAVE: &[Choice] = &[
    Choice {
        label: "-1",
        value: 20,
    },
    Choice {
        label: "0",
        value: 21,
    },
    Choice {
        label: "+1",
        value: 22,
    },
    Choice {
        label: "+2",
        value: 23,
    },
];
const OSC3_OCTAVE: &[Choice] = &[
    Choice {
        label: "-1",
        value: 24,
    },
    Choice {
        label: "0",
        value: 25,
    },
    Choice {
        label: "+1",
        value: 26,
    },
    Choice {
        label: "+2",
        value: 27,
    },
];
const POLY_MODE: &[Choice] = &[
    Choice {
        label: "Mono",
        value: 0,
    },
    Choice {
        label: "Mono AG",
        value: 1,
    },
    Choice {
        label: "Poly 1",
        value: 2,
    },
    Choice {
        label: "Poly 2",
        value: 3,
    },
];
const UNISON: &[Choice] = &[
    Choice {
        label: "Off",
        value: 0,
    },
    Choice {
        label: "2",
        value: 1,
    },
    Choice {
        label: "3",
        value: 2,
    },
    Choice {
        label: "4",
        value: 3,
    },
    Choice {
        label: "5",
        value: 4,
    },
    Choice {
        label: "6",
        value: 5,
    },
    Choice {
        label: "7",
        value: 6,
    },
    Choice {
        label: "8",
        value: 7,
    },
];
const OSC_SYNC: &[Choice] = &[
    Choice {
        label: "Off",
        value: 10,
    },
    Choice {
        label: "On",
        value: 11,
    },
];
const PORTA_MODE: &[Choice] = &[
    Choice {
        label: "Exponential",
        value: 0,
    },
    Choice {
        label: "Linear",
        value: 1,
    },
];
const NOISE_TYPE: &[Choice] = &[
    Choice {
        label: "White",
        value: 0,
    },
    Choice {
        label: "High pass",
        value: 1,
    },
    Choice {
        label: "Band pass",
        value: 2,
    },
    Choice {
        label: "HP x BP",
        value: 3,
    },
];
const FILTER_TYPE: &[Choice] = &[
    Choice {
        label: "Low pass",
        value: 0,
    },
    Choice {
        label: "Band pass",
        value: 1,
    },
    Choice {
        label: "High pass",
        value: 2,
    },
];
const LFO1_KEYSYNC: &[Choice] = &[
    Choice {
        label: "Off",
        value: 18,
    },
    Choice {
        label: "On",
        value: 19,
    },
];
const LFO1_ONESHOT: &[Choice] = &[
    Choice {
        label: "Off",
        value: 16,
    },
    Choice {
        label: "On",
        value: 17,
    },
];
const LFO2_KEYSYNC: &[Choice] = &[
    Choice {
        label: "Off",
        value: 24,
    },
    Choice {
        label: "On",
        value: 25,
    },
];
const LFO2_ONESHOT: &[Choice] = &[
    Choice {
        label: "Off",
        value: 22,
    },
    Choice {
        label: "On",
        value: 23,
    },
];
const UNIPOLAR: &[Choice] = &[
    Choice {
        label: "Centre zero",
        value: 0,
    },
    Choice {
        label: "All positive",
        value: 1,
    },
];
const REVERB_TYPE: &[Choice] = &[
    Choice {
        label: "Chamber",
        value: 8,
    },
    Choice {
        label: "Small room",
        value: 9,
    },
    Choice {
        label: "Large room",
        value: 10,
    },
    Choice {
        label: "Small hall",
        value: 11,
    },
    Choice {
        label: "Large hall",
        value: 12,
    },
    Choice {
        label: "Grand hall",
        value: 13,
    },
];
const CHORUS_TYPE: &[Choice] = &[
    Choice {
        label: "Chorus",
        value: 34,
    },
    Choice {
        label: "Phaser",
        value: 35,
    },
];
const ARP_DIRECTION: &[Choice] = &[
    Choice {
        label: "Up",
        value: 0,
    },
    Choice {
        label: "Down",
        value: 1,
    },
    Choice {
        label: "Up/Down 1",
        value: 2,
    },
    Choice {
        label: "Up/Down 2",
        value: 3,
    },
    Choice {
        label: "Order",
        value: 4,
    },
    Choice {
        label: "Chord",
        value: 5,
    },
];
const ARP_OCTAVES: &[Choice] = &[
    Choice {
        label: "1",
        value: 36,
    },
    Choice {
        label: "2",
        value: 37,
    },
    Choice {
        label: "3",
        value: 38,
    },
    Choice {
        label: "4",
        value: 39,
    },
];
const ARP_KEYSYNC: &[Choice] = &[
    Choice {
        label: "Off",
        value: 30,
    },
    Choice {
        label: "On",
        value: 31,
    },
];
const ARP_LATCH: &[Choice] = &[
    Choice {
        label: "Off",
        value: 32,
    },
    Choice {
        label: "On",
        value: 33,
    },
];
const XGATOR_ON: &[Choice] = &[
    Choice {
        label: "Off",
        value: 44,
    },
    Choice {
        label: "On",
        value: 45,
    },
];
const XGATOR_MODE: &[Choice] = &[
    Choice {
        label: "Mono 16",
        value: 0,
    },
    Choice {
        label: "Mono alt 1",
        value: 1,
    },
    Choice {
        label: "Mono alt 2",
        value: 2,
    },
    Choice {
        label: "Stereo 16",
        value: 3,
    },
    Choice {
        label: "Stereo swap 1",
        value: 4,
    },
    Choice {
        label: "Stereo swap 2",
        value: 5,
    },
];

/// All ten Novation XioSynth sound editor pages.
pub const PAGES: &[Page] = &[
    Page {
        name: "Oscillator 1",
        params: &[
            Param::nrpn("Waveform", 50, 31, 0),
            Param::nrpn_choices("Octave", 105, OSC1_OCTAVE),
            Param::cc_range("Semitone", 40, 52, 76, -64, 64, "st"),
            Param::cc_range("Cents", 41, 14, 114, -64, 64, "ct"),
            Param::cc_signed("Bend Wheel Pitch", 42),
            Param::cc_signed("LFO 1 Pitch", 43),
            Param::cc_signed("Mod Env Pitch", 44),
            Param::cc_signed("Pulse Width Position", 45),
            Param::cc_signed("LFO 2 PW Mod", 46),
            Param::cc_signed("Mod Env PW Mod", 47),
        ],
    },
    Page {
        name: "Oscillator 2",
        params: &[
            Param::nrpn("Waveform", 51, 31, 0),
            Param::nrpn_choices("Octave", 105, OSC2_OCTAVE),
            Param::cc_range("Semitone", 48, 52, 76, -64, 64, "st"),
            Param::cc_range("Cents", 49, 14, 114, -64, 64, "ct"),
            Param::cc_signed("Bend Wheel Pitch", 50),
            Param::cc_signed("LFO 1 Pitch", 51),
            Param::cc_signed("Mod Env Pitch", 52),
            Param::cc_signed("Pulse Width Position", 53),
            Param::cc_signed("LFO 2 PW Mod", 54),
            Param::cc_signed("Mod Env PW Mod", 55),
        ],
    },
    Page {
        name: "Oscillator 3",
        params: &[
            Param::nrpn("Waveform", 52, 31, 0),
            Param::nrpn_choices("Octave", 105, OSC3_OCTAVE),
            Param::cc_range("Semitone", 56, 52, 76, -64, 64, "st"),
            Param::cc_range("Cents", 57, 14, 114, -64, 64, "ct"),
            Param::cc_signed("Bend Wheel Pitch", 58),
            Param::cc_signed("LFO 1 Pitch", 59),
            Param::cc_signed("Mod Env Pitch", 60),
            Param::cc_signed("Pulse Width Position", 61),
            Param::cc_signed("LFO 2 PW Mod", 62),
            Param::cc_signed("Mod Env PW Mod", 63),
        ],
    },
    Page {
        name: "Oscillators Common",
        params: &[
            Param::nrpn_choices("Poly Mode", 105, POLY_MODE),
            Param::nrpn_choices("Unison", 106, UNISON),
            Param::cc("Unison Detune", 68, 0),
            Param::cc("Random Detune", 69, 0),
            Param::nrpn_choices("Osc 1>2 Sync", 104, OSC_SYNC),
            Param::cc("Portamento Time", 5, 0),
            Param::cc_choices("Portamento Mode", 70, PORTA_MODE),
            Param::cc_range("Preglide", 8, 52, 76, -64, 64, "semitones"),
            Param::nrpn("FM 2>3 Fixed Level", 0, 127, 0),
            Param::nrpn_signed("FM 2>3 AD Env Depth", 1),
            Param::nrpn_signed("FM 2>3 LFO 1 Depth", 78),
            Param {
                name: "Keysync Start Phase",
                target: Target::Nrpn(69),
                min: 0,
                max: 120,
                offset: 0,
                default: 0,
                unit: "x3 deg",
                choices: &[],
            },
            Param::nrpn("Fixed Note", 74, 127, 0),
        ],
    },
    Page {
        name: "Mixer",
        params: &[
            Param::cc("Osc 1 Level", 72, 127),
            Param::cc("Osc 2 Level", 73, 0),
            Param::cc("Osc 3 Level", 74, 0),
            Param::cc("Noise Level", 75, 0),
            Param::cc("Ring Mod 1x2 Level", 76, 0),
            Param::nrpn_choices("Noise Type", 70, NOISE_TYPE),
            Param::nrpn_signed("Osc 1 LFO 1 Level Mod", 63),
            Param::nrpn_signed("Osc 2 LFO 2 Level Mod", 64),
            Param::nrpn_signed("Osc 3 AD Env Level Mod", 65),
            Param::nrpn_signed("Noise LFO 1 Level Mod", 66),
            Param::nrpn_signed("Ring LFO 1 Level Mod", 67),
            Param::cc("Mix Output Level", 119, 100),
        ],
    },
    Page {
        name: "Filter",
        params: &[
            Param::nrpn_choices("Type", 62, FILTER_TYPE),
            Param::nrpn("Shape", 36, 127, 0),
            Param::cc("Frequency", 105, 127),
            Param::cc("Resonance", 106, 0),
            Param::cc("Overdrive", 104, 0),
            Param::cc("Q Normalise", 103, 0),
            Param::cc_signed("Mod Env Depth", 107),
            Param::cc_signed("LFO 2 Depth", 102),
            Param::nrpn("Keyboard Tracking", 11, 127, 0),
            Param::nrpn_signed("Velocity Control", 72),
            Param::nrpn_signed("Mod Wheel Frequency", 12),
            Param::nrpn_signed("Aftertouch Frequency", 13),
        ],
    },
    Page {
        name: "Envelopes",
        params: &[
            Param::cc("Amp Attack", 108, 0),
            Param::cc("Amp Decay", 109, 64),
            Param::cc("Amp Sustain", 110, 127),
            Param::cc("Amp Release", 111, 16),
            Param::cc_signed("Amp Velocity", 112),
            Param::cc("Mod Attack", 114, 0),
            Param::cc("Mod Decay", 115, 64),
            Param::cc("Mod Sustain", 116, 64),
            Param::cc("Mod Release", 117, 16),
            Param::cc_signed("Mod Velocity", 118),
            Param::nrpn("AD Attack", 3, 127, 0),
            Param::nrpn("AD Decay", 4, 127, 64),
            Param::nrpn_signed("AD Velocity", 2),
        ],
    },
    Page {
        name: "LFOs",
        params: &[
            Param::nrpn("LFO 1 Waveform", 53, 31, 0),
            Param::cc("LFO 1 Speed", 80, 64),
            Param::cc_range("LFO 1 Sync Speed", 81, 0, 34, 0, 0, ""),
            Param::cc("LFO 1 Delay", 82, 0),
            Param::nrpn_choices("LFO 1 Keysync", 104, LFO1_KEYSYNC),
            Param::nrpn_choices("LFO 1 One-shot", 104, LFO1_ONESHOT),
            Param::nrpn_choices("LFO 1 Unipolar", 55, UNIPOLAR),
            Param::nrpn("LFO 2 Waveform", 54, 31, 0),
            Param::cc("LFO 2 Speed", 83, 64),
            Param::cc_range("LFO 2 Sync Speed", 84, 0, 34, 0, 0, ""),
            Param::cc("LFO 2 Delay", 85, 0),
            Param::nrpn_choices("LFO 2 Keysync", 104, LFO2_KEYSYNC),
            Param::nrpn_choices("LFO 2 One-shot", 104, LFO2_ONESHOT),
            Param::nrpn_choices("LFO 2 Unipolar", 56, UNIPOLAR),
        ],
    },
    Page {
        name: "Effects",
        params: &[
            Param::cc("Distortion", 90, 0),
            Param::cc("Distortion Compensation", 17, 64),
            Param::cc("Delay Send", 92, 0),
            Param::cc("Delay Time", 19, 64),
            Param::cc_range("Delay Sync Time", 20, 0, 19, 0, 0, ""),
            Param::cc("Delay Feedback", 21, 32),
            Param::cc("Delay Stereo Width", 22, 64),
            Param::cc("Delay Ratio", 23, 64),
            Param::cc("Reverb Send", 91, 0),
            Param::nrpn_choices("Reverb Type", 106, REVERB_TYPE),
            Param::cc("Reverb Decay", 25, 64),
            Param::cc("Chorus Send", 93, 0),
            Param::nrpn_choices("Chorus Type", 104, CHORUS_TYPE),
            Param::cc("Chorus Rate", 27, 40),
            Param::cc("Chorus Feedback", 29, 0),
            Param::cc("Chorus Mod Depth", 30, 40),
            Param::cc("Chorus Centre", 31, 64),
            Param::cc("Pan Mod Depth", 94, 0),
            Param::cc("Pan Rate", 12, 40),
            Param::cc_signed("Pan", 10),
            Param::nrpn_signed("EQ Bass", 29),
            Param::nrpn_signed("EQ Mid", 30),
            Param::nrpn_signed("EQ Treble", 31),
        ],
    },
    Page {
        name: "Arpeggiator",
        params: &[
            Param::nrpn("Pattern", 71, 32, 0),
            Param::cc_choices("Direction", 3, ARP_DIRECTION),
            Param::cc_range("Rate", 9, 0, 127, 64, 56, "bpm"),
            Param::cc_range("Sync", 87, 0, 15, 0, 0, ""),
            Param::cc_signed("Gate Time", 88),
            Param::nrpn_choices("Octaves", 105, ARP_OCTAVES),
            Param::nrpn_choices("Keysync", 104, ARP_KEYSYNC),
            Param::nrpn_choices("Latch", 104, ARP_LATCH),
            Param::nrpn_choices("X-Gator", 104, XGATOR_ON),
            Param::nrpn_choices("X-Gator Mode", 41, XGATOR_MODE),
            Param::nrpn("X-Gator Sync", 40, 19, 0),
            Param::nrpn("X-Gator Level", 42, 127, 64),
            Param::nrpn("X-Gator Edge", 43, 127, 0),
            Param::nrpn("X-Gator Decay", 44, 127, 64),
            Param::nrpn_signed("X-Gator Delay", 45),
        ],
    },
];

/// Novation XioSynth factory Bank A patches (CC 32 = 0, Program 0..=99).
pub const BANK_A: [&str; 100] = [
    "Big Bass",
    "Little Old One",
    "Alls Square In",
    "Moosa",
    "Some Where Out",
    "Grapefruit Dream",
    "A Bassive",
    "The Mellow One",
    "Blade Stepper",
    "They Well Grow",
    "Bottom Feeder",
    "SEMe Bass",
    "Click Bass",
    "Bend Me",
    "Rezo Lead",
    "1/5s Arp",
    "ZigZag Bass",
    "Wooden Sides Too",
    "SEMe Pad",
    "Sliding",
    "JZab NastyBass",
    "JZab HypnoBass",
    "JZab troit Bass",
    "JZab Square Bass",
    "JZab Steel Bass",
    "JZab Wap Lead",
    "JZab Bronze",
    "JZab Gamelan",
    "JZab Nice Pad",
    "JZab CricketB",
    "Fery FuzzyTriad",
    "Fery MangoBass",
    "Fery Beefy",
    "Fery LemonChick",
    "Fery ZeeLead",
    "Fery TriLazer",
    "Fery AcidPassArp",
    "Fery Housesitter",
    "Fery Aurian",
    "Fery PitchClitch",
    "RenP Bigfoot",
    "RenP Amp Bass",
    "RenP Diatribe",
    "RenP Funkenstein",
    "RenP Perception",
    "RenP Paranoia",
    "RenP Pyro",
    "RenP Tube",
    "RenP Rushcutter",
    "RenP Rip",
    "Skwf Throaty",
    "Skwf Sweepitup",
    "Skwf Long Road",
    "Skwf Shizzle",
    "Skwf Roargun",
    "Skwf Butcher",
    "Skwf Lil Ben",
    "Skwf Vibey",
    "Skwf Skankhead",
    "Skwf Swirlygig",
    "Shim Mod Wheely",
    "Shim Shim Bass",
    "Shim Mod Bass",
    "Shim Lokey Bass",
    "Shim StabardMod",
    "Shim EastisEast",
    "Shim Dark Storm",
    "Shim Mars",
    "Shim Loopay",
    "Shim Spacecode",
    "Roots Wingart",
    "Roots Wobblin",
    "Roots SnapBass",
    "Roots Gabriel",
    "Roots Stringoo",
    "Roots lasticBand",
    "Roots strings",
    "Roots Vibratine",
    "Roots DrawtheBar",
    "Roots DeepSpace9",
    "MyroMe",
    "DropTheP",
    "DropTheP2",
    "80 Pad1(wMod)",
    "80 Pad2(wMod)",
    "Raymondsynth",
    "Ampsource",
    "StolenFork",
    "Siren(8vesplit)",
    "Superfoghorn",
    "Wooden Sides",
    "Pluck Pad",
    "Maroder",
    "Wired X-Gator",
    "Pad Gator",
    "Classic Pulse",
    "Mars Gator",
    "Release me",
    "SlipperyWalls",
    "Init Program",
];

/// Novation XioSynth factory Bank B patches (CC 32 = 1, Program 0..=99).
pub const BANK_B: [&str; 100] = [
    "Angry Bass",
    "Chunky Bass",
    "Concept",
    "Detroit Bass",
    "DnB Bandpass Bas",
    "DnB Reverse Bass",
    "DnB Short Bass",
    "DnB Square Bass",
    "Eighty",
    "Electro Bass",
    "Fat Kid Bass",
    "Garage Bass",
    "Heavy Bass",
    "LFO Repeat Bass",
    "Lime Green Bass",
    "Moogs R Us",
    "Mute Bass",
    "Need Atkins?",
    "ObiUnia",
    "Old Funk Bass",
    "ProOnes",
    "Punch Growl Bass",
    "Ruff Bass",
    "TB-303 Bass",
    "Velo Bass Slap",
    "Wobble Bass",
    "X-Bass",
    "Abalone",
    "Action",
    "All Star",
    "Backwoods",
    "Band Pass Sweep",
    "Bright Lead",
    "Cold Call Lead",
    "Eastern Promise",
    "EDP Wasp Lead",
    "Freakin",
    "Greydance",
    "Guilotine",
    "Harsh Lead",
    "Kano",
    "Legato Lead",
    "Liam Lead",
    "Lovely Hard Lead",
    "Mario MDMA",
    "Mellow Chord Ld",
    "Prophecy Lead",
    "Quake",
    "Rockford Lead",
    "Shadow lead",
    "Simple Plink",
    "Soft me hit Meee",
    "Spooked",
    "Square Stab",
    "Surrender",
    "Aerosol",
    "Ambient Pad",
    "Big Soft Pad",
    "Brass Pad",
    "British Patch",
    "Cern",
    "Church Organ",
    "Euphonic",
    "Float Pad",
    "Long Pad S/H+AT",
    "Melancholy",
    "Quiet Pad",
    "Sharp String",
    "Space Pad",
    "Symphonic",
    "Tall Frost",
    "Tape Strings",
    "Thin Obie String",
    "Twin Peaks Strng",
    "And Again Arp",
    "Animation",
    "Epic Arp",
    "Fast Boil",
    "Hazel Eye",
    "Simple Chord Arp",
    "Trance Thing Arp",
    "Aaaaahhhh",
    "Brain Washed MW",
    "Ciani PopNPour",
    "Digeridoo    AT",
    "Engines",
    "Kick Start AT",
    "Schrodingers Cat",
    "Scratch Perverts",
    "SH101 Thunder",
    "Take Off",
    "Whoosh",
    "WindscreenWipers",
    "808 Cowbell",
    "DnB Bomb Tom",
    "Fuzz 808 Kick",
    "Gaba Kick & 303",
    "Psy Kick Pattern",
    "Single Psy Kick",
    "Splat",
];

fn control_number(cc: u8) -> ControlNumber {
    ControlNumber::new(cc.min(127)).unwrap_or_else(|_| ControlNumber::channel_volume())
}

/// Encodes one XioSynth parameter change as MIDI messages (1 CC or 3-message NRPN).
#[must_use]
pub fn encode_param(channel: Channel, param: &Param, raw_value: u8) -> Vec<MidiMessage> {
    let value = raw_value.clamp(param.min, param.max).min(127);
    match param.target {
        Target::Cc(controller) => vec![MidiMessage::ControlChange {
            channel,
            controller: control_number(controller),
            value,
        }],
        Target::Nrpn(lsb) => vec![
            MidiMessage::ControlChange {
                channel,
                controller: control_number(99),
                value: 0,
            },
            MidiMessage::ControlChange {
                channel,
                controller: control_number(98),
                value: lsb.min(127),
            },
            MidiMessage::ControlChange {
                channel,
                controller: control_number(6),
                value,
            },
        ],
    }
}

/// Encodes a XioSynth bank + patch selection (`CC 32 = bank`, `ProgramChange = patch`).
#[must_use]
pub fn encode_patch_select(channel: Channel, bank: u8, patch: u8) -> Vec<MidiMessage> {
    vec![
        MidiMessage::ControlChange {
            channel,
            controller: control_number(32),
            value: bank.min(1),
        },
        MidiMessage::ProgramChange {
            channel,
            program: patch.min(99),
        },
    ]
}

/// Mutable state for the dedicated Novation XioSynth editor UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XioState {
    /// Selected MIDI channel (`1..=16`).
    pub channel: u8,
    /// Active page index into [`PAGES`].
    pub page: usize,
    /// Selected bank (`0` = Bank A, `1` = Bank B).
    pub bank: u8,
    /// Selected zero-based patch (`0..=99`).
    pub patch: u8,
    /// Raw MIDI value for every parameter `[page][param]`.
    pub values: Vec<Vec<u8>>,
}

impl Default for XioState {
    fn default() -> Self {
        let values = PAGES
            .iter()
            .map(|page| page.params.iter().map(|param| param.default).collect())
            .collect();
        Self {
            channel: 1,
            page: 0,
            bank: 0,
            patch: 0,
            values,
        }
    }
}

impl XioState {
    /// Resets one page's parameters to their defaults.
    pub fn reset_page(&mut self, page_index: usize) {
        if let (Some(page), Some(row)) = (PAGES.get(page_index), self.values.get_mut(page_index)) {
            *row = page.params.iter().map(|param| param.default).collect();
        }
    }

    /// MIDI channel clamped to `1..=16`.
    #[must_use]
    pub fn midi_channel(&self) -> Channel {
        Channel::from_one_based(self.channel.clamp(1, 16)).unwrap_or_else(|_| Channel::first())
    }

    /// Encodes every parameter on `page_index` into MIDI messages.
    #[must_use]
    pub fn page_messages(&self, page_index: usize) -> Vec<MidiMessage> {
        let Some(page) = PAGES.get(page_index) else {
            return Vec::new();
        };
        let Some(row) = self.values.get(page_index) else {
            return Vec::new();
        };
        let channel = self.midi_channel();
        let mut out = Vec::new();
        for (param, value) in page.params.iter().zip(row.iter().copied()) {
            out.extend(encode_param(channel, param, value));
        }
        out
    }

    /// Current factory patch list for `self.bank`.
    #[must_use]
    pub fn current_bank_names(&self) -> &'static [&'static str; 100] {
        if self.bank == 0 { &BANK_A } else { &BANK_B }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_ten_pages_and_parameters_have_valid_midi_ranges() {
        assert_eq!(PAGES.len(), 10);
        let total: usize = PAGES.iter().map(|page| page.params.len()).sum();
        assert!(
            total >= 100,
            "expected full XioSynth parameter set, got {total}"
        );
        for page in PAGES {
            for param in page.params {
                assert!(param.min <= param.default && param.default <= param.max);
                assert!(param.max <= 127);
                for choice in param.choices {
                    assert!(
                        choice.value >= param.min && choice.value <= param.max,
                        "choice {} out of range for {}",
                        choice.label,
                        param.name
                    );
                }
            }
        }
    }

    #[test]
    fn cc_and_nrpn_messages_match_xiosynth_chart() {
        let ch = Channel::first();
        // Filter Frequency = CC 105
        let filter_freq = &PAGES[5].params[2];
        assert_eq!(
            encode_param(ch, filter_freq, 90),
            vec![MidiMessage::ControlChange {
                channel: ch,
                controller: control_number(105),
                value: 90,
            }]
        );
        // Oscillator 1 Octave "+1" = NRPN 105, value 18
        let osc1_oct = &PAGES[0].params[1];
        assert_eq!(
            encode_param(ch, osc1_oct, 18),
            vec![
                MidiMessage::ControlChange {
                    channel: ch,
                    controller: control_number(99),
                    value: 0,
                },
                MidiMessage::ControlChange {
                    channel: ch,
                    controller: control_number(98),
                    value: 105,
                },
                MidiMessage::ControlChange {
                    channel: ch,
                    controller: control_number(6),
                    value: 18,
                },
            ]
        );
        assert_eq!(osc1_oct.format_value(18), "+1");
        assert_eq!(PAGES[0].params[2].format_value(64), "0 st");
        assert_eq!(PAGES[0].params[2].format_value(70), "+6 st");
    }

    #[test]
    fn bank_and_patch_selection_uses_cc32_and_program_change() {
        let ch = Channel::from_one_based(3).expect("ch 3");
        assert_eq!(BANK_A.len(), 100);
        assert_eq!(BANK_B.len(), 100);
        assert_eq!(BANK_A[0], "Big Bass");
        assert_eq!(BANK_B[23], "TB-303 Bass");
        assert_eq!(
            encode_patch_select(ch, 1, 23),
            vec![
                MidiMessage::ControlChange {
                    channel: ch,
                    controller: control_number(32),
                    value: 1,
                },
                MidiMessage::ProgramChange {
                    channel: ch,
                    program: 23,
                },
            ]
        );
    }

    #[test]
    fn state_defaults_and_page_messages_work() {
        let mut state = XioState::default();
        assert_eq!(state.values.len(), PAGES.len());
        state.values[5][2] = 42;
        state.reset_page(5);
        assert_eq!(state.values[5][2], 127);
        assert!(!state.page_messages(0).is_empty());
    }
}
