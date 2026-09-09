use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SdpOfferInfo {
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub crypto_line: String,
    pub crypto_lines: Vec<String>,
    pub candidate_ip: String,
    pub candidate_port: u16,
    pub video: Option<SdpVideoInfo>,
}

#[derive(Debug, Clone)]
pub struct SdpVideoInfo {
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub crypto_lines: Vec<String>,
    pub candidate_ip: String,
    pub candidate_port: u16,
}

pub(crate) fn main_video_section(blob: &str) -> Option<String> {
    let blob =
        crate::calling::sdp_compress::decompress_sdp(blob).unwrap_or_else(|_| blob.to_string());
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut in_video = false;

    for line in blob.lines().map(str::trim) {
        if line.starts_with("m=") {
            if in_video && !current.is_empty() {
                sections.push(std::mem::take(&mut current));
            }
            in_video = line.starts_with("m=video");
        }
        if in_video {
            current.push_str(line);
            current.push_str("\r\n");
        }
    }
    if in_video && !current.is_empty() {
        sections.push(current);
    }

    let mut best = None;
    let mut best_score = 0;
    for section in sections {
        let lower = section.to_ascii_lowercase();
        let score =
            if lower.contains("a=label:main-video") || lower.contains("a=x-source:main-video") {
                3
            } else if lower.contains("x-h264uc/90000") {
                2
            } else {
                1
            };
        if score > best_score {
            best_score = score;
            best = Some(section);
        }
    }
    best
}

pub fn parse_sdp_offer(blob: &str) -> Result<SdpOfferInfo> {
    let blob =
        crate::calling::sdp_compress::decompress_sdp(blob).unwrap_or_else(|_| blob.to_string());
    let mut ice_ufrag = String::new();
    let mut ice_pwd = String::new();
    let mut crypto_line = String::new();
    let mut crypto_lines = Vec::new();
    let mut candidate_ip = String::new();
    let mut candidate_port = 0;

    #[derive(PartialEq)]
    enum Section {
        Session,
        Audio,
        Video,
        Other,
    }

    let mut section = Section::Session;
    for line in blob.lines().map(str::trim) {
        if line.starts_with("m=audio") {
            section = Section::Audio;
        } else if line.starts_with("m=video") {
            section = Section::Video;
        } else if line.starts_with("m=") {
            section = Section::Other;
        }

        if let Some(value) = line.strip_prefix("a=ice-ufrag:") {
            if section == Section::Audio || section == Section::Session && ice_ufrag.is_empty() {
                ice_ufrag = value.to_string();
            }
        }
        if let Some(value) = line.strip_prefix("a=ice-pwd:") {
            if section == Section::Audio || section == Section::Session && ice_pwd.is_empty() {
                ice_pwd = value.to_string();
            }
        }
        if (line.starts_with("a=crypto:") || line.starts_with("a=cryptoscale:"))
            && section == Section::Audio
        {
            if crypto_line.is_empty() {
                crypto_line = line.to_string();
            }
            crypto_lines.push(line.to_string());
        }
        if line.starts_with("a=candidate:") && section == Section::Audio && candidate_ip.is_empty()
        {
            let mut parts = line.split_whitespace();
            if let Some(address) = parts.nth(4) {
                candidate_ip = address.to_owned();
                candidate_port = parts.next().and_then(|port| port.parse().ok()).unwrap_or(0);
            }
        }
    }

    if ice_ufrag.is_empty() {
        anyhow::bail!("Could not extract ice-ufrag from SDP offer");
    }

    let video = main_video_section(&blob).map(|video_section| {
        let mut video_ice_ufrag = String::new();
        let mut video_ice_pwd = String::new();
        let mut video_crypto_lines = Vec::new();
        let mut video_candidate_ip = String::new();
        let mut video_candidate_port = 0;

        for line in video_section.lines().map(str::trim) {
            if let Some(value) = line.strip_prefix("a=ice-ufrag:") {
                video_ice_ufrag = value.to_string();
            }
            if let Some(value) = line.strip_prefix("a=ice-pwd:") {
                video_ice_pwd = value.to_string();
            }
            if line.starts_with("a=crypto:") || line.starts_with("a=cryptoscale:") {
                video_crypto_lines.push(line.to_string());
            }
            if line.starts_with("a=candidate:") && video_candidate_ip.is_empty() {
                let mut parts = line.split_whitespace();
                if let Some(address) = parts.nth(4) {
                    video_candidate_ip = address.to_owned();
                    video_candidate_port =
                        parts.next().and_then(|port| port.parse().ok()).unwrap_or(0);
                }
            }
        }

        SdpVideoInfo {
            ice_ufrag: if video_ice_ufrag.is_empty() {
                ice_ufrag.clone()
            } else {
                video_ice_ufrag
            },
            ice_pwd: if video_ice_pwd.is_empty() {
                ice_pwd.clone()
            } else {
                video_ice_pwd
            },
            crypto_lines: if video_crypto_lines.is_empty() {
                crypto_lines.clone()
            } else {
                video_crypto_lines
            },
            candidate_ip: if video_candidate_ip.is_empty() {
                candidate_ip.clone()
            } else {
                video_candidate_ip
            },
            candidate_port: if video_candidate_port == 0 {
                candidate_port
            } else {
                video_candidate_port
            },
        }
    });

    Ok(SdpOfferInfo {
        ice_ufrag,
        ice_pwd,
        crypto_line,
        crypto_lines,
        candidate_ip,
        candidate_port,
        video,
    })
}
