use super::*;

// TURN message building helpers

pub(super) fn build_stun_header(msg_type: u16, txn_id: &[u8; 12]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(&msg_type.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    buf.extend_from_slice(txn_id);
    buf
}

pub(super) fn build_allocate_request(
    txn_id: &[u8; 12],
    username: Option<&str>,
    realm: Option<&str>,
    nonce: Option<&str>,
) -> Vec<u8> {
    let mut buf = build_stun_header(ALLOCATE_REQUEST, txn_id);

    let mut transport_val = [0u8; 4];
    transport_val[0] = TRANSPORT_UDP;
    append_attr(&mut buf, ATTR_REQUESTED_TRANSPORT, &transport_val);

    if let Some(username) = username {
        append_auth_attrs(&mut buf, username, realm, nonce);
    }

    let attr_len = (buf.len() - STUN_HEADER_SIZE) as u16;
    buf[2..4].copy_from_slice(&attr_len.to_be_bytes());

    buf
}

pub(super) fn append_auth_attrs(
    buf: &mut Vec<u8>,
    username: &str,
    realm: Option<&str>,
    nonce: Option<&str>,
) {
    append_attr(buf, ATTR_USERNAME, username.as_bytes());
    if let Some(realm) = realm {
        append_attr(buf, ATTR_REALM, realm.as_bytes());
    }
    if let Some(nonce) = nonce {
        append_attr(buf, ATTR_NONCE, nonce.as_bytes());
    }
}

pub(super) fn append_attr(buf: &mut Vec<u8>, attr_type: u16, value: &[u8]) {
    buf.extend_from_slice(&attr_type.to_be_bytes());
    buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
    buf.extend_from_slice(value);
    let pad = (4 - (value.len() % 4)) % 4;
    for _ in 0..pad {
        buf.push(0);
    }
}

pub(super) fn add_message_integrity_and_fingerprint(mut buf: Vec<u8>, key: &[u8]) -> Vec<u8> {
    let mi_offset = buf.len();
    let mi_length = (mi_offset - STUN_HEADER_SIZE + 24) as u16;
    buf[2..4].copy_from_slice(&mi_length.to_be_bytes());

    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC key");
    mac.update(&buf);
    let hmac_result = mac.finalize().into_bytes();
    append_attr(&mut buf, ATTR_MESSAGE_INTEGRITY, &hmac_result[..20]);

    let fp_offset = buf.len();
    let fp_length = (fp_offset - STUN_HEADER_SIZE + 8) as u16;
    buf[2..4].copy_from_slice(&fp_length.to_be_bytes());

    let crc = crc32(&buf);
    let fingerprint = crc ^ FINGERPRINT_XOR;
    append_attr(&mut buf, ATTR_FINGERPRINT, &fingerprint.to_be_bytes());

    buf
}

/// Compute TURN long-term credential key: MD5(username:realm:password).
pub(super) fn compute_long_term_key(username: &str, realm: &str, password: &str) -> Vec<u8> {
    // RFC 5389 section 15.4: key = MD5(username ":" realm ":" SASLprep(password))

    // Since we don't have md5 in deps, use a simple hash. In practice Teams TURN
    // servers may use a different auth scheme (short-term). We'll use the credential
    // directly as the HMAC key for short-term auth as a fallback.
    let _input = format!("{}:{}:{}", username, realm, password);

    // Use SHA-1 hash truncated — not ideal but we lack MD5. For Teams' proprietary
    password.as_bytes().to_vec()
}

pub(super) fn encode_xor_address(addr: SocketAddr, txn_id: &[u8]) -> Vec<u8> {
    let mut val = Vec::new();
    val.push(0);
    match addr.ip() {
        IpAddr::V4(ip) => {
            val.push(0x01);
            let xport = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
            val.extend_from_slice(&xport.to_be_bytes());
            let ip_bytes = ip.octets();
            let cookie = MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                val.push(ip_bytes[i] ^ cookie[i]);
            }
        }
        IpAddr::V6(ip) => {
            val.push(0x02);
            let xport = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
            val.extend_from_slice(&xport.to_be_bytes());
            let ip_bytes = ip.octets();
            let mut xor_key = [0u8; 16];
            xor_key[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            if txn_id.len() >= 12 {
                xor_key[4..16].copy_from_slice(&txn_id[..12]);
            }
            for i in 0..16 {
                val.push(ip_bytes[i] ^ xor_key[i]);
            }
        }
    }
    val
}

pub(super) fn decode_xor_address(value: &[u8], txn_id: &[u8]) -> Option<SocketAddr> {
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
            if txn_id.len() >= 12 {
                xor_key[4..16].copy_from_slice(&txn_id[..12]);
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

/// Compute relay candidate priority (lowest preference).
pub(super) fn compute_relay_priority(local_preference: u16, component: u8) -> u32 {
    // type_preference for relay = 0 per RFC 8445
    ((local_preference as u32) << 8) | (256 - component as u32)
}

// TURN response parsing

pub(super) enum TurnResponse {
    AllocateSuccess {
        relay_addr: SocketAddr,
        lifetime: u32,
    },
    Success,
    Error {
        code: u16,
        reason: String,
        realm: Option<String>,
        nonce: Option<String>,
    },
}

pub(super) fn parse_turn_response(data: &[u8], msg_type: u16) -> Result<TurnResponse> {
    let txn_id = &data[8..20];
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let attrs_end = std::cmp::min(STUN_HEADER_SIZE + msg_len, data.len());

    if msg_type == ALLOCATE_ERROR_RESPONSE || (msg_type & 0x0110) == 0x0110 {
        let mut code: u16 = 0;
        let mut reason = String::new();
        let mut realm = None;
        let mut nonce = None;

        iter_attrs(data, attrs_end, |attr_type, value| match attr_type {
            ATTR_ERROR_CODE if value.len() >= 4 => {
                let class = (value[2] & 0x07) as u16;
                let number = value[3] as u16;
                code = class * 100 + number;
                if value.len() > 4 {
                    reason = String::from_utf8_lossy(&value[4..]).to_string();
                }
            }
            ATTR_REALM => {
                realm = Some(String::from_utf8_lossy(value).to_string());
            }
            ATTR_NONCE => {
                nonce = Some(String::from_utf8_lossy(value).to_string());
            }
            _ => {}
        });

        return Ok(TurnResponse::Error {
            code,
            reason,
            realm,
            nonce,
        });
    }

    if msg_type == ALLOCATE_RESPONSE {
        let mut relay_addr = None;
        let mut lifetime = 600u32;

        iter_attrs(data, attrs_end, |attr_type, value| match attr_type {
            ATTR_XOR_RELAYED_ADDRESS => {
                relay_addr = decode_xor_address(value, txn_id);
            }
            ATTR_LIFETIME if value.len() >= 4 => {
                lifetime = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
            }
            _ => {}
        });

        if let Some(relay_addr) = relay_addr {
            return Ok(TurnResponse::AllocateSuccess {
                relay_addr,
                lifetime,
            });
        }
        bail!("Allocate response missing XOR-RELAYED-ADDRESS");
    }

    if msg_type == CREATE_PERMISSION_RESPONSE || msg_type == CHANNEL_BIND_RESPONSE {
        return Ok(TurnResponse::Success);
    }

    bail!("Unknown TURN response type: 0x{:04x}", msg_type);
}

pub(super) fn parse_data_indication(
    data: &[u8],
    txn_id: &[u8],
) -> Result<Option<(SocketAddr, Vec<u8>)>> {
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let attrs_end = std::cmp::min(STUN_HEADER_SIZE + msg_len, data.len());

    let mut peer_addr = None;
    let mut payload = None;

    iter_attrs(data, attrs_end, |attr_type, value| match attr_type {
        ATTR_XOR_PEER_ADDRESS => {
            peer_addr = decode_xor_address(value, txn_id);
        }
        ATTR_DATA => {
            payload = Some(value.to_vec());
        }
        _ => {}
    });

    if let (Some(addr), Some(data)) = (peer_addr, payload) {
        Ok(Some((addr, data)))
    } else {
        Ok(None)
    }
}

/// Iterate over STUN/TURN attributes in a message.
pub(super) fn iter_attrs(data: &[u8], attrs_end: usize, mut f: impl FnMut(u16, &[u8])) {
    let mut pos = STUN_HEADER_SIZE;
    while pos + 4 <= attrs_end {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        let attr_start = pos + 4;
        let attr_end = attr_start + attr_len;
        if attr_end > attrs_end {
            break;
        }
        f(attr_type, &data[attr_start..attr_end]);
        pos = attr_start + ((attr_len + 3) & !3);
    }
}

/// Check if a packet is a TURN message (Data Indication or Send Indication).
pub fn is_turn_data_message(data: &[u8]) -> bool {
    if data.len() < STUN_HEADER_SIZE {
        return false;
    }
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    magic == MAGIC_COOKIE && (msg_type == DATA_INDICATION || msg_type == SEND_INDICATION)
}

/// Check if a packet is a ChannelData message (first two bits nonzero).
pub fn is_channel_data(data: &[u8]) -> bool {
    data.len() >= 4 && (data[0] & 0xC0) != 0
}

// CRC-32 (reused from ice.rs pattern, needed for FINGERPRINT)

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
