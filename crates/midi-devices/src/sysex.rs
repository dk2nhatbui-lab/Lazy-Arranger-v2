//! Yamaha-style SysEx parameter changes.
//!
//! The builder is generic: a device number, a three-byte address, a data block
//! and the standard 7-bit checksum. `xg_on` is the one well-known message this
//! build sends; device-specific editor dumps are not implemented.

/// The 7-bit checksum Yamaha uses: the address and data bytes summed, then
/// negated within the 7-bit range.
#[must_use]
pub fn checksum(address: [u8; 3], data: &[u8]) -> u8 {
    let sum = address
        .iter()
        .chain(data.iter())
        .fold(0_u32, |sum, byte| sum + u32::from(*byte));
    ((128 - (sum % 128)) % 128) as u8
}

/// One parameter change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterChange {
    /// Device number, 0..=15 written as `0x1n`.
    pub device: u8,
    /// Parameter address.
    pub address: [u8; 3],
    /// Data bytes, each 0..=127.
    pub data: Vec<u8>,
}

impl ParameterChange {
    /// Builds the checksummed message from the start byte to `F7`.
    ///
    /// # Errors
    ///
    /// Returns a message when the device is above 15 or a byte is above 127.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        if self.device > 15 {
            return Err(String::from("device number must be 0..=15"));
        }
        if self
            .address
            .iter()
            .chain(self.data.iter())
            .any(|byte| *byte > 127)
        {
            return Err(String::from("address and data bytes must be 0..=127"));
        }
        let mut bytes = vec![0xf0, 0x43, 0x10 | self.device, 0x4c];
        bytes.extend_from_slice(&self.address);
        bytes.extend_from_slice(&self.data);
        bytes.push(checksum(self.address, &self.data));
        bytes.push(0xf7);
        Ok(bytes)
    }
}

/// The XG System On message for a device.
#[must_use]
pub fn xg_on(device: u8) -> Vec<u8> {
    ParameterChange {
        device: device.min(15),
        address: [0x00, 0x00, 0x7e],
        data: vec![0x00],
    }
    .to_bytes()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_matches_the_xg_on_example() {
        assert_eq!(checksum([0x00, 0x00, 0x7e], &[0x00]), 2);
        assert_eq!(checksum([0x08, 0x00, 0x00], &[0x40]), 56);
        assert_eq!(
            xg_on(0),
            vec![0xf0, 0x43, 0x10, 0x4c, 0x00, 0x00, 0x7e, 0x00, 0x02, 0xf7]
        );
        assert_eq!(xg_on(3)[2], 0x13);
    }

    #[test]
    fn messages_carry_the_checksum_and_end_byte() {
        let message = ParameterChange {
            device: 1,
            address: [0x08, 0x00, 0x00],
            data: vec![0x40],
        };
        let bytes = message.to_bytes().expect("message");
        assert_eq!(bytes.first(), Some(&0xf0));
        assert_eq!(bytes.last(), Some(&0xf7));
        assert_eq!(bytes.len(), 4 + 3 + 1 + 1 + 1);
        assert_eq!(bytes[8], checksum([0x08, 0x00, 0x00], &[0x40]));
        assert!(
            ParameterChange {
                device: 16,
                address: [0, 0, 0],
                data: vec![]
            }
            .to_bytes()
            .is_err()
        );
    }
}
