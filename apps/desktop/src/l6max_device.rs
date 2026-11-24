//! Zoom LiveTrak L6max mixer-control CC map.
//!
//! Numbers follow the factory map in the device profile and the L6max manual:
//! ten parameters per channel, eight channels, mutes, MONO/USB toggles, effect
//! type, compressor and scenes A-D.

/// Parameter labels in row order.
pub const PARAMETERS: [&str; 10] = [
    "HIGH", "MID FREQ", "MID", "LOW", "SUB", "AUX1", "AUX2", "EFX", "PAN", "LEVEL",
];

/// First controller number for each parameter row.
pub const BASES: [u8; 10] = [1, 11, 21, 33, 41, 49, 57, 65, 73, 81];

/// Mute controllers for CH1..CH8.
pub const MUTES: [u8; 8] = [93, 94, 95, 102, 103, 104, 105, 106];

/// MONO x2 controllers for CH5 and CH6.
pub const MONO: [(usize, u8); 2] = [(4, 109), (5, 110)];

/// USB controllers for CH7 and CH8.
pub const USB: [(usize, u8); 2] = [(6, 113), (7, 114)];

/// Effect type controller.
pub const EFX_TYPE: u8 = 117;

/// Effect type names.
pub const EFX_NAMES: [&str; 6] = ["AI NR", "Hall", "Room", "Spring", "Delay", "Echo"];

/// Effect type controller values.
pub const EFX_VALUES: [u8; 6] = [0, 21, 43, 64, 85, 107];

/// Compressor on/off controller.
pub const COMPRESSOR: u8 = 119;

/// Default value for one parameter row.
#[must_use]
pub fn default_value(row: usize) -> u8 {
    match row {
        9 => 100,
        4..=7 => 0,
        _ => 64,
    }
}

/// Controller number for one parameter and zero-based channel.
#[must_use]
pub fn parameter_cc(row: usize, channel: usize) -> Option<u8> {
    let base = *BASES.get(row)?;
    let channel = u8::try_from(channel).ok()?;
    (channel < 8).then(|| base + channel)
}

/// Mutable mixer state shown by the window.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceState {
    /// Ten parameter values per channel.
    pub values: [[u8; 10]; 8],
    /// Mute per channel.
    pub mutes: [bool; 8],
    /// MONO/USB mode per channel; only channels 5-8 use it.
    pub modes: [bool; 8],
    /// Effect type index.
    pub efx: usize,
    /// Compressor enabled.
    pub compressor: bool,
    /// MIDI channel, 1..=16.
    pub channel: u8,
}

impl Default for DeviceState {
    fn default() -> Self {
        let mut values = [[0_u8; 10]; 8];
        for channel in &mut values {
            for (row, value) in channel.iter_mut().enumerate() {
                *value = default_value(row);
            }
        }
        Self {
            values,
            mutes: [false; 8],
            modes: [false; 8],
            efx: 0,
            compressor: false,
            channel: 1,
        }
    }
}

/// Path for the persisted last-sent values.
pub const STATE_PATH: &str = "l6max-device.json";

/// Reads persisted values, or defaults.
#[must_use]
pub fn load(path: &std::path::Path) -> DeviceState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the values, best effort.
pub fn save(path: &std::path::Path, state: &DeviceState) {
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cc_map_matches_the_device_profile() {
        assert_eq!(parameter_cc(0, 0), Some(1));
        assert_eq!(parameter_cc(0, 7), Some(8));
        assert_eq!(parameter_cc(9, 0), Some(81));
        assert_eq!(parameter_cc(9, 7), Some(88));
        assert_eq!(parameter_cc(4, 2), Some(43));
        assert_eq!(parameter_cc(0, 8), None);
        assert_eq!(MUTES, [93, 94, 95, 102, 103, 104, 105, 106]);
        assert_eq!(MONO, [(4, 109), (5, 110)]);
        assert_eq!(USB, [(6, 113), (7, 114)]);
    }

    #[test]
    fn values_round_trip_through_json() {
        let mut state = DeviceState::default();
        state.values[2][9] = 42;
        state.mutes[4] = true;
        state.modes[6] = true;
        state.efx = 3;
        state.compressor = true;
        state.channel = 7;
        let text = serde_json::to_string(&state).expect("json");
        let back: DeviceState = serde_json::from_str(&text).expect("parse");
        assert_eq!(back, state);
    }

    #[test]
    fn defaults_are_pan_centre_and_unity_level() {
        let state = DeviceState::default();
        assert_eq!(state.values[0][8], 64);
        assert_eq!(state.values[0][9], 100);
        assert_eq!(state.values[0][4], 0);
        assert_eq!(state.channel, 1);
    }
}
