//! Device-code sign-in: the user opens a URL and types a short code.
//!
//! Chosen as the primary flow because it needs no embedded browser, no
//! loopback listener, and no client secret — the user authenticates on
//! Microsoft's own page and we only ever receive tokens.

use serde::{Deserialize, Serialize};

/// What the user must be shown to complete sign-in.
///
/// `device_code` is a bearer-like handle: anyone holding it can claim the
/// tokens from a completed sign-in. It is therefore never serialized, so it
/// cannot reach the webview, a log line, or a config file. The backend keeps
/// the prompt and polls with it itself.
///
/// Two wire formats meet here: Microsoft's response is snake_case, while the
/// launcher's IPC payloads are camelCase like every other struct the webview
/// receives. Renaming only on serialize keeps both sides honest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DeviceCodePrompt {
    /// Opaque handle used when polling for the token.
    #[serde(skip_serializing)]
    pub device_code: String,
    /// The short code the user types.
    pub user_code: String,
    /// Where the user enters it.
    pub verification_uri: String,
    /// Seconds until this code expires.
    pub expires_in: u64,
    /// Seconds to wait between polls (server-dictated).
    pub interval: u64,
    /// Microsoft's own human-readable instruction text.
    #[serde(default)]
    pub message: String,
}

/// Progress of the polling loop, surfaced to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PollState {
    /// The user has not finished yet; keep waiting.
    Pending,
    /// Sign-in completed and tokens were issued.
    Complete,
}

/// Microsoft's OAuth error codes that are not fatal during polling.
pub(crate) fn is_pending_error(code: &str) -> bool {
    matches!(code, "authorization_pending" | "slow_down")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_parses_microsoft_shape() {
        let raw = r#"{
            "device_code": "DEVICE-ABC",
            "user_code": "H4KL2M9",
            "verification_uri": "https://microsoft.com/link",
            "expires_in": 900,
            "interval": 5,
            "message": "To sign in, use a web browser…"
        }"#;
        let prompt: DeviceCodePrompt = serde_json::from_str(raw).unwrap();
        assert_eq!(prompt.user_code, "H4KL2M9");
        assert_eq!(prompt.interval, 5);
    }

    #[test]
    fn device_code_is_never_serialized_back_out() {
        let prompt = DeviceCodePrompt {
            device_code: "SECRET-HANDLE".into(),
            user_code: "H4KL2M9".into(),
            verification_uri: "https://microsoft.com/link".into(),
            expires_in: 900,
            interval: 5,
            message: String::new(),
        };
        let json = serde_json::to_string(&prompt).unwrap();
        assert!(
            !json.contains("SECRET-HANDLE"),
            "device_code must not leak to the UI"
        );
        assert!(json.contains("H4KL2M9"));
    }

    /// The webview types this as camelCase (`userCode`, `verificationUri`);
    /// a snake_case leak shows up as "Open undefined in your browser".
    #[test]
    fn prompt_serializes_camel_case_for_the_ui() {
        let prompt = DeviceCodePrompt {
            device_code: "SECRET-HANDLE".into(),
            user_code: "H4KL2M9".into(),
            verification_uri: "https://microsoft.com/link".into(),
            expires_in: 900,
            interval: 5,
            message: "open the page".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&prompt).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "expiresIn",
                "interval",
                "message",
                "userCode",
                "verificationUri"
            ],
            "IPC payloads are camelCase and never carry the device code"
        );
        assert_eq!(json["verificationUri"], "https://microsoft.com/link");
    }

    #[test]
    fn pending_errors_are_recognized() {
        assert!(is_pending_error("authorization_pending"));
        assert!(is_pending_error("slow_down"));
        assert!(!is_pending_error("expired_token"));
        assert!(!is_pending_error("authorization_declined"));
    }
}
