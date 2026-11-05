//! WM's opt-in WMC1 v3 extension, not stock 12340 opcodes.
//!
//! Source: wm-project/native_modules/mod-multiclass/src/MulticlassProtocol.h and
//! MulticlassTransport.cpp and MulticlassHunt.cpp::SendHunt (operation 8, absent from the
//! Operation enum); addon-facing ABI: client_extensions/multiclass/MulticlassClient.cpp.

pub const CLIENT_OPCODE: u32 = 0x51b;
pub const SERVER_OPCODE: u16 = 0x51a;
pub const MAX_REPLY: usize = 4096;

#[derive(Clone, Copy)]
pub struct Request {
    pub operation: u16,
    pub serial: u32,
    guid: u32,
    choice: u8,
    token_low: u32,
    token_high: u32,
}

impl Request {
    pub fn from_arguments([op, serial, guid, choice, low, high]: [u32; 6]) -> Option<Self> {
        if !(1..=5).contains(&op) || serial == 0 || choice > 11
            || (op != 3 && op != 5 && (low != 0 || high != 0))
            || (op == 1 && (guid != 0 || choice != 0))
        {
            return None;
        }
        Some(Self {
            operation: op as u16, serial, guid, choice: choice as u8,
            token_low: low, token_high: high,
        })
    }

    pub fn in_world(self) -> bool {
        self.operation == 4 || self.operation == 5
    }

    /// Keep the preview authority in the resident host as well as in Glue presentation.
    /// All three variable-length counters must fit before the final 64-bit token.
    pub fn preview_reply(self, bytes: &[u8]) -> Option<PreviewToken> {
        let header = ReplyHeader::decode(bytes)?;
        if self.operation != 2 || header.operation != 2 || header.result != 0
            || bytes[14..16] != 1u16.to_le_bytes() || bytes.len() < 41
            || bytes[16..20] != self.guid.to_le_bytes() || bytes[22] != self.choice
            || bytes[27] != 1
        {
            return None;
        }
        let mut offset = 30usize;
        for _ in 0..3 {
            let length = usize::from(*bytes.get(offset)?);
            offset = offset.checked_add(1 + length)?;
            if offset > bytes.len() { return None; }
        }
        if offset.checked_add(8)? != bytes.len() { return None; }
        let low = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        let high = u32::from_le_bytes(bytes[offset + 4..].try_into().ok()?);
        if low == 0 && high == 0 { return None; }
        Some(PreviewToken { guid: self.guid, choice: self.choice, low, high })
    }

    /// The socket owns the opcode and encrypted header; this is exactly the 28-byte body.
    pub fn encode(self, wire_id: u32) -> [u8; 28] {
        let mut bytes = [0; 28];
        bytes[..4].copy_from_slice(b"WMC1");
        bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.operation.to_le_bytes());
        bytes[8..12].copy_from_slice(&wire_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.guid.to_le_bytes());
        bytes[16] = self.choice;
        bytes[20..24].copy_from_slice(&self.token_low.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.token_high.to_le_bytes());
        bytes
    }
}

/// Deliberately not Debug: preview tokens authorize a persistent character mutation.
pub struct PreviewToken {
    guid: u32,
    choice: u8,
    low: u32,
    high: u32,
}

impl PreviewToken {
    pub fn permits(&self, request: Request) -> bool {
        request.operation == 3 && request.guid == self.guid && request.choice == self.choice
            && request.token_low == self.low && request.token_high == self.high
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReplyHeader {
    pub operation: u16,
    pub wire_id: u32,
    pub result: u16,
}

impl ReplyHeader {
    /// Bound and validate the envelope before routing. The installed Lua consumers validate
    /// each versioned body, including counters and trailing bytes; this is not a body decoder.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if !(16..=MAX_REPLY).contains(&bytes.len()) || &bytes[..4] != b"WMC1"
            || bytes[4..6] != 3u16.to_le_bytes()
        {
            return None;
        }
        let operation = u16::from_le_bytes([bytes[6], bytes[7]]);
        let wire_id = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let result = u16::from_le_bytes([bytes[12], bytes[13]]);
        let count = u16::from_le_bytes([bytes[14], bytes[15]]);
        if !(1..=8).contains(&operation) || result > 12
            || ((operation <= 5) != (wire_id != 0))
            || (result != 0 && (count != 0 || bytes.len() != 16))
            || (operation >= 6 && (result != 0 || count != 1 || bytes.len() == 16))
            || (result == 0 && operation == 3 && (count != 0 || bytes.len() != 16))
        {
            return None;
        }
        Some(Self { operation, wire_id, result })
    }

    /// Ownership is a server snapshot, not inferred from Lua selection or the base class.
    /// Talent rows are 18 bytes each after the seven fixed bytes and one counter string.
    pub fn owned_classes(self, bytes: &[u8]) -> Option<u16> {
        if !matches!(self.operation, 4 | 5) || self.result != 0 || bytes.len() < 24 {
            return None;
        }
        let count = usize::from(u16::from_le_bytes([bytes[14], bytes[15]]));
        let counter_len = usize::from(bytes[23]);
        let owned = u16::from_le_bytes([bytes[16], bytes[17]]);
        if count > 150 || counter_len == 0 || counter_len > 96 || 24 + counter_len + count * 18 != bytes.len()
            || owned == 0 || owned & !1535 != 0 || bytes[22] > 1
        {
            return None;
        }
        if !bytes[24..24 + counter_len].iter().all(u8::is_ascii_digit) { return None; }
        for row in bytes[24 + counter_len..].chunks_exact(18) {
            if row[4] > 2 || row[5] > 10 || row[6] > 3 || row[7] > row[8]
                || row[8] == 0 || row[8] > 5
            {
                return None;
            }
        }
        Some(owned)
    }
}

pub fn hex_reply(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    text
}
