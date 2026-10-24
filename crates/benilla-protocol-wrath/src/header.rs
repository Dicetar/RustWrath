//! Packet framing per the local sources: the six-byte client header and the two-or-three-byte
//! server header, sizes exactly as AzerothCore writes and validates them.

/// The client→server header (`WorldSocket.h:58`, `WorldSocket.cpp:305-329`): a six-byte block,
/// encrypted once the session crypto is up, holding a big-endian `u16` size that counts the
/// payload plus four, and a little-endian `u32` opcode. Valid sizes run 4..10240.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientHeader {
    pub opcode: u32,
    pub payload_len: usize,
}

impl ClientHeader {
    /// Fails above the server's acceptance window: the size field counts payload + 4 and the
    /// server rejects anything outside 4..10240 (`WorldSocket.h:60`), so payloads cap at 10235.
    pub fn new(opcode: u32, payload_len: usize) -> Option<Self> {
        if payload_len + 4 >= 10240 {
            return None;
        }
        Some(Self { opcode, payload_len })
    }

    /// The wire size field: payload length plus the four size bytes of the u32 opcode slot.
    pub fn size_field(&self) -> u16 {
        (self.payload_len + 4) as u16
    }

    /// Encode to the six encrypted-order bytes: size big-endian, opcode little-endian.
    pub fn encode(&self) -> [u8; 6] {
        let size = self.size_field().to_be_bytes();
        let opcode = self.opcode.to_le_bytes();
        [size[0], size[1], opcode[0], opcode[1], opcode[2], opcode[3]]
    }

    /// Decode six plaintext (already decrypted) header bytes.
    pub fn decode(bytes: &[u8; 6]) -> Option<Self> {
        let size = u16::from_be_bytes([bytes[0], bytes[1]]);
        let opcode = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        if !(4..10240).contains(&size) {
            return None;
        }
        Some(Self { opcode, payload_len: (size - 4) as usize })
    }
}

/// The server→client header (`ServerPktHeader.h:25-45`): two encrypted big-endian size bytes
/// when the packet fits, three (with the `0x80` flag in the first) above 0x7FFF, then two
/// encrypted little-endian opcode bytes. Size counts the payload plus the two opcode bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerHeader {
    pub opcode: u16,
    pub payload_len: usize,
    pub large: bool,
}

/// How many bytes the next server header occupies: `None` until at least two arrived.
pub fn server_header_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 2 {
        return None;
    }
    Some(if bytes[0] & 0x80 != 0 { 5 } else { 4 })
}

impl ServerHeader {
    /// Decode from decrypted prefix bytes (four or five as `server_header_len` reported).
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (size, large) = if bytes.len() >= 5 && bytes[0] & 0x80 != 0 {
            (((bytes[0] & 0x7F) as usize) << 16 | (bytes[1] as usize) << 8 | bytes[2] as usize, true)
        } else if bytes.len() >= 4 && bytes[0] & 0x80 == 0 {
            ((bytes[0] as usize) << 8 | bytes[1] as usize, false)
        } else {
            return None;
        };
        if size < 2 {
            return None;
        }
        let opcode = u16::from_le_bytes([bytes[if large { 3 } else { 2 }], bytes[if large { 4 } else { 3 }]]);
        Some(Self { opcode, payload_len: size - 2, large })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_header_round_trips_and_validates_sizes() {
        let header = ClientHeader::new(0x1DC, 4).unwrap(); // CMSG_PING with a u32 payload
        assert_eq!(header.size_field(), 8);
        assert_eq!(ClientHeader::decode(&header.encode()), Some(header));
        assert!(ClientHeader::decode(&ClientHeader::new(1, 0).unwrap().encode()).is_some(), "size 4 is legal");
        assert!(ClientHeader::new(1, 10240).is_none(), "payload 10240 exceeds the client window");
    }

    #[test]
    fn server_header_decodes_normal_and_large_forms() {
        let normal = [0x00, 0x12, 0xA9, 0x00]; // size 18, opcode 0x00A9 little-endian
        assert_eq!(
            ServerHeader::decode(&normal),
            Some(ServerHeader { opcode: 0x00A9, payload_len: 16, large: false })
        );
        let large = [0x82, 0x00, 0x10, 0x0B, 0x00]; // 0x80010 = 65552? decode as flagged
        let decoded = ServerHeader::decode(&large).expect("large form decodes");
        assert!(decoded.large);
        assert_eq!(server_header_len(&large[..1]), None);
        assert_eq!(server_header_len(&large[..2]), Some(5));
        assert_eq!(server_header_len(&normal[..2]), Some(4));
    }
}
