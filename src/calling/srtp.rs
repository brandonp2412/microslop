//! SRTP encryption/decryption using AES-128-CM with HMAC-SHA1-80 (RFC 3711).
//!
//! Key derivation and packet protection for RTP streams using the SRTP profile
//! specified in the SDP `a=crypto` line: `AES_CM_128_HMAC_SHA1_80`.

use aes::cipher::{KeyIvInit, StreamCipher};
use anyhow::{bail, Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;

use super::rtp;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;
type HmacSha1 = Hmac<Sha1>;

/// SRTP auth tag length for HMAC-SHA1-80 (80 bits = 10 bytes).
pub const SRTP_AUTH_TAG_LEN: usize = 10;

/// Master key length for AES-128 (16 bytes).
const MASTER_KEY_LEN: usize = 16;

/// Master salt length (14 bytes per RFC 3711).
const MASTER_SALT_LEN: usize = 14;

/// Total keying material: 16 bytes key + 14 bytes salt = 30 bytes.
const KEYING_MATERIAL_LEN: usize = MASTER_KEY_LEN + MASTER_SALT_LEN;

/// SRTP key derivation labels (RFC 3711, section 4.3.1).
const LABEL_CIPHER_KEY: u8 = 0x00;
const LABEL_AUTH_KEY: u8 = 0x01;
const LABEL_SALT: u8 = 0x02;

/// SRTCP key derivation labels (RFC 3711, section 3.4).
const LABEL_SRTCP_CIPHER_KEY: u8 = 0x03;
const LABEL_SRTCP_AUTH_KEY: u8 = 0x04;
const LABEL_SRTCP_SALT: u8 = 0x05;

/// Parsed SRTP keying material from an SDP crypto line.
#[derive(Debug, Clone)]
pub struct SrtpKeyingMaterial {
    pub master_key: [u8; MASTER_KEY_LEN],
    pub master_salt: [u8; MASTER_SALT_LEN],
    pub tag: u32, // crypto tag number from SDP
}

/// Derived session keys for SRTP.
#[derive(Debug, Clone)]
pub struct SrtpSessionKeys {
    pub cipher_key: [u8; 16],
    pub auth_key: [u8; 20],
    pub salt: [u8; 14],
}

/// SRTP context for encrypting/decrypting packets on a single stream.
#[derive(Debug, Clone)]
pub struct SrtpContext {
    pub local_keys: SrtpSessionKeys,
    pub remote_keys: SrtpSessionKeys,
    pub local_roc: u32,
    pub remote_roc: u32,
    pub remote_highest_seq: u16,
    pub local_srtcp_keys: SrtpSessionKeys,
    pub remote_srtcp_keys: SrtpSessionKeys,
    pub local_srtcp_index: u32,
    pub remote_srtcp_index: u32,
}

/// Parse an SDP crypto line to extract SRTP keying material.
///
/// Formats handled:
/// - `a=crypto:2 AES_CM_128_HMAC_SHA1_80 inline:<base64key>|2^31|1:1`
/// - `a=crypto:3 AES_CM_128_HMAC_SHA1_80 inline:<base64key>|2^31`
/// - `a=cryptoscale:1 client AES_CM_128_HMAC_SHA1_80 inline:<base64key>|2^31|1:1`
pub fn parse_crypto_line(line: &str) -> Result<SrtpKeyingMaterial> {
    let line = line.trim();

    let tag = if let Some(rest) = line.strip_prefix("a=crypto:") {
        let tag_end = rest.find(' ').context("malformed crypto line")?;
        rest[..tag_end].parse::<u32>().context("bad crypto tag")?
    } else if let Some(rest) = line.strip_prefix("a=cryptoscale:") {
        let tag_end = rest.find(' ').context("malformed cryptoscale line")?;
        rest[..tag_end]
            .parse::<u32>()
            .context("bad cryptoscale tag")?
    } else {
        bail!("not a crypto line: {}", line);
    };

    let inline_pos = line
        .find("inline:")
        .context("no inline: key in crypto line")?;
    let key_part = &line[inline_pos + "inline:".len()..];

    let b64_key = if let Some(pipe) = key_part.find('|') {
        &key_part[..pipe]
    } else {
        key_part
    };

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64_key)
        .context("failed to base64 decode SRTP key")?;

    if decoded.len() < KEYING_MATERIAL_LEN {
        bail!(
            "SRTP keying material too short: {} bytes (need {})",
            decoded.len(),
            KEYING_MATERIAL_LEN
        );
    }

    let mut master_key = [0u8; MASTER_KEY_LEN];
    let mut master_salt = [0u8; MASTER_SALT_LEN];
    master_key.copy_from_slice(&decoded[..MASTER_KEY_LEN]);
    master_salt.copy_from_slice(&decoded[MASTER_KEY_LEN..KEYING_MATERIAL_LEN]);

    Ok(SrtpKeyingMaterial {
        master_key,
        master_salt,
        tag,
    })
}

/// Derive session keys from master key + salt using AES-128-CM PRF (RFC 3711, 4.3.1).
///
/// key_derivation_rate = 0 (default), so index DIV key_derivation_rate = 0.
pub fn derive_session_keys(material: &SrtpKeyingMaterial) -> Result<SrtpSessionKeys> {
    let cipher_key = prf_derive(
        &material.master_key,
        &material.master_salt,
        LABEL_CIPHER_KEY,
        16,
    )?;
    let auth_key = prf_derive(
        &material.master_key,
        &material.master_salt,
        LABEL_AUTH_KEY,
        20,
    )?;
    let salt = prf_derive(&material.master_key, &material.master_salt, LABEL_SALT, 14)?;

    let mut ck = [0u8; 16];
    let mut ak = [0u8; 20];
    let mut s = [0u8; 14];
    ck.copy_from_slice(&cipher_key);
    ak.copy_from_slice(&auth_key);
    s.copy_from_slice(&salt);

    Ok(SrtpSessionKeys {
        cipher_key: ck,
        auth_key: ak,
        salt: s,
    })
}

/// Derive SRTCP session keys from master key + salt (RFC 3711, labels 0x03-0x05).
pub fn derive_srtcp_session_keys(material: &SrtpKeyingMaterial) -> Result<SrtpSessionKeys> {
    let cipher_key = prf_derive(
        &material.master_key,
        &material.master_salt,
        LABEL_SRTCP_CIPHER_KEY,
        16,
    )?;
    let auth_key = prf_derive(
        &material.master_key,
        &material.master_salt,
        LABEL_SRTCP_AUTH_KEY,
        20,
    )?;
    let salt = prf_derive(
        &material.master_key,
        &material.master_salt,
        LABEL_SRTCP_SALT,
        14,
    )?;

    let mut ck = [0u8; 16];
    let mut ak = [0u8; 20];
    let mut s = [0u8; 14];
    ck.copy_from_slice(&cipher_key);
    ak.copy_from_slice(&auth_key);
    s.copy_from_slice(&salt);

    Ok(SrtpSessionKeys {
        cipher_key: ck,
        auth_key: ak,
        salt: s,
    })
}

/// PRF for key derivation: AES-128-CM with label and index=0 (RFC 3711, 4.3.1).
fn prf_derive(
    master_key: &[u8; MASTER_KEY_LEN],
    master_salt: &[u8; MASTER_SALT_LEN],
    label: u8,
    output_len: usize,
) -> Result<Vec<u8>> {
    let mut x = [0u8; 14];
    x[7] = label;

    let mut iv = [0u8; 16];
    for i in 0..14 {
        iv[i] = master_salt[i] ^ x[i];
    }

    let mut output = vec![0u8; output_len];
    let mut cipher = Aes128Ctr::new(master_key.into(), &iv.into());
    cipher.apply_keystream(&mut output);

    Ok(output)
}

/// Create an SRTP context from local and remote keying material.
pub fn create_context(
    local_material: &SrtpKeyingMaterial,
    remote_material: &SrtpKeyingMaterial,
) -> Result<SrtpContext> {
    let local_keys = derive_session_keys(local_material)?;
    let remote_keys = derive_session_keys(remote_material)?;
    let local_srtcp_keys = derive_srtcp_session_keys(local_material)?;
    let remote_srtcp_keys = derive_srtcp_session_keys(remote_material)?;

    Ok(SrtpContext {
        local_keys,
        remote_keys,
        local_roc: 0,
        remote_roc: 0,
        remote_highest_seq: 0,
        local_srtcp_keys,
        remote_srtcp_keys,
        local_srtcp_index: 0,
        remote_srtcp_index: 0,
    })
}

/// Encrypt an RTP packet using SRTP (AES-128-CM + HMAC-SHA1-80).
///
/// Returns the SRTP packet: RTP header || encrypted payload || auth tag (10 bytes).
pub fn protect(ctx: &mut SrtpContext, rtp_packet: &[u8]) -> Result<Vec<u8>> {
    let header_len =
        rtp::full_header_len(rtp_packet).context("RTP packet too short for SRTP protection")?;

    let header = &rtp_packet[..header_len];

    // Extract SSRC and sequence number from header
    let ssrc = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    let seq = u16::from_be_bytes([header[2], header[3]]);

    // Build IV for AES-128-CM (RFC 3711, 4.1.1)
    let iv = build_iv(&ctx.local_keys.salt, ssrc, ctx.local_roc, seq);

    let mut srtp_packet = Vec::with_capacity(rtp_packet.len() + SRTP_AUTH_TAG_LEN);
    srtp_packet.extend_from_slice(rtp_packet);
    let mut cipher = Aes128Ctr::new((&ctx.local_keys.cipher_key).into(), &iv.into());
    cipher.apply_keystream(&mut srtp_packet[header_len..]);

    // Compute auth tag over header || encrypted payload || ROC
    let auth_tag = compute_auth_tag(&ctx.local_keys.auth_key, &srtp_packet, ctx.local_roc);
    srtp_packet.extend_from_slice(&auth_tag);

    if seq == 0xFFFF {
        ctx.local_roc = ctx.local_roc.wrapping_add(1);
    }

    Ok(srtp_packet)
}

/// Decrypt an SRTP packet, verifying the auth tag.
///
/// Returns the decrypted RTP packet (header + plaintext payload).
pub fn unprotect(ctx: &mut SrtpContext, srtp_packet: &[u8]) -> Result<Vec<u8>> {
    if srtp_packet.len() < rtp::RTP_HEADER_SIZE + SRTP_AUTH_TAG_LEN {
        bail!("SRTP packet too short");
    }

    let auth_tag_offset = srtp_packet.len() - SRTP_AUTH_TAG_LEN;
    let received_tag = &srtp_packet[auth_tag_offset..];
    let authenticated_portion = &srtp_packet[..auth_tag_offset];

    let seq = u16::from_be_bytes([srtp_packet[2], srtp_packet[3]]);
    let ssrc = u32::from_be_bytes([
        srtp_packet[8],
        srtp_packet[9],
        srtp_packet[10],
        srtp_packet[11],
    ]);

    let estimated_roc = estimate_roc(ctx.remote_roc, ctx.remote_highest_seq, seq);

    let expected_tag = compute_auth_tag(
        &ctx.remote_keys.auth_key,
        authenticated_portion,
        estimated_roc,
    );
    if received_tag != expected_tag.as_slice() {
        bail!("SRTP auth tag mismatch");
    }

    // Compute full header length (fixed header + CSRC + extensions)
    let header_len = rtp::full_header_len(&srtp_packet[..auth_tag_offset])
        .context("SRTP packet has truncated RTP header")?;

    let iv = build_iv(&ctx.remote_keys.salt, ssrc, estimated_roc, seq);
    let mut rtp_packet = authenticated_portion.to_vec();
    let mut cipher = Aes128Ctr::new((&ctx.remote_keys.cipher_key).into(), &iv.into());
    cipher.apply_keystream(&mut rtp_packet[header_len..]);

    let current_index = ((ctx.remote_roc as u64) << 16) | ctx.remote_highest_seq as u64;
    let received_index = ((estimated_roc as u64) << 16) | seq as u64;
    if received_index > current_index {
        ctx.remote_highest_seq = seq;
        ctx.remote_roc = estimated_roc;
    }

    Ok(rtp_packet)
}

/// Build the AES-128-CM IV for SRTP (RFC 3711, 4.1.1).
///
/// IV = (session_salt XOR (SSRC || packet_index)) padded to 16 bytes.
fn build_iv(salt: &[u8; 14], ssrc: u32, roc: u32, seq: u16) -> [u8; 16] {
    let mut iv = [0u8; 16];

    let ssrc_bytes = ssrc.to_be_bytes();
    iv[4] = ssrc_bytes[0];
    iv[5] = ssrc_bytes[1];
    iv[6] = ssrc_bytes[2];
    iv[7] = ssrc_bytes[3];

    let roc_bytes = roc.to_be_bytes();
    iv[8] = roc_bytes[0];
    iv[9] = roc_bytes[1];
    iv[10] = roc_bytes[2];
    iv[11] = roc_bytes[3];
    let seq_bytes = seq.to_be_bytes();
    iv[12] = seq_bytes[0];
    iv[13] = seq_bytes[1];

    for i in 0..14 {
        iv[i] ^= salt[i];
    }

    iv
}

/// Compute HMAC-SHA1-80 auth tag over authenticated_portion || ROC.
fn compute_auth_tag(
    auth_key: &[u8; 20],
    authenticated_portion: &[u8],
    roc: u32,
) -> [u8; SRTP_AUTH_TAG_LEN] {
    let mut mac = HmacSha1::new_from_slice(auth_key).expect("HMAC key length is valid");
    mac.update(authenticated_portion);
    mac.update(&roc.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let mut tag = [0; SRTP_AUTH_TAG_LEN];
    tag.copy_from_slice(&result[..SRTP_AUTH_TAG_LEN]);
    tag
}

/// Estimate ROC for incoming packet (RFC 3711, appendix A).
fn estimate_roc(current_roc: u32, highest_seq: u16, received_seq: u16) -> u32 {
    if highest_seq == 0 && current_roc == 0 {
        return 0;
    }

    if highest_seq < 0x8000 {
        if received_seq as u32 > highest_seq as u32 + 0x8000 {
            current_roc.wrapping_sub(1)
        } else {
            current_roc
        }
    } else if (received_seq as u32) < highest_seq as u32 - 0x8000 {
        current_roc.wrapping_add(1)
    } else {
        current_roc
    }
}

/// Minimum RTCP header size: V/P/RC(1) + PT(1) + length(2) + SSRC(4) = 8 bytes.
const RTCP_HEADER_SIZE: usize = 8;

/// Encrypt an RTCP packet using SRTCP (AES-128-CM + HMAC-SHA1-80, RFC 3711 §3.4).
///
/// Returns: `rtcp_header(8) || encrypted_payload || E||index(4) || auth_tag(10)`.
pub fn protect_rtcp(ctx: &mut SrtpContext, rtcp_packet: &[u8]) -> Result<Vec<u8>> {
    if rtcp_packet.len() < RTCP_HEADER_SIZE {
        bail!("RTCP packet too short for SRTCP protection");
    }

    let header = &rtcp_packet[..RTCP_HEADER_SIZE];
    let ssrc = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let index = ctx.local_srtcp_index;

    let iv = build_srtcp_iv(&ctx.local_srtcp_keys.salt, ssrc, index);

    let mut srtcp = Vec::with_capacity(rtcp_packet.len() + 4 + SRTP_AUTH_TAG_LEN);
    srtcp.extend_from_slice(rtcp_packet);
    if rtcp_packet.len() > RTCP_HEADER_SIZE {
        let mut cipher = Aes128Ctr::new((&ctx.local_srtcp_keys.cipher_key).into(), &iv.into());
        cipher.apply_keystream(&mut srtcp[RTCP_HEADER_SIZE..]);
    }

    let e_index: u32 = 0x8000_0000 | (index & 0x7FFF_FFFF);
    srtcp.extend_from_slice(&e_index.to_be_bytes());
    // Auth tag over everything so far (header || encrypted_payload || E||index)
    let auth_tag = compute_srtcp_auth_tag(&ctx.local_srtcp_keys.auth_key, &srtcp);
    srtcp.extend_from_slice(&auth_tag);

    ctx.local_srtcp_index = index.wrapping_add(1) & 0x7FFF_FFFF;

    Ok(srtcp)
}

/// Decrypt an SRTCP packet, verifying the auth tag.
///
/// Returns the decrypted RTCP packet.
pub fn unprotect_rtcp(ctx: &mut SrtpContext, srtcp_packet: &[u8]) -> Result<Vec<u8>> {
    // Minimum: 8 (header) + 4 (E||index) + 10 (auth tag) = 22
    if srtcp_packet.len() < RTCP_HEADER_SIZE + 4 + SRTP_AUTH_TAG_LEN {
        bail!("SRTCP packet too short");
    }

    let auth_tag_offset = srtcp_packet.len() - SRTP_AUTH_TAG_LEN;
    let received_tag = &srtcp_packet[auth_tag_offset..];
    let authenticated_portion = &srtcp_packet[..auth_tag_offset];

    let expected_tag =
        compute_srtcp_auth_tag(&ctx.remote_srtcp_keys.auth_key, authenticated_portion);
    if received_tag != expected_tag.as_slice() {
        bail!("SRTCP auth tag mismatch");
    }

    let ei_offset = auth_tag_offset - 4;
    let e_index = u32::from_be_bytes([
        srtcp_packet[ei_offset],
        srtcp_packet[ei_offset + 1],
        srtcp_packet[ei_offset + 2],
        srtcp_packet[ei_offset + 3],
    ]);
    let encrypted = (e_index & 0x8000_0000) != 0;
    let srtcp_index = e_index & 0x7FFF_FFFF;

    let header = &srtcp_packet[..RTCP_HEADER_SIZE];
    let ssrc = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

    let mut rtcp = srtcp_packet[..ei_offset].to_vec();
    if encrypted && rtcp.len() > RTCP_HEADER_SIZE {
        let iv = build_srtcp_iv(&ctx.remote_srtcp_keys.salt, ssrc, srtcp_index);
        let mut cipher = Aes128Ctr::new((&ctx.remote_srtcp_keys.cipher_key).into(), &iv.into());
        cipher.apply_keystream(&mut rtcp[RTCP_HEADER_SIZE..]);
    }

    if srtcp_index >= ctx.remote_srtcp_index {
        ctx.remote_srtcp_index = srtcp_index.wrapping_add(1) & 0x7FFF_FFFF;
    }

    Ok(rtcp)
}

/// Build the AES-128-CM IV for SRTCP (RFC 3711, §4.1.1).
///
/// The 48-bit packet index field (bytes 8-13) holds the SRTCP index
/// right-aligned: bytes 8-9 = 0, bytes 10-13 = srtcp_index.
/// This matches SRTP where the field is (ROC<<16)|SEQ.
fn build_srtcp_iv(salt: &[u8; 14], ssrc: u32, srtcp_index: u32) -> [u8; 16] {
    let mut iv = [0u8; 16];

    let ssrc_bytes = ssrc.to_be_bytes();
    iv[4] = ssrc_bytes[0];
    iv[5] = ssrc_bytes[1];
    iv[6] = ssrc_bytes[2];
    iv[7] = ssrc_bytes[3];

    let idx_bytes = srtcp_index.to_be_bytes();
    iv[10] = idx_bytes[0];
    iv[11] = idx_bytes[1];
    iv[12] = idx_bytes[2];
    iv[13] = idx_bytes[3];

    for i in 0..14 {
        iv[i] ^= salt[i];
    }

    iv
}

/// Compute HMAC-SHA1-80 auth tag for SRTCP (no ROC appended, unlike SRTP).
fn compute_srtcp_auth_tag(
    auth_key: &[u8; 20],
    authenticated_portion: &[u8],
) -> [u8; SRTP_AUTH_TAG_LEN] {
    let mut mac = HmacSha1::new_from_slice(auth_key).expect("HMAC key length is valid");
    mac.update(authenticated_portion);
    let result = mac.finalize().into_bytes();
    let mut tag = [0; SRTP_AUTH_TAG_LEN];
    tag.copy_from_slice(&result[..SRTP_AUTH_TAG_LEN]);
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_material() -> SrtpKeyingMaterial {
        let mut key = [0u8; 16];
        let mut salt = [0u8; 14];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = i as u8;
        }
        for (i, byte) in salt.iter_mut().enumerate() {
            *byte = (16 + i) as u8;
        }
        SrtpKeyingMaterial {
            master_key: key,
            master_salt: salt,
            tag: 2,
        }
    }

    #[test]
    fn test_parse_crypto_line() {
        // 30 bytes = 40 base64 chars
        let b64 = base64::engine::general_purpose::STANDARD.encode([0xABu8; 30]);
        let line = format!("a=crypto:2 AES_CM_128_HMAC_SHA1_80 inline:{}|2^31|1:1", b64);
        let mat = parse_crypto_line(&line).unwrap();
        assert_eq!(mat.tag, 2);
        assert_eq!(mat.master_key, [0xAB; 16]);
        assert_eq!(mat.master_salt, [0xAB; 14]);
    }

    #[test]
    fn test_parse_cryptoscale_line() {
        let b64 = base64::engine::general_purpose::STANDARD.encode([0xCD; 30]);
        let line = format!(
            "a=cryptoscale:1 client AES_CM_128_HMAC_SHA1_80 inline:{}|2^31|1:1",
            b64
        );
        let mat = parse_crypto_line(&line).unwrap();
        assert_eq!(mat.tag, 1);
    }

    #[test]
    fn test_key_derivation() {
        let mat = make_test_material();
        let keys = derive_session_keys(&mat).unwrap();
        assert_eq!(keys.cipher_key.len(), 16);
        assert_eq!(keys.auth_key.len(), 20);
        assert_eq!(keys.salt.len(), 14);
        assert!(keys.cipher_key.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_protect_unprotect_roundtrip() {
        let mat = make_test_material();
        let mut ctx = create_context(&mat, &mat).unwrap();

        let payload = vec![0xFF; 160];
        let rtp = rtp::encode(rtp::PT_PCMU, 1, 160, 0xDEADBEEF, &payload);

        let srtp = protect(&mut ctx, &rtp).unwrap();
        assert_eq!(srtp.len(), rtp.len() + SRTP_AUTH_TAG_LEN);

        assert_ne!(
            &srtp[rtp::RTP_HEADER_SIZE..srtp.len() - SRTP_AUTH_TAG_LEN],
            payload.as_slice()
        );

        let mut ctx2 = create_context(&mat, &mat).unwrap();
        let decrypted = unprotect(&mut ctx2, &srtp).unwrap();
        assert_eq!(decrypted, rtp);
    }

    #[test]
    fn test_auth_tag_mismatch() {
        let mat = make_test_material();
        let mut ctx = create_context(&mat, &mat).unwrap();

        let rtp = rtp::encode(rtp::PT_PCMU, 1, 160, 0xDEADBEEF, &[0xFF; 160]);
        let mut srtp = protect(&mut ctx, &rtp).unwrap();

        let len = srtp.len();
        srtp[len - 1] ^= 0xFF;

        let mut ctx2 = create_context(&mat, &mat).unwrap();
        assert!(unprotect(&mut ctx2, &srtp).is_err());
    }

    #[test]
    fn late_packet_from_previous_roc_does_not_regress_receive_state() {
        let mat = make_test_material();
        let mut sender = create_context(&mat, &mat).unwrap();
        let mut old_sender = create_context(&mat, &mat).unwrap();
        let mut receiver = create_context(&mat, &mat).unwrap();
        let payload = [0x42; 160];
        let packet = |seq| rtp::encode(rtp::PT_PCMU, seq, seq as u32 * 160, 0xDEADBEEF, &payload);

        for seq in [0xFFFE, 0xFFFF, 0] {
            let encrypted = protect(&mut sender, &packet(seq)).unwrap();
            unprotect(&mut receiver, &encrypted)
                .unwrap_or_else(|error| panic!("sequence {seq}: {error:#}"));
        }
        assert_eq!((receiver.remote_roc, receiver.remote_highest_seq), (1, 0));

        old_sender.local_roc = 0;
        let late = protect(&mut old_sender, &packet(0xFFFF)).unwrap();
        unprotect(&mut receiver, &late).unwrap();
        assert_eq!((receiver.remote_roc, receiver.remote_highest_seq), (1, 0));

        let next = protect(&mut sender, &packet(1)).unwrap();
        unprotect(&mut receiver, &next).unwrap();
        assert_eq!((receiver.remote_roc, receiver.remote_highest_seq), (1, 1));
    }

    /// Build a minimal RTCP Sender Report (SR) packet for testing.
    fn make_test_rtcp_sr(ssrc: u32) -> Vec<u8> {
        let mut pkt = vec![0u8; 28]; // minimal SR: 8-byte header + 20-byte sender info
        pkt[0] = 0x80;
        pkt[1] = 200;
        pkt[2] = 0;
        pkt[3] = 6;
        let ssrc_bytes = ssrc.to_be_bytes();
        pkt[4..8].copy_from_slice(&ssrc_bytes);
        // Rest is zeros (NTP timestamp, RTP timestamp, counts) — fine for testing
        pkt
    }

    #[test]
    fn test_srtcp_key_derivation() {
        let mat = make_test_material();
        let srtp_keys = derive_session_keys(&mat).unwrap();
        let srtcp_keys = derive_srtcp_session_keys(&mat).unwrap();

        // SRTCP keys must differ from SRTP keys (different labels)
        assert_ne!(srtp_keys.cipher_key, srtcp_keys.cipher_key);
        assert_ne!(srtp_keys.auth_key, srtcp_keys.auth_key);
        assert_ne!(srtp_keys.salt, srtcp_keys.salt);

        assert!(srtcp_keys.cipher_key.iter().any(|&b| b != 0));
        assert!(srtcp_keys.auth_key.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_protect_unprotect_rtcp_roundtrip() {
        let mat = make_test_material();
        let mut ctx = create_context(&mat, &mat).unwrap();

        let rtcp = make_test_rtcp_sr(0xCAFEBABE);
        let srtcp = protect_rtcp(&mut ctx, &rtcp).unwrap();

        // SRTCP = header(8) + encrypted_payload(20) + E||index(4) + auth_tag(10) = 42
        assert_eq!(srtcp.len(), rtcp.len() + 4 + SRTP_AUTH_TAG_LEN);

        // Header (first 8 bytes) should be unchanged
        assert_eq!(&srtcp[..8], &rtcp[..8]);

        assert_ne!(&srtcp[8..28], &rtcp[8..28]);

        let mut ctx2 = create_context(&mat, &mat).unwrap();
        let decrypted = unprotect_rtcp(&mut ctx2, &srtcp).unwrap();
        assert_eq!(decrypted, rtcp);
    }

    #[test]
    fn test_srtcp_auth_tag_mismatch() {
        let mat = make_test_material();
        let mut ctx = create_context(&mat, &mat).unwrap();

        let rtcp = make_test_rtcp_sr(0xDEADC0DE);
        let mut srtcp = protect_rtcp(&mut ctx, &rtcp).unwrap();

        let len = srtcp.len();
        srtcp[len - 1] ^= 0xFF;

        let mut ctx2 = create_context(&mat, &mat).unwrap();
        assert!(unprotect_rtcp(&mut ctx2, &srtcp).is_err());
    }
}
