pub mod auth {
    pub const WORK_CLIENT_ID: &str = "1fec8e78-bce4-4aaf-ab1b-5451cc387264";
    pub const PERSONAL_CLIENT_ID: &str = "8ec6bc83-69c8-4392-8f08-b3c986009232";
    pub const PERSONAL_TENANT_ID: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";
    pub const PERSONAL_TEAMS_SCOPE: &str =
        "https://mtsvc.fl.teams.microsoft.com/teams.mt.readwrite openid profile offline_access";
    pub const PERSONAL_SKYPE_SCOPE: &str =
        "service::api.fl.spaces.skype.com::MBI_SSL openid profile offline_access";
    pub const NATIVE_REDIRECT_URI: &str =
        "https://login.microsoftonline.com/common/oauth2/nativeclient";
    pub const ORGANIZATIONS_AUTHORITY: &str =
        "https://login.microsoftonline.com/organizations/oauth2/v2.0";
    pub const TEAMS_RESOURCE_SCOPE: &str = "https://api.spaces.skype.com/.default";
    pub const GRAPH_RESOURCE_SCOPE: &str = "https://graph.microsoft.com/.default";
    pub const IC3_RESOURCE_SCOPE: &str = "https://ic3.teams.office.com/.default";
    pub const TEAMS_SCOPE: &str = "https://api.spaces.skype.com/.default offline_access";
    pub const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default offline_access";
    pub const IC3_SCOPE: &str = "https://ic3.teams.office.com/.default offline_access";
    pub const RECORDER_SCOPE: &str = "4580fd1d-e5a3-4f56-9ad1-aab0e3bf8f76/.default";
    pub const WORK_AUTHZ_URL: &str = "https://teams.microsoft.com/api/authsvc/v1.0/authz";
    pub const PERSONAL_AUTHZ_URL: &str = "https://teams.live.com/api/auth/v1.0/authz/consumer";

    pub fn oauth_url(tenant: &str, endpoint: &str) -> String {
        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/{endpoint}")
    }
}

pub mod teams;

pub mod calling;
