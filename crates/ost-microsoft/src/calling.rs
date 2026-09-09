pub const SKYPE_CLIENT_HEADER: &str = "SkypeSpaces/1415/teams-cli/TsCallingVersion=2025.49.01.15";
pub const SKYPE_TOKEN_HEADER: &str = "X-Skypetoken";
pub const CHAIN_ID_HEADER: &str = "x-microsoft-skype-chain-id";
pub const MESSAGE_ID_HEADER: &str = "x-microsoft-skype-message-id";
pub const CLIENT_HEADER: &str = "x-microsoft-skype-client";
pub const REFERER_HEADER: &str = "Referer";
pub const TEAMS_REFERER: &str = "https://teams.microsoft.com/";
pub const TEAMS_LIVE_REFERER: &str = "https://teams.live.com/";
pub const PARTITION_HEADER: &str = "ms-teams-partition";
pub const REGION_HEADER: &str = "ms-teams-region";
pub const RING_HEADER: &str = "ms-teams-ring";
pub const MIGRATION_HEADER: &str = "x-ms-migration";
pub const MIGRATION_VALUE: &str = "True";
pub const PROXY_CLUSTER_CONTEXT_HEADER: &str = "x-microsoft-skype-proxy-cluster-context";
pub const TEAMS_PARTITION: &str = "amer03";
pub const TEAMS_REGION: &str = "amer";
pub const TEAMS_RING: &str = "general";
pub const TROUTER_CLIENT_CONTEXT: &str =
    r#"{"cv":"TEAMS_TROUTER_TCCV","ua":"TeamsCDL","hr":"","v":"TEAMS_CLIENTINFO_VERSION"}"#;
pub const TROUTER_NEGOTIATION_BASE: &str = "https://go.trouter.teams.microsoft.com/v4/a";
pub const FLIGHTPROXY_TURN_HOST: &str = "api.flightproxy.teams.microsoft.com";
pub const FLIGHTPROXY_TURN_PORT: u16 = 3478;
pub const FLIGHTPROXY_RELAY_URL: &str =
    "https://api.flightproxy.teams.microsoft.com/api/v2/ep/relay/token";
pub const PERSONAL_CALL_CONVERSATION_URL: &str = "https://api.flightproxy.skype.com/api/v2/cpconv";
pub const PERSONAL_CLIENT_ENDPOINT_CAPABILITIES: u64 = 42_876_960;
pub const RECORDER_BOT_MRI: &str = "28:bdd75849-e0a6-4cce-8fc1-d7c0d4da43e5";
pub const RECORDER_SERVICE_BASE: &str = "https://api.flightproxy.teams.microsoft.com/api/v2/ep/aks-prod-usea-p08-api.callrecorder.teams.cloud.microsoft:23444";
pub const NEXT_GEN_CALL_PATH: &str = "NGCallManagerWin";
pub const ECHO_BOT_MRI: &str = "28:cf28171e-fcfd-47e4-a1d6-79460b0b3ca0";
pub const ECHO_BOT_OID: &str = "cf28171e-fcfd-47e4-a1d6-79460b0b3ca0";

mod incoming;
mod payloads;
mod recording;
mod rtcp;
mod trouter;
pub use incoming::*;
pub use payloads::*;
pub use recording::*;
pub use rtcp::*;
pub use trouter::*;

#[derive(Clone, Copy)]
pub struct RegistrarRegistration {
    pub app_id: &'static str,
    pub template_key: &'static str,
    pub path_suffix: &'static str,
    pub context: &'static str,
}

pub const REGISTRATIONS: &[RegistrarRegistration] = &[
    RegistrarRegistration {
        app_id: "TeamsCDLWebWorker",
        template_key: "TeamsCDLWebWorker_2.6",
        path_suffix: "",
        context: "TFL",
    },
    RegistrarRegistration {
        app_id: "SkypeSpacesWeb",
        template_key: "SkypeSpacesWeb_2.4",
        path_suffix: "SkypeSpacesWeb",
        context: "",
    },
    RegistrarRegistration {
        app_id: "NextGenCalling",
        template_key: "DesktopNgc_2.5:SkypeNgc",
        path_suffix: NEXT_GEN_CALL_PATH,
        context: "",
    },
];

pub fn trouter_negotiation_url(endpoint_id: &str) -> String {
    format!("{TROUTER_NEGOTIATION_BASE}?epid={endpoint_id}")
}

pub fn trouter_callback(trouter_surl: &str, endpoint_id: &str, path: &str) -> String {
    let hash = format!("{:08x}", {
        let mut hash = 0x811c9dc5u32;
        for byte in endpoint_id.bytes().chain(path.bytes()) {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(0x01000193);
        }
        hash
    });
    format!("{trouter_surl}callAgent/{endpoint_id}/{hash}/{path}")
}

pub fn echo_thread_id(caller_oid: &str) -> String {
    format!("19:{caller_oid}_{ECHO_BOT_OID}@unq.gbl.spaces")
}

pub fn call_modalities(include_video: bool) -> &'static [&'static str] {
    if include_video {
        &["Audio", "Video"]
    } else {
        &["Audio"]
    }
}

pub fn registrar_payload(
    registration: RegistrarRegistration,
    registration_id: &str,
    path: &str,
) -> serde_json::Value {
    serde_json::json!({
        "clientDescription": {
            "appId": registration.app_id,
            "aesKey": "",
            "languageId": "en-US",
            "platform": "edge",
            "templateKey": registration.template_key,
            "platformUIVersion": "49/1.0.0"
        },
        "registrationId": registration_id,
        "nodeId": "",
        "transports": {
            "TROUTER": [{
                "context": registration.context,
                "path": path,
                "ttl": 86400
            }]
        }
    })
}
pub const VIDEO_FPS_INDEX: u8 = 4;
