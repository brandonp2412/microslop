use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, Result, bail};

// ICE candidate types (unchanged from original)

/// ICE candidate type.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateType {
    Host,
    ServerReflexive,
    Relay,
}

/// ICE transport protocol.
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    Udp,
    TcpActive,
    TcpPassive,
}

/// Parsed ICE candidate from SDP.
#[derive(Debug, Clone)]
pub struct IceCandidate {
    pub foundation: String,
    pub component: u8,
    pub transport: Transport,
    pub priority: u32,
    pub address: String,
    pub port: u16,
    pub candidate_type: CandidateType,
    /// For srflx/relay: the related address.
    pub raddr: Option<String>,
    pub rport: Option<u16>,
}

impl IceCandidate {
    /// Format this candidate as an SDP `a=candidate:` line (without the `a=` prefix).
    pub fn to_sdp_line(&self) -> String {
        let transport_str = match self.transport {
            Transport::Udp => "UDP",
            Transport::TcpActive => "TCP-ACT",
            Transport::TcpPassive => "TCP-PASS",
        };
        let type_str = match self.candidate_type {
            CandidateType::Host => "host",
            CandidateType::ServerReflexive => "srflx",
            CandidateType::Relay => "relay",
        };
        let mut line = format!(
            "candidate:{} {} {} {} {} {} typ {}",
            self.foundation,
            self.component,
            transport_str,
            self.priority,
            self.address,
            self.port,
            type_str
        );
        if let (Some(ref ra), Some(rp)) = (&self.raddr, self.rport) {
            line.push_str(&format!(" raddr {} rport {}", ra, rp));
        }
        line
    }
}

/// ICE credentials (ufrag + pwd).
#[derive(Debug, Clone)]
pub struct IceCredentials {
    pub ufrag: String,
    pub pwd: String,
}

/// Parse an `a=candidate:` line from SDP into an IceCandidate.
pub fn parse_candidate(line: &str) -> Result<IceCandidate> {
    let line = line.trim();
    let content = line
        .strip_prefix("a=candidate:")
        .or_else(|| line.strip_prefix("candidate:"))
        .ok_or_else(|| anyhow::anyhow!("not a candidate line: {}", line))?;

    let mut parts = content.split_whitespace();
    let mut next = || {
        parts
            .next()
            .with_context(|| format!("candidate line too short: {line}"))
    };
    let foundation = next()?.to_owned();
    let component: u8 = next()?.parse().context("bad component")?;
    let transport_name = next()?;
    let transport = if transport_name.eq_ignore_ascii_case("UDP") {
        Transport::Udp
    } else if transport_name.eq_ignore_ascii_case("TCP-ACT") {
        Transport::TcpActive
    } else if transport_name.eq_ignore_ascii_case("TCP-PASS") {
        Transport::TcpPassive
    } else {
        bail!("unsupported transport: {transport_name}");
    };
    let priority = next()?.parse().context("bad priority")?;
    let address = next()?.to_owned();
    let port = next()?.parse().context("bad port")?;
    let type_keyword = next()?;
    if type_keyword != "typ" {
        bail!("expected 'typ' keyword at position 6, got: {type_keyword}");
    }
    let candidate_type = match parts.next().context("missing candidate type")? {
        "host" => CandidateType::Host,
        "srflx" => CandidateType::ServerReflexive,
        "relay" => CandidateType::Relay,
        other => bail!("unknown candidate type: {other}"),
    };

    let mut raddr = None;
    let mut rport = None;
    while let Some(field) = parts.next() {
        match field {
            "raddr" => {
                if let Some(value) = parts.next() {
                    raddr = Some(value.to_owned());
                }
            }
            "rport" => {
                if let Some(value) = parts.next() {
                    rport = Some(value.parse().context("bad rport")?);
                }
            }
            _ => {}
        }
    }

    Ok(IceCandidate {
        foundation,
        component,
        transport,
        priority,
        address,
        port,
        candidate_type,
        raddr,
        rport,
    })
}

/// Parse all ICE candidates from an SDP string (audio section only).
pub fn parse_candidates_from_sdp(sdp: &str) -> Vec<IceCandidate> {
    parse_candidates_from_sdp_section(sdp, "audio")
}

/// Parse all ICE candidates from a specific SDP media section.
pub fn parse_candidates_from_sdp_section(sdp: &str, section_type: &str) -> Vec<IceCandidate> {
    let sdp = crate::calling::sdp_compress::decompress_sdp(sdp).unwrap_or_else(|_| sdp.to_string());
    let mut candidates = Vec::new();
    let mut in_target = false;
    let prefix = format!("m={}", section_type);

    for line in sdp.lines() {
        let line = line.trim();
        if line.starts_with("m=") {
            if in_target {
                break;
            }
            in_target = line.starts_with(&prefix);
        }

        if in_target && line.starts_with("a=candidate:") {
            if let Ok(c) = parse_candidate(line) {
                candidates.push(c);
            }
        }
    }

    candidates
}

pub fn parse_main_video_candidates_from_sdp(sdp: &str) -> Vec<IceCandidate> {
    crate::calling::sdp::main_video_section(sdp)
        .map(|section| parse_candidates_from_sdp_section(&section, "video"))
        .unwrap_or_default()
}

/// Select the best remote candidate for direct UDP connectivity (no STUN check).
pub fn select_remote_candidate(candidates: &[IceCandidate]) -> Option<SocketAddr> {
    let mut best = None;
    for candidate in candidates {
        if candidate.transport != Transport::Udp
            || candidate.component != 1
            || best.is_some_and(|(priority, _)| candidate.priority <= priority)
        {
            continue;
        }
        if let Ok(ip) = candidate.address.parse::<IpAddr>() {
            best = Some((candidate.priority, SocketAddr::new(ip, candidate.port)));
        }
    }
    best.map(|(_, address)| address)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(address: &str, port: u16, priority: u32) -> IceCandidate {
        IceCandidate {
            foundation: "1".to_owned(),
            component: 1,
            transport: Transport::Udp,
            priority,
            address: address.to_owned(),
            port,
            candidate_type: CandidateType::Host,
            raddr: None,
            rport: None,
        }
    }

    #[test]
    fn selects_highest_priority_parseable_udp_candidate() {
        let invalid = candidate("invalid", 1000, 500);
        let valid = candidate("10.0.0.2", 2000, 400);
        let lower = candidate("10.0.0.3", 3000, 300);
        assert_eq!(
            select_remote_candidate(&[lower, invalid, valid]),
            Some("10.0.0.2:2000".parse().unwrap())
        );
    }

    #[test]
    fn selects_ipv6_candidate_without_string_reformatting() {
        let ipv6 = candidate("2001:db8::1", 4000, 500);
        assert_eq!(
            select_remote_candidate(&[ipv6]),
            Some("[2001:db8::1]:4000".parse().unwrap())
        );
    }

    #[test]
    fn main_video_candidates_do_not_mix_multiple_video_sections() {
        let sdp = "v=0\r\nm=video 40000 RTP/SAVP 122\r\na=label:applicationsharing-video\r\na=rtpmap:122 X-H264UC/90000\r\na=candidate:1 1 UDP 100 10.0.0.10 40000 typ host\r\nm=video 50000 RTP/SAVP 122\r\na=label:main-video\r\na=rtpmap:122 X-H264UC/90000\r\na=candidate:2 1 UDP 100 10.0.0.20 50000 typ host\r\n";

        let generic = parse_candidates_from_sdp_section(sdp, "video");
        assert_eq!(generic.len(), 1);
        assert_eq!(generic[0].address, "10.0.0.10");

        let main = parse_main_video_candidates_from_sdp(sdp);
        assert_eq!(main.len(), 1);
        assert_eq!(main[0].address, "10.0.0.20");
        assert_eq!(main[0].port, 50000);
    }
}
