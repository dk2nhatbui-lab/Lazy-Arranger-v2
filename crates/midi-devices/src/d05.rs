//! Roland D-05 / D-50 System Exclusive protocol and temporary patch model.
//!
//! Uses the public Roland D-50 / D-05 MIDI Implementation:
//! - Manufacturer ID `0x41` (Roland)
//! - Device ID `0x00..=0x1F` (default `0x00`, channel 1)
//! - Model ID `0x14` (D-50 / D-550 / D-05)
//! - Commands: `0x11` (RQ1 data request) and `0x12` (DT1 data set)
//! - 3-byte 7-bit addressing (`aa bb cc`) and Roland 7-bit checksum.

/// Roland SysEx manufacturer ID.
pub const ROLAND_ID: u8 = 0x41;
/// Roland D-50 / D-05 model ID.
pub const MODEL_ID: u8 = 0x14;
/// Roland Request Data 1 command byte.
pub const CMD_RQ1: u8 = 0x11;
/// Roland Data Set 1 command byte.
pub const CMD_DT1: u8 = 0x12;
/// Roland Handshake Want to Send Data command byte.
pub const CMD_WSD: u8 = 0x40;
/// Roland Handshake Request Data command byte.
pub const CMD_RQD: u8 = 0x41;
/// Roland Handshake Data packet command byte.
pub const CMD_DAT: u8 = 0x42;
/// Roland Handshake Acknowledge command byte.
pub const CMD_ACK: u8 = 0x43;
/// Roland Handshake End of Data command byte.
pub const CMD_EOD: u8 = 0x45;
/// Roland Handshake Communication Error command byte.
pub const CMD_ERR: u8 = 0x4e;
/// Roland Handshake Rejection command byte.
pub const CMD_RJC: u8 = 0x4f;

/// Universal Non-Realtime Identity Request (`F0 7E 7F 06 01 F7`).
pub const IDENTITY_REQUEST: [u8; 6] = [0xf0, 0x7e, 0x7f, 0x06, 0x01, 0xf7];

/// Temporary edit-buffer size in bytes (7 blocks of 64 bytes).
pub const TEMP_PATCH_BYTES: usize = 448;
/// Maximum payload per Roland D-50/D-05 DT1 packet.
pub const MAX_DT1_PAYLOAD: usize = 256;

/// Temporary area block base addresses.
pub const ADDR_UPPER_PARTIAL_1: [u8; 3] = [0x00, 0x00, 0x00];
/// Temporary area Upper Partial 2 base address.
pub const ADDR_UPPER_PARTIAL_2: [u8; 3] = [0x00, 0x00, 0x40];
/// Temporary area Upper Tone Common base address.
pub const ADDR_UPPER_COMMON: [u8; 3] = [0x00, 0x01, 0x00];
/// Temporary area Lower Partial 1 base address.
pub const ADDR_LOWER_PARTIAL_1: [u8; 3] = [0x00, 0x01, 0x40];
/// Temporary area Lower Partial 2 base address.
pub const ADDR_LOWER_PARTIAL_2: [u8; 3] = [0x00, 0x02, 0x00];
/// Temporary area Lower Tone Common base address.
pub const ADDR_LOWER_COMMON: [u8; 3] = [0x00, 0x02, 0x40];
/// Temporary area Patch Common base address.
pub const ADDR_PATCH_COMMON: [u8; 3] = [0x00, 0x03, 0x00];
/// Internal patch memory base address (64 patches of 448 bytes each).
pub const ADDR_INTERNAL_MEMORY: [u8; 3] = [0x02, 0x00, 0x00];

/// Patch common key-mode names.
pub const KEY_MODES: [&str; 9] = [
    "Whole",
    "Dual",
    "Split",
    "Separate",
    "Whole-S",
    "Dual-S",
    "Split-U",
    "Split-L",
    "Separate-S",
];

/// Patch common output-mode names.
pub const OUTPUT_MODES: [&str; 4] = ["Mode 1", "Mode 2", "Mode 3", "Mode 4"];

/// Patch common chase-mode names.
pub const CHASE_MODES: [&str; 3] = ["U+L", "Upper", "Lower"];

/// D-50 / D-05 64-entry character table used for patch and tone names.
const CHAR_TABLE: [char; 64] = [
    ' ', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R',
    'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k',
    'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3',
    '4', '5', '6', '7', '8', '9', '-',
];

/// Roland 7-bit checksum over a 3-byte address and payload.
#[must_use]
pub fn checksum(address: [u8; 3], payload: &[u8]) -> u8 {
    crate::sysex::checksum(address, payload)
}

/// Converts a 3-byte 7-bit Roland address into a linear offset.
#[must_use]
pub fn address_to_offset(address: [u8; 3]) -> usize {
    (usize::from(address[0]) << 14) | (usize::from(address[1]) << 7) | usize::from(address[2])
}

/// Converts a linear offset into a 3-byte 7-bit Roland address.
#[must_use]
pub fn offset_to_address(offset: usize) -> [u8; 3] {
    [
        ((offset >> 14) & 0x7f) as u8,
        ((offset >> 7) & 0x7f) as u8,
        (offset & 0x7f) as u8,
    ]
}

/// Adds a linear byte offset to a 3-byte 7-bit Roland address.
#[must_use]
pub fn address_add(base: [u8; 3], offset: usize) -> [u8; 3] {
    offset_to_address(address_to_offset(base).saturating_add(offset))
}

/// Decodes D-05/D-50 name bytes into a trimmed UTF-8 string.
#[must_use]
pub fn decode_name(bytes: &[u8]) -> String {
    let text: String = bytes
        .iter()
        .map(|&byte| CHAR_TABLE.get(usize::from(byte)).copied().unwrap_or(' '))
        .collect();
    text.trim_end().to_string()
}

/// Encodes a name string into `len` D-05/D-50 character-table bytes.
#[must_use]
pub fn encode_name(name: &str, len: usize) -> Vec<u8> {
    let mut out = vec![0_u8; len];
    for (slot, ch) in out.iter_mut().zip(name.chars()) {
        *slot = CHAR_TABLE
            .iter()
            .position(|&candidate| candidate == ch)
            .map_or(0, |index| index as u8);
    }
    out
}

/// Builds a Roland D-05/D-50 RQ1 (Request Data 1) SysEx message.
///
/// # Errors
///
/// Returns an error if `device > 31` or any address/size byte exceeds 127.
pub fn rq1(device: u8, address: [u8; 3], size: [u8; 3]) -> Result<Vec<u8>, String> {
    if device > 31 {
        return Err(String::from("Roland device ID must be 0..=31"));
    }
    if address.iter().chain(size.iter()).any(|&byte| byte > 127) {
        return Err(String::from("address and size bytes must be 0..=127"));
    }
    let sum = checksum(address, &size);
    Ok(vec![
        0xf0, ROLAND_ID, device, MODEL_ID, CMD_RQ1, address[0], address[1], address[2], size[0],
        size[1], size[2], sum, 0xf7,
    ])
}

/// Builds a Roland D-05/D-50 DT1 (Data Set 1) SysEx message.
///
/// # Errors
///
/// Returns an error if `device > 31`, `data` is empty, or any byte exceeds 127.
pub fn dt1(device: u8, address: [u8; 3], data: &[u8]) -> Result<Vec<u8>, String> {
    if device > 31 {
        return Err(String::from("Roland device ID must be 0..=31"));
    }
    if data.is_empty() {
        return Err(String::from("DT1 data must not be empty"));
    }
    if address.iter().chain(data.iter()).any(|&byte| byte > 127) {
        return Err(String::from("address and data bytes must be 0..=127"));
    }
    let sum = checksum(address, data);
    let mut out = Vec::with_capacity(10 + data.len());
    out.extend_from_slice(&[
        0xf0, ROLAND_ID, device, MODEL_ID, CMD_DT1, address[0], address[1], address[2],
    ]);
    out.extend_from_slice(data);
    out.push(sum);
    out.push(0xf7);
    Ok(out)
}

/// A parsed Roland D-05/D-50 DT1 SysEx message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dt1Message {
    /// Device ID byte (`0x00..=0x1F`).
    pub device: u8,
    /// 3-byte Roland parameter address.
    pub address: [u8; 3],
    /// Payload bytes.
    pub data: Vec<u8>,
}

/// Builds a Roland D-05/D-50 RQD (Handshake Request Data) SysEx message.
///
/// # Errors
///
/// Returns an error if `device > 31` or any address/size byte exceeds 127.
pub fn rqd(device: u8, address: [u8; 3], size: [u8; 3]) -> Result<Vec<u8>, String> {
    if device > 31 {
        return Err(String::from("Roland device ID must be 0..=31"));
    }
    if address.iter().chain(size.iter()).any(|&byte| byte > 127) {
        return Err(String::from("address and size bytes must be 0..=127"));
    }
    let sum = checksum(address, &size);
    Ok(vec![
        0xf0, ROLAND_ID, device, MODEL_ID, CMD_RQD, address[0], address[1], address[2], size[0],
        size[1], size[2], sum, 0xf7,
    ])
}

/// Builds a Roland D-05/D-50 ACK (Acknowledge) SysEx message.
#[must_use]
pub fn ack(device: u8) -> Vec<u8> {
    vec![0xf0, ROLAND_ID, device.min(31), MODEL_ID, CMD_ACK, 0xf7]
}

/// Parses and checksum-verifies a Roland D-05/D-50 DT1 or DAT message.
#[must_use]
pub fn parse_data_packet(bytes: &[u8]) -> Option<Dt1Message> {
    if bytes.len() < 11 {
        return None;
    }
    if bytes[0] != 0xf0
        || *bytes.last()? != 0xf7
        || bytes[1] != ROLAND_ID
        || bytes[3] != MODEL_ID
        || (bytes[4] != CMD_DT1 && bytes[4] != CMD_DAT)
    {
        return None;
    }
    // Everything between F0 and F7 must be 7-bit, device ids are 0..=31.
    if bytes[1..bytes.len() - 1].iter().any(|byte| *byte > 0x7f) || bytes[2] > 31 {
        return None;
    }
    let address = [bytes[5], bytes[6], bytes[7]];
    let data = &bytes[8..bytes.len() - 2];
    let expected = checksum(address, data);
    if bytes[bytes.len() - 2] != expected {
        return None;
    }
    Some(Dt1Message {
        device: bytes[2],
        address,
        data: data.to_vec(),
    })
}

/// Parses and checksum-verifies a Roland D-05/D-50 DT1 message.
#[must_use]
pub fn parse_dt1(bytes: &[u8]) -> Option<Dt1Message> {
    if bytes.get(4) != Some(&CMD_DT1) {
        return None;
    }
    parse_data_packet(bytes)
}

/// Returns true when `bytes` is a Roland D-05/D-50 End-of-Data (`EOD`) message.
#[must_use]
pub fn is_eod(bytes: &[u8]) -> bool {
    bytes.len() == 6
        && bytes[0] == 0xf0
        && bytes[1] == ROLAND_ID
        && bytes[3] == MODEL_ID
        && bytes[4] == CMD_EOD
        && bytes[5] == 0xf7
}

/// Returns true when `bytes` is a Roland D-05/D-50 Reject (`RJC`) or Error (`ERR`).
#[must_use]
pub fn is_reject_or_error(bytes: &[u8]) -> bool {
    bytes.len() == 6
        && bytes[0] == 0xf0
        && bytes[1] == ROLAND_ID
        && bytes[3] == MODEL_ID
        && (bytes[4] == CMD_RJC || bytes[4] == CMD_ERR)
        && bytes[5] == 0xf7
}

/// Parses a Universal Non-Realtime Identity Reply (`F0 7E dev 06 02 ... F7`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityReply {
    /// Device ID byte.
    pub device: u8,
    /// Manufacturer ID bytes (1 byte, or 3 bytes when starting with `0x00`).
    pub manufacturer: Vec<u8>,
    /// 2-byte family code (LSB, MSB).
    pub family: [u8; 2],
    /// 2-byte member/model code (LSB, MSB).
    pub member: [u8; 2],
    /// 4-byte software revision bytes.
    pub revision: [u8; 4],
}

impl IdentityReply {
    /// Parses a Universal Identity Reply packet.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 15
            || bytes[0] != 0xf0
            || *bytes.last()? != 0xf7
            || bytes[1] != 0x7e
            || bytes[3] != 0x06
            || bytes[4] != 0x02
        {
            return None;
        }
        let (manufacturer, rest) = if bytes[5] == 0x00 {
            if bytes.len() < 17 {
                return None;
            }
            (bytes[5..8].to_vec(), &bytes[8..bytes.len() - 1])
        } else {
            (vec![bytes[5]], &bytes[6..bytes.len() - 1])
        };
        if rest.len() < 8 {
            return None;
        }
        Some(Self {
            device: bytes[2],
            manufacturer,
            family: [rest[0], rest[1]],
            member: [rest[2], rest[3]],
            revision: [rest[4], rest[5], rest[6], rest[7]],
        })
    }

    /// Formats the revision bytes as ASCII when printable, otherwise hex.
    #[must_use]
    pub fn revision_text(&self) -> String {
        if self
            .revision
            .iter()
            .all(|byte| (0x20..=0x7e).contains(byte))
        {
            String::from_utf8_lossy(&self.revision).into_owned()
        } else {
            self.revision
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

/// Roland D-05 Bank Select MSB (CC 0).
pub const BANK_SELECT_MSB: u8 = 87;

/// Roland D-05 bank names indexed by Bank Select LSB (`0..=13`).
pub const BANK_NAMES: [&str; 14] = [
    "Internal 1 (U1)",
    "Internal 2 (U2)",
    "Internal 3 (U3)",
    "Internal 4 (U4)",
    "Internal 5 (U5)",
    "Internal 6 (U6)",
    "Internal 7 (U7)",
    "Internal 8 (U8)",
    "Preset 1 (PR 1)",
    "Preset 2 (PR 2)",
    "Preset 3 (PR 3)",
    "Preset 4 (PR 4)",
    "Preset 5 (PR 5)",
    "Preset 6 (PR 6)",
];

/// Builds the CC 0 / CC 32 Bank Select messages for a D-05 bank.
///
/// `bank_lsb` is `0..=7` for Internal 1–8 and `8..=13` for Preset 1–6;
/// `channel` is zero-based `0..=15`.
#[must_use]
pub fn bank_select_messages(channel: u8, bank_lsb: u8) -> [[u8; 3]; 2] {
    let ch = channel.min(15);
    [
        [0xb0 | ch, 0, BANK_SELECT_MSB],
        [0xb0 | ch, 32, bank_lsb.min(13)],
    ]
}

/// Builds the 2-byte Program Change message for a D-05 patch (`0..=63`).
#[must_use]
pub fn program_change_message(channel: u8, program: u8) -> [u8; 2] {
    [0xc0 | channel.min(15), program.min(63)]
}

/// Returns true when `key_mode` is Separate (`3`) or Separate-S (`8`).
#[must_use]
pub fn is_separate_mode(key_mode: u8) -> bool {
    matches!(key_mode, 3 | 8)
}

/// One-based MIDI channel used for the lower tone in Separate mode (next channel, wrapping 16 -> 1).
#[must_use]
pub fn separate_lower_channel(channel_one_based: u8) -> u8 {
    (channel_one_based.clamp(1, 16) % 16) + 1
}

/// Human-readable label for a D-05 reverb type (`0..=31`, including user slots 17–32).
#[must_use]
pub fn reverb_slot_label(reverb_type: u8) -> String {
    let slot = reverb_type.min(31) + 1;
    if slot >= 17 {
        format!("User {slot}")
    } else {
        format!("Preset {slot}")
    }
}

/// Builds a single-parameter DT1 message for an offset in `0..TEMP_PATCH_BYTES`.
///
/// # Errors
///
/// Returns an error if `offset >= TEMP_PATCH_BYTES`, `device > 31`, or `value > 127`.
pub fn parameter_dt1(device: u8, offset: usize, value: u8) -> Result<Vec<u8>, String> {
    if offset >= TEMP_PATCH_BYTES {
        return Err(format!("D-05 temp patch offset {offset} out of range"));
    }
    dt1(device, offset_to_address(offset), &[value])
}

/// Builds DT1 messages for Upper Tone Name, Lower Tone Name, and Patch Name.
///
/// # Errors
///
/// Returns an error if `device > 31`.
pub fn send_names_dt1(device: u8, patch: &TempPatch) -> Result<Vec<Vec<u8>>, String> {
    Ok(vec![
        dt1(device, ADDR_UPPER_COMMON, &patch.bytes[128..128 + 10])?,
        dt1(device, ADDR_LOWER_COMMON, &patch.bytes[320..320 + 10])?,
        dt1(device, ADDR_PATCH_COMMON, &patch.bytes[384..384 + 18])?,
    ])
}

/// Reads the active 448-byte temporary patch from a live Roland D-05 / D-50.
///
/// # Errors
///
/// Returns an error if MIDI ports fail or no DT1 packets arrive.
pub fn read_temp_patch(
    input_name: &str,
    output_name: &str,
    device: u8,
) -> Result<(TempPatch, Vec<u8>), String> {
    let requests = TempPatch::request_messages(device)?;
    let mut patch = TempPatch {
        bytes: [0; TEMP_PATCH_BYTES],
    };
    let mut raw = Vec::new();
    let mut written = 0_usize;
    for request in &requests {
        let replies = crate::devices::sysex_exchange(
            input_name,
            output_name,
            request,
            std::time::Duration::from_millis(1500),
            std::time::Duration::from_millis(250),
        )
        .map_err(|err| err.to_string())?;
        for reply in replies {
            raw.extend_from_slice(&reply);
            if let Some(dt1) = parse_dt1(&reply) {
                written += patch.apply_dt1(&dt1);
            }
        }
    }
    if written == 0 {
        return Err(format!(
            "no D-05 DT1 reply received on {input_name} (device {device})"
        ));
    }
    Ok((patch, raw))
}

/// Sends a full `TempPatch` to a live Roland D-05 / D-50 as chunked DT1 packets.
///
/// # Errors
///
/// Returns an error if encoding fails or the MIDI output port cannot be written.
pub fn write_temp_patch(output_name: &str, device: u8, patch: &TempPatch) -> Result<usize, String> {
    let packets = patch.to_dt1_messages(device)?;
    let mut output = crate::devices::open_output(output_name).map_err(|err| err.to_string())?;
    for packet in &packets {
        output.send(packet).map_err(|err| err.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(packets.len())
}

/// Reads `count` (`1..=64`) patch names from one Roland D-05 bank (`bank_lsb`
/// `0..=13`) by selecting each patch on `channel`, requesting the 18-byte
/// temporary patch name via `RQ1`, and restoring the original temporary patch
/// afterward.
///
/// # Errors
///
/// Returns an error when MIDI ports fail or no patch names are returned.
pub fn read_bank_names(
    input_name: &str,
    output_name: &str,
    device: u8,
    channel: u8,
    bank_lsb: u8,
    count: usize,
) -> Result<Vec<String>, String> {
    use std::sync::mpsc::{RecvTimeoutError, channel as mpsc_channel};
    use std::time::{Duration, Instant};

    let count = count.clamp(1, 64);
    let saved = read_temp_patch(input_name, output_name, device).ok();
    let name_req = rq1(device, ADDR_PATCH_COMMON, [0x00, 0x00, 0x12])?;

    let mut input =
        midir::MidiInput::new("lazy-arranger-d05-names").map_err(|err| err.to_string())?;
    input.ignore(midir::Ignore::TimeAndActiveSense);
    let ports = input.ports();
    let port = ports
        .into_iter()
        .find(|p| input.port_name(p).ok().as_deref() == Some(input_name))
        .ok_or_else(|| format!("MIDI input port {input_name:?} not found"))?;
    let (sender, receiver) = mpsc_channel::<Vec<u8>>();
    let _conn = input
        .connect(
            &port,
            "lazy-arranger-d05-names-in",
            move |_ts, bytes, _| {
                if !bytes.is_empty() {
                    let _ = sender.send(bytes.to_vec());
                }
            },
            (),
        )
        .map_err(|err| err.to_string())?;
    let mut output = crate::devices::open_output(output_name).map_err(|err| err.to_string())?;

    let mut names = Vec::with_capacity(count);
    let mut pending = Vec::new();
    let mut batch = Vec::new();
    for program in 0..count {
        let bank_msgs = bank_select_messages(channel, bank_lsb);
        for msg in bank_msgs {
            output.send(&msg).map_err(|err| err.to_string())?;
        }
        output
            .send(&program_change_message(channel, program as u8))
            .map_err(|err| err.to_string())?;
        std::thread::sleep(Duration::from_millis(25));
        while receiver.try_recv().is_ok() {}
        pending.clear();
        output.send(&name_req).map_err(|err| err.to_string())?;

        let deadline = Instant::now() + Duration::from_millis(400);
        let mut found_name = None;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match receiver.recv_timeout(remaining) {
                Ok(chunk) => {
                    batch.clear();
                    crate::devices::collect_sysex_chunks(&mut pending, &chunk, &mut batch);
                    for msg in batch.drain(..) {
                        if let Some(dt1) = parse_dt1(&msg)
                            && dt1.address == ADDR_PATCH_COMMON
                            && dt1.data.len() >= 18
                        {
                            found_name = Some(decode_name(&dt1.data[..18]));
                            break;
                        }
                    }
                    if found_name.is_some() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        if let Some(name) = found_name {
            names.push(name);
        } else {
            break;
        }
    }
    drop(_conn);
    drop(output);
    if let Some((original, _)) = saved {
        let _ = write_temp_patch(output_name, device, &original);
    }
    if names.is_empty() {
        return Err(format!(
            "no D-05 patch names received from bank {bank_lsb} on {input_name}"
        ));
    }
    Ok(names)
}

/// Reads a Roland D-05/D-50 memory region using the `RQD`/`DAT`/`ACK`/`EOD`
/// handshake protocol, returning all decoded data packets.
///
/// # Errors
///
/// Returns an error when the MIDI ports fail, the device rejects the request,
/// or no data packets arrive before the timeout.
pub fn handshake_read(
    input_name: &str,
    output_name: &str,
    device: u8,
    address: [u8; 3],
    size: [u8; 3],
    timeout: std::time::Duration,
) -> Result<Vec<Dt1Message>, String> {
    let request = rqd(device, address, size)?;
    let ack_bytes = ack(device);
    let packets = crate::devices::sysex_handshake(
        input_name,
        output_name,
        &request,
        &ack_bytes,
        timeout,
        is_eod,
        is_reject_or_error,
    )
    .map_err(|err| err.to_string())?;
    Ok(packets
        .iter()
        .filter_map(|packet| parse_data_packet(packet))
        .collect())
}

/// Address of patch `index` (`0..64`) inside internal memory (`02 00 00`).
#[must_use]
pub fn internal_patch_address(index: usize) -> [u8; 3] {
    address_add(ADDR_INTERNAL_MEMORY, index * TEMP_PATCH_BYTES)
}

/// Address of the 18-byte patch name for internal patch `index` (`0..64`).
#[must_use]
pub fn internal_patch_name_address(index: usize) -> [u8; 3] {
    address_add(internal_patch_address(index), 384)
}

/// Full 448-byte Roland D-05/D-50 temporary edit buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TempPatch {
    /// Raw 7 x 64-byte blocks (`00 00 00` through `00 03 3F`).
    pub bytes: [u8; TEMP_PATCH_BYTES],
}

impl Default for TempPatch {
    fn default() -> Self {
        let mut patch = Self {
            bytes: [0; TEMP_PATCH_BYTES],
        };
        patch.set_patch_name("Init Patch");
        patch.set_upper_tone_name("Upper");
        patch.set_lower_tone_name("Lower");
        patch.set_patch_common(0x20, 100); // Total volume
        patch.set_patch_common(0x21, 50); // Tone balance
        patch
    }
}

impl TempPatch {
    /// Builds RQ1 requests covering the full 448-byte temporary patch buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when `device > 31`.
    pub fn request_messages(device: u8) -> Result<Vec<Vec<u8>>, String> {
        Ok(vec![rq1(
            device,
            ADDR_UPPER_PARTIAL_1,
            offset_to_address(TEMP_PATCH_BYTES),
        )?])
    }

    /// Applies one DT1 packet into the temporary patch buffer if its address
    /// falls within `00 00 00..00 03 40`. Returns the number of bytes written.
    pub fn apply_dt1(&mut self, message: &Dt1Message) -> usize {
        let start = address_to_offset(message.address);
        if start >= TEMP_PATCH_BYTES {
            return 0;
        }
        let end = (start + message.data.len()).min(TEMP_PATCH_BYTES);
        let count = end - start;
        self.bytes[start..end].copy_from_slice(&message.data[..count]);
        count
    }

    /// Encodes the temporary patch as chunked DT1 messages (<= 256 bytes each).
    ///
    /// # Errors
    ///
    /// Returns an error when `device > 31` or any byte exceeds 127.
    pub fn to_dt1_messages(&self, device: u8) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        let mut offset = 0;
        while offset < TEMP_PATCH_BYTES {
            let len = (TEMP_PATCH_BYTES - offset).min(MAX_DT1_PAYLOAD);
            out.push(dt1(
                device,
                offset_to_address(offset),
                &self.bytes[offset..offset + len],
            )?);
            offset += len;
        }
        Ok(out)
    }

    /// Serializes the temporary patch as concatenated `.syx` DT1 packets.
    ///
    /// # Errors
    ///
    /// Returns an error if `device > 31` or any byte exceeds 127.
    pub fn to_syx_bytes(&self, device: u8) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        for packet in self.to_dt1_messages(device)? {
            out.extend_from_slice(&packet);
        }
        Ok(out)
    }

    /// Parses one or more Roland D-05/D-50 DT1/DAT packets from `.syx` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when no valid D-05 temporary-patch packet is found.
    pub fn from_syx_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut pending = Vec::new();
        let mut packets = Vec::new();
        crate::devices::collect_sysex_chunks(&mut pending, bytes, &mut packets);
        let mut patch = Self {
            bytes: [0; TEMP_PATCH_BYTES],
        };
        let mut written = 0_usize;
        for packet in &packets {
            if let Some(msg) = parse_data_packet(packet) {
                written += patch.apply_dt1(&msg);
            }
        }
        if written == 0 {
            return Err(String::from(
                "no valid Roland D-05 temporary-patch DT1 packets found",
            ));
        }
        Ok(patch)
    }

    /// Reads one byte from the Patch Common block (`00 03 00`, `offset < 64`).
    #[must_use]
    pub fn patch_common(&self, offset: usize) -> u8 {
        self.bytes.get(384 + offset).copied().unwrap_or(0)
    }

    /// Writes one byte in the Patch Common block (`00 03 00`, `offset < 64`).
    pub fn set_patch_common(&mut self, offset: usize, value: u8) {
        if offset < 64 {
            self.bytes[384 + offset] = value.min(127);
        }
    }

    /// Reads one byte from Upper Common (`00 01 00`, `offset < 64`).
    #[must_use]
    pub fn upper_common(&self, offset: usize) -> u8 {
        self.bytes.get(128 + offset).copied().unwrap_or(0)
    }

    /// Writes one byte in Upper Common (`00 01 00`, `offset < 64`).
    pub fn set_upper_common(&mut self, offset: usize, value: u8) {
        if offset < 64 {
            self.bytes[128 + offset] = value.min(127);
        }
    }

    /// Reads one byte from Lower Common (`00 02 40`, `offset < 64`).
    #[must_use]
    pub fn lower_common(&self, offset: usize) -> u8 {
        self.bytes.get(320 + offset).copied().unwrap_or(0)
    }

    /// Writes one byte in Lower Common (`00 02 40`, `offset < 64`).
    pub fn set_lower_common(&mut self, offset: usize, value: u8) {
        if offset < 64 {
            self.bytes[320 + offset] = value.min(127);
        }
    }

    /// Decoded 18-character patch name.
    #[must_use]
    pub fn patch_name(&self) -> String {
        decode_name(&self.bytes[384..384 + 18])
    }

    /// Sets the 18-character patch name.
    pub fn set_patch_name(&mut self, name: &str) {
        let encoded = encode_name(name, 18);
        self.bytes[384..384 + 18].copy_from_slice(&encoded);
    }

    /// Decoded 10-character Upper Tone name.
    #[must_use]
    pub fn upper_tone_name(&self) -> String {
        decode_name(&self.bytes[128..128 + 10])
    }

    /// Sets the 10-character Upper Tone name.
    pub fn set_upper_tone_name(&mut self, name: &str) {
        let encoded = encode_name(name, 10);
        self.bytes[128..128 + 10].copy_from_slice(&encoded);
    }

    /// Decoded 10-character Lower Tone name.
    #[must_use]
    pub fn lower_tone_name(&self) -> String {
        decode_name(&self.bytes[320..320 + 10])
    }

    /// Sets the 10-character Lower Tone name.
    pub fn set_lower_tone_name(&mut self, name: &str) {
        let encoded = encode_name(name, 10);
        self.bytes[320..320 + 10].copy_from_slice(&encoded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_and_messages_follow_roland_d50_spec() {
        let req = rq1(0, ADDR_PATCH_COMMON, [0x00, 0x00, 0x40]).expect("rq1");
        assert_eq!(
            req,
            vec![
                0xf0, 0x41, 0x00, 0x14, 0x11, 0x00, 0x03, 0x00, 0x00, 0x00, 0x40, 0x3d, 0xf7
            ]
        );
        let msg = dt1(0, ADDR_PATCH_COMMON, &[1, 2, 3]).expect("dt1");
        let parsed = parse_dt1(&msg).expect("parse");
        assert_eq!(parsed.device, 0);
        assert_eq!(parsed.address, ADDR_PATCH_COMMON);
        assert_eq!(parsed.data, vec![1, 2, 3]);
    }

    #[test]
    fn names_round_trip_through_d50_character_table() {
        let encoded = encode_name("Fantasia D-50", 18);
        assert_eq!(encoded.len(), 18);
        assert_eq!(decode_name(&encoded), "Fantasia D-50");
    }

    #[test]
    fn temp_patch_round_trips_through_chunked_dt1() {
        let mut original = TempPatch::default();
        original.set_patch_name("Glass Voices");
        original.set_upper_tone_name("Chime");
        original.set_lower_tone_name("Warm Pad");
        original.set_patch_common(0x12, 3); // Separate
        original.set_patch_common(0x1e, 20); // User reverb slot
        let packets = original.to_dt1_messages(0).expect("packets");
        assert_eq!(packets.len(), 2);
        let mut rebuilt = TempPatch {
            bytes: [0; TEMP_PATCH_BYTES],
        };
        for packet in packets {
            let parsed = parse_dt1(&packet).expect("parse");
            assert!(rebuilt.apply_dt1(&parsed) > 0);
        }
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn address_math_handles_seven_bit_boundaries() {
        assert_eq!(offset_to_address(TEMP_PATCH_BYTES), [0x00, 0x03, 0x40]);
        assert_eq!(address_to_offset([0x00, 0x03, 0x40]), 448);
        assert_eq!(internal_patch_address(0), [0x02, 0x00, 0x00]);
        assert_eq!(internal_patch_address(1), [0x02, 0x03, 0x40]);
        assert_eq!(internal_patch_name_address(0), [0x02, 0x03, 0x00]);
    }

    #[test]
    fn syx_dump_names_and_modes_round_trip() {
        let mut patch = TempPatch::default();
        patch.set_patch_name("Neo Horizon");
        patch.set_upper_tone_name("Bass Fx");
        patch.set_lower_tone_name("SweepPad");
        let syx = patch.to_syx_bytes(0).expect("syx");
        let back = TempPatch::from_syx_bytes(&syx).expect("from_syx");
        assert_eq!(back, patch);

        let name_msgs = send_names_dt1(0, &patch).expect("names");
        assert_eq!(name_msgs.len(), 3);
        let parsed = parse_dt1(&name_msgs[2]).expect("patch name");
        assert_eq!(parsed.address, ADDR_PATCH_COMMON);
        assert_eq!(decode_name(&parsed.data), "Neo Horizon");

        let single = parameter_dt1(0, 384 + 0x1e, 23).expect("param");
        let parsed = parse_dt1(&single).expect("parsed");
        assert_eq!(parsed.address, [0x00, 0x03, 0x1e]);
        assert_eq!(parsed.data, vec![23]);

        assert!(is_separate_mode(3));
        assert!(is_separate_mode(8));
        assert!(!is_separate_mode(1));
        assert_eq!(separate_lower_channel(1), 2);
        assert_eq!(separate_lower_channel(16), 1);
        assert_eq!(reverb_slot_label(0), "Preset 1");
        assert_eq!(reverb_slot_label(16), "User 17");
        assert_eq!(reverb_slot_label(31), "User 32");
    }

    #[test]
    fn identity_reply_parses_live_l6max_packet() {
        let packet = [
            0xf0, 0x7e, 0x00, 0x06, 0x02, 0x52, 0x72, 0x00, 0x0d, 0x00, 0x30, 0x31, 0x31, 0x30,
            0xf7,
        ];
        let id = IdentityReply::parse(&packet).expect("identity");
        assert_eq!(id.device, 0);
        assert_eq!(id.manufacturer, vec![0x52]);
        assert_eq!(id.family, [0x72, 0x00]);
        assert_eq!(id.member, [0x0d, 0x00]);
        assert_eq!(id.revision_text(), "0110");
    }

    #[test]
    fn malformed_d05_sysex_and_identity_inputs_never_panic() {
        let crafted: &[&[u8]] = &[
            &[],
            &[0xf0],
            &[0xf0, 0xf7],
            &[0xf0, 0x41, 0x00, 0x14, 0x12, 0xf7],
            &[0xf0, 0x41, 0x00, 0x14, 0x12, 0x7f, 0x7f, 0x7f, 0x00, 0xf7],
            &[0xf0, 0x7e, 0x00, 0x06, 0x02, 0x00, 0xf7],
            &[0xf0, 0x7e, 0x7f, 0x06, 0x02, 0x00, 0x01, 0x02, 0xf7],
        ];
        for bytes in crafted {
            let _ = parse_dt1(bytes);
            let _ = parse_data_packet(bytes);
            let _ = TempPatch::from_syx_bytes(bytes);
            let _ = IdentityReply::parse(bytes);
        }

        let mut seed = 0xD050_5EED_1234_5678_u64;
        for case in 0..256_usize {
            let len = (case * 19) % 512;
            let mut buf = vec![0_u8; len];
            for byte in &mut buf {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                *byte = (seed >> 56) as u8;
            }
            if buf.len() >= 2 {
                buf[0] = 0xf0;
                *buf.last_mut().expect("non-empty") = 0xf7;
            }
            let _ = parse_dt1(&buf);
            let _ = parse_data_packet(&buf);
            let _ = TempPatch::from_syx_bytes(&buf);
            let _ = IdentityReply::parse(&buf);
        }
    }
}
