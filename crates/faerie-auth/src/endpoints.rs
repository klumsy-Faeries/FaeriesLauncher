//! Endpoint configuration. Real URLs by default; overridable so the whole
//! token chain can be exercised against a local test server.

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub device_code: String,
    pub token: String,
    pub xbox_authenticate: String,
    pub xsts_authorize: String,
    pub minecraft_login: String,
    pub minecraft_profile: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            device_code: "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
                .into(),
            token: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token".into(),
            xbox_authenticate: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts_authorize: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            minecraft_login: "https://api.minecraftservices.com/authentication/login_with_xbox"
                .into(),
            minecraft_profile: "https://api.minecraftservices.com/minecraft/profile".into(),
        }
    }
}

impl Endpoints {
    /// Point every endpoint at one base URL (test server).
    pub fn all_at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            device_code: format!("{base}/devicecode"),
            token: format!("{base}/token"),
            xbox_authenticate: format!("{base}/xbox/authenticate"),
            xsts_authorize: format!("{base}/xsts/authorize"),
            minecraft_login: format!("{base}/mc/login"),
            minecraft_profile: format!("{base}/mc/profile"),
        }
    }
}

/// OAuth scope needed for the Xbox Live token exchange.
pub const SCOPE: &str = "XboxLive.signin offline_access";
