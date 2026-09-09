use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// STUN magic cookie (RFC 5389).
const MAGIC_COOKIE: u32 = 0x2112A442;

/// STUN message types.
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;

/// STUN attribute types.
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_USERNAME: u16 = 0x0006;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
pub(super) const ATTR_FINGERPRINT: u16 = 0x8028;
const ATTR_PRIORITY: u16 = 0x0024;
const ATTR_USE_CANDIDATE: u16 = 0x0025;
const ATTR_ICE_CONTROLLED: u16 = 0x8029;
const ATTR_ICE_CONTROLLING: u16 = 0x802A;

/// STUN header size (type + length + magic + transaction ID).
const STUN_HEADER_SIZE: usize = 20;

/// FINGERPRINT XOR constant per RFC 5389.
pub(super) const FINGERPRINT_XOR: u32 = 0x5354554e;

// CRC-32 (IEEE 802.3) — needed for STUN FINGERPRINT attribute.

const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

pub(super) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    crc ^ 0xFFFFFFFF
}

// STUN message building and parsing (RFC 5389)

/// Generate a random 12-byte STUN transaction ID.
pub fn generate_transaction_id() -> [u8; 12] {
    let id1 = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();
    let b1 = id1.as_bytes();
    let b2 = id2.as_bytes();
    let mut txn = [0u8; 12];
    txn[..8].copy_from_slice(&b1[..8]);
    txn[8..12].copy_from_slice(&b2[..4]);
    txn
}

/// Build a minimal STUN Binding Request (header only, no attributes).
pub fn build_stun_binding_request(transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(STUN_HEADER_SIZE);
    buf.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    buf.extend_from_slice(transaction_id);
    buf
}

/// Build a STUN Binding Request with USERNAME, MESSAGE-INTEGRITY, and FINGERPRINT
/// for ICE connectivity checks.
///
/// `username` is `{remote_ufrag}:{local_ufrag}`.
/// `key` is the remote ICE password (used as HMAC-SHA1 key for MESSAGE-INTEGRITY).
/// `priority` is the local candidate priority to advertise.
pub fn build_ice_binding_request(
    transaction_id: &[u8; 12],
    username: &str,
    key: &[u8],
    priority: u32,
    controlling: bool,
    tie_breaker: u64,
) -> Vec<u8> {
    // Start with header (length placeholder)
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    buf.extend_from_slice(transaction_id);

    append_stun_attr_string(&mut buf, ATTR_USERNAME, username);

    append_stun_attr(&mut buf, ATTR_PRIORITY, &priority.to_be_bytes());

    // ICE-CONTROLLING or ICE-CONTROLLED (8 bytes)
    if controlling {
        append_stun_attr(&mut buf, ATTR_ICE_CONTROLLING, &tie_breaker.to_be_bytes());
        append_stun_attr(&mut buf, ATTR_USE_CANDIDATE, &[]);
    } else {
        append_stun_attr(&mut buf, ATTR_ICE_CONTROLLED, &tie_breaker.to_be_bytes());
    }

    // Per RFC 5389: the length field in the header must include the MESSAGE-INTEGRITY
    let mi_offset = buf.len();
    let mi_length = (mi_offset - STUN_HEADER_SIZE + 24) as u16;
    buf[2..4].copy_from_slice(&mi_length.to_be_bytes());

    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC key length is valid");
    mac.update(&buf);
    let hmac_result = mac.finalize().into_bytes();
    append_stun_attr(&mut buf, ATTR_MESSAGE_INTEGRITY, &hmac_result[..20]);

    // XOR'd with 0x5354554e. The header length must include the FINGERPRINT attr (8 bytes).
    let fp_offset = buf.len();
    let fp_length = (fp_offset - STUN_HEADER_SIZE + 8) as u16;
    buf[2..4].copy_from_slice(&fp_length.to_be_bytes());

    let crc = crc32(&buf);
    let fingerprint = crc ^ FINGERPRINT_XOR;
    append_stun_attr(&mut buf, ATTR_FINGERPRINT, &fingerprint.to_be_bytes());

    buf
}

/// Build a STUN Binding Success Response with XOR-MAPPED-ADDRESS.
pub fn build_binding_response(
    transaction_id: &[u8; 12],
    mapped_addr: SocketAddr,
    key: Option<&[u8]>,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    buf.extend_from_slice(transaction_id);

    let xma = encode_xor_mapped_address(mapped_addr, transaction_id);
    append_stun_attr(&mut buf, ATTR_XOR_MAPPED_ADDRESS, &xma);

    if let Some(key) = key {
        let mi_offset = buf.len();
        let mi_length = (mi_offset - STUN_HEADER_SIZE + 24) as u16;
        buf[2..4].copy_from_slice(&mi_length.to_be_bytes());

        let mut mac = HmacSha1::new_from_slice(key).expect("HMAC key length is valid");
        mac.update(&buf);
        let hmac_result = mac.finalize().into_bytes();
        append_stun_attr(&mut buf, ATTR_MESSAGE_INTEGRITY, &hmac_result[..20]);

        let fp_offset = buf.len();
        let fp_length = (fp_offset - STUN_HEADER_SIZE + 8) as u16;
        buf[2..4].copy_from_slice(&fp_length.to_be_bytes());

        let crc = crc32(&buf);
        let fingerprint = crc ^ FINGERPRINT_XOR;
        append_stun_attr(&mut buf, ATTR_FINGERPRINT, &fingerprint.to_be_bytes());
    } else {
        // Update length without integrity/fingerprint
        let attr_len = (buf.len() - STUN_HEADER_SIZE) as u16;
        buf[2..4].copy_from_slice(&attr_len.to_be_bytes());
    }

    buf
}

/// Check if a received UDP packet is a STUN message (any type).
pub fn is_stun_message(data: &[u8]) -> bool {
    if data.len() < STUN_HEADER_SIZE {
        return false;
    }
    // First two bits must be 0, magic cookie must match.
    let first_byte = data[0];
    if first_byte & 0xC0 != 0 {
        return false;
    }
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    magic == MAGIC_COOKIE
}

/// Check if a received UDP packet is a STUN Binding Success Response.
pub fn is_stun_response(data: &[u8]) -> bool {
    if data.len() < STUN_HEADER_SIZE {
        return false;
    }
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    msg_type == BINDING_RESPONSE && magic == MAGIC_COOKIE
}

/// Check if a received UDP packet is a STUN Binding Request.
pub fn is_stun_request(data: &[u8]) -> bool {
    if data.len() < STUN_HEADER_SIZE {
        return false;
    }
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    msg_type == BINDING_REQUEST && magic == MAGIC_COOKIE
}

/// Extract the transaction ID from a STUN message.
pub fn get_transaction_id(data: &[u8]) -> Option<[u8; 12]> {
    if data.len() < STUN_HEADER_SIZE {
        return None;
    }
    let mut txn = [0u8; 12];
    txn.copy_from_slice(&data[8..20]);
    Some(txn)
}

/// Parse a STUN Binding Response and extract the XOR-MAPPED-ADDRESS.
pub fn parse_binding_response(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < STUN_HEADER_SIZE {
        return None;
    }
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    if msg_type != BINDING_RESPONSE {
        return None;
    }
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if magic != MAGIC_COOKIE {
        return None;
    }

    let txn_id = &data[8..20];
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let attrs_end = std::cmp::min(STUN_HEADER_SIZE + msg_len, data.len());

    let mut pos = STUN_HEADER_SIZE;
    while pos + 4 <= attrs_end {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        let attr_start = pos + 4;
        let attr_end = attr_start + attr_len;

        if attr_end > attrs_end {
            break;
        }

        if attr_type == ATTR_XOR_MAPPED_ADDRESS {
            return decode_xor_mapped_address(&data[attr_start..attr_end], txn_id);
        }

        // Also handle MAPPED-ADDRESS as fallback
        if attr_type == ATTR_MAPPED_ADDRESS {
            return decode_mapped_address(&data[attr_start..attr_end]);
        }

        pos = attr_start + ((attr_len + 3) & !3);
    }

    None
}

/// Verify MESSAGE-INTEGRITY of a received STUN message.
pub fn verify_message_integrity(data: &[u8], key: &[u8]) -> bool {
    if data.len() < STUN_HEADER_SIZE {
        return false;
    }

    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let attrs_end = std::cmp::min(STUN_HEADER_SIZE + msg_len, data.len());

    let mut pos = STUN_HEADER_SIZE;
    while pos + 4 <= attrs_end {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        let attr_start = pos + 4;

        if attr_type == ATTR_MESSAGE_INTEGRITY && attr_len == 20 {
            let attr_end = attr_start + 20;
            if attr_end > data.len() {
                return false;
            }
            let received_hmac = &data[attr_start..attr_end];

            // Compute HMAC over header + attributes up to MESSAGE-INTEGRITY.
            let mut check_buf = data[..pos].to_vec();
            let adjusted_len = (pos - STUN_HEADER_SIZE + 24) as u16;
            check_buf[2..4].copy_from_slice(&adjusted_len.to_be_bytes());

            let mut mac = HmacSha1::new_from_slice(key).expect("HMAC key length is valid");
            mac.update(&check_buf);
            let computed = mac.finalize().into_bytes();

            return &computed[..20] == received_hmac;
        }

        pos = attr_start + ((attr_len + 3) & !3);
    }

    false
}

// STUN attribute helpers

/// Append a STUN attribute with raw value bytes (handles 4-byte padding).
fn append_stun_attr(buf: &mut Vec<u8>, attr_type: u16, value: &[u8]) {
    buf.extend_from_slice(&attr_type.to_be_bytes());
    buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
    buf.extend_from_slice(value);
    let pad = (4 - (value.len() % 4)) % 4;
    for _ in 0..pad {
        buf.push(0);
    }
}

/// Append a STUN attribute with a UTF-8 string value.
fn append_stun_attr_string(buf: &mut Vec<u8>, attr_type: u16, value: &str) {
    append_stun_attr(buf, attr_type, value.as_bytes());
}

/// Encode a SocketAddr as XOR-MAPPED-ADDRESS value bytes.
pub(super) fn encode_xor_mapped_address(addr: SocketAddr, transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut val = Vec::new();
    val.push(0);
    match addr.ip() {
        IpAddr::V4(ip) => {
            val.push(0x01);
            let xport = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
            val.extend_from_slice(&xport.to_be_bytes());
            let ip_bytes = ip.octets();
            let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                val.push(ip_bytes[i] ^ cookie_bytes[i]);
            }
        }
        IpAddr::V6(ip) => {
            val.push(0x02);
            let xport = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
            val.extend_from_slice(&xport.to_be_bytes());
            let ip_bytes = ip.octets();
            let mut xor_key = [0u8; 16];
            xor_key[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            xor_key[4..16].copy_from_slice(transaction_id);
            for i in 0..16 {
                val.push(ip_bytes[i] ^ xor_key[i]);
            }
        }
    }
    val
}

/// Decode XOR-MAPPED-ADDRESS attribute value.
pub(super) fn decode_xor_mapped_address(value: &[u8], transaction_id: &[u8]) -> Option<SocketAddr> {
    if value.len() < 4 {
        return None;
    }
    let family = value[1];
    let xport = u16::from_be_bytes([value[2], value[3]]);
    let port = xport ^ (MAGIC_COOKIE >> 16) as u16;

    match family {
        0x01 if value.len() >= 8 => {
            let cookie = MAGIC_COOKIE.to_be_bytes();
            let ip = Ipv4Addr::new(
                value[4] ^ cookie[0],
                value[5] ^ cookie[1],
                value[6] ^ cookie[2],
                value[7] ^ cookie[3],
            );
            Some(SocketAddr::new(IpAddr::V4(ip), port))
        }
        0x02 if value.len() >= 20 => {
            let mut xor_key = [0u8; 16];
            xor_key[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            if transaction_id.len() >= 12 {
                xor_key[4..16].copy_from_slice(&transaction_id[..12]);
            }
            let mut octets = [0u8; 16];
            for i in 0..16 {
                octets[i] = value[4 + i] ^ xor_key[i];
            }
            let ip = std::net::Ipv6Addr::from(octets);
            Some(SocketAddr::new(IpAddr::V6(ip), port))
        }
        _ => None,
    }
}

/// Decode MAPPED-ADDRESS attribute value (no XOR).
fn decode_mapped_address(value: &[u8]) -> Option<SocketAddr> {
    if value.len() < 4 {
        return None;
    }
    let family = value[1];
    let port = u16::from_be_bytes([value[2], value[3]]);

    match family {
        0x01 if value.len() >= 8 => {
            let ip = Ipv4Addr::new(value[4], value[5], value[6], value[7]);
            Some(SocketAddr::new(IpAddr::V4(ip), port))
        }
        _ => None,
    }
}
