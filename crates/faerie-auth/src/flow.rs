//! The Microsoft → Xbox Live → XSTS → Minecraft token chain.

use std::time::Duration;

use faerie_core::Secret;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::device_code::{is_pending_error, DeviceCodePrompt};
use crate::endpoints::{Endpoints, SCOPE};
use crate::AuthError;

/// A signed-in account. Tokens are `Secret`, so they cannot be printed or
/// logged accidentally, and they are never serialized with the account
/// metadata — see [`crate::store`].
#[derive(Debug, Clone)]
pub struct Account {
    pub profile: MinecraftProfile,
    pub minecraft_token: Secret<String>,
    /// Unix seconds when `minecraft_token` stops being valid.
    pub expires_at_secs: u64,
    pub refresh_token: Secret<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
    /// Xbox user id, needed as a launch argument.
    #[serde(default)]
    pub xuid: String,
}

#[derive(Deserialize)]
struct MsTokenResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct MsErrorResponse {
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<XuiClaim>,
}

#[derive(Deserialize)]
struct XuiClaim {
    /// User hash, required to build the Minecraft login identity token.
    uhs: String,
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Deserialize)]
struct XstsErrorResponse {
    #[serde(rename = "XErr", default)]
    xerr: u64,
}

#[derive(Deserialize)]
struct McLoginResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

pub struct AuthFlow {
    client: reqwest::Client,
    endpoints: Endpoints,
    client_id: String,
}

impl AuthFlow {
    pub fn new(client: reqwest::Client, endpoints: Endpoints, client_id: String) -> Self {
        Self {
            client,
            endpoints,
            client_id,
        }
    }

    fn require_client_id(&self) -> Result<&str, AuthError> {
        if self.client_id.trim().is_empty() {
            return Err(AuthError::NoClientId);
        }
        Ok(&self.client_id)
    }

    /// Step 1: ask Microsoft for a device code to show the user.
    pub async fn start_device_code(&self) -> Result<DeviceCodePrompt, AuthError> {
        let client_id = self.require_client_id()?;
        let response = self
            .client
            .post(&self.endpoints.device_code)
            .form(&[("client_id", client_id), ("scope", SCOPE)])
            .send()
            .await
            .map_err(|e| AuthError::Network {
                stage: "Microsoft device code",
                reason: e.to_string(),
            })?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AuthError::Rejected {
                stage: "Microsoft device code",
                status: status.as_u16(),
                reason: describe_ms_error(&body),
            });
        }
        serde_json::from_str(&body).map_err(|e| AuthError::BadResponse {
            stage: "Microsoft device code",
            reason: e.to_string(),
        })
    }

    /// Step 2: poll until the user finishes signing in, then run the rest of
    /// the chain. `on_wait` is called before each sleep so the UI can show a
    /// countdown; it returns `false` to abort.
    pub async fn complete_device_code(
        &self,
        prompt: &DeviceCodePrompt,
        mut on_wait: impl FnMut(u64) -> bool,
    ) -> Result<Account, AuthError> {
        let client_id = self.require_client_id()?;
        let mut interval = prompt.interval.max(1);
        let deadline = std::time::Instant::now() + Duration::from_secs(prompt.expires_in.max(60));

        loop {
            if std::time::Instant::now() >= deadline {
                return Err(AuthError::DeviceCodeExpired);
            }
            if !on_wait(interval) {
                return Err(AuthError::DeclinedByUser);
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;

            let response = self
                .client
                .post(&self.endpoints.token)
                .form(&[
                    ("client_id", client_id),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", &prompt.device_code),
                ])
                .send()
                .await
                .map_err(|e| AuthError::Network {
                    stage: "Microsoft token",
                    reason: e.to_string(),
                })?;

            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status.is_success() {
                let tokens: MsTokenResponse =
                    serde_json::from_str(&body).map_err(|e| AuthError::BadResponse {
                        stage: "Microsoft token",
                        reason: e.to_string(),
                    })?;
                return self.exchange_for_minecraft(tokens).await;
            }

            let error: MsErrorResponse = serde_json::from_str(&body).unwrap_or(MsErrorResponse {
                error: "unknown_error".into(),
                error_description: body.clone(),
            });
            match error.error.as_str() {
                e if is_pending_error(e) => {
                    if e == "slow_down" {
                        interval += 5; // Microsoft asks us to back off
                    }
                }
                "authorization_declined" => return Err(AuthError::DeclinedByUser),
                "expired_token" => return Err(AuthError::DeviceCodeExpired),
                _ => {
                    return Err(AuthError::Rejected {
                        stage: "Microsoft token",
                        status: status.as_u16(),
                        reason: if error.error_description.is_empty() {
                            error.error
                        } else {
                            error.error_description
                        },
                    })
                }
            }
        }
    }

    /// Refresh an expired Minecraft token using a stored refresh token.
    pub async fn refresh(&self, refresh_token: &Secret<String>) -> Result<Account, AuthError> {
        let client_id = self.require_client_id()?;
        let response = self
            .client
            .post(&self.endpoints.token)
            .form(&[
                ("client_id", client_id),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose().as_str()),
                ("scope", SCOPE),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Network {
                stage: "Microsoft refresh",
                reason: e.to_string(),
            })?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AuthError::Rejected {
                stage: "Microsoft refresh",
                status: status.as_u16(),
                reason: describe_ms_error(&body),
            });
        }
        let tokens: MsTokenResponse =
            serde_json::from_str(&body).map_err(|e| AuthError::BadResponse {
                stage: "Microsoft refresh",
                reason: e.to_string(),
            })?;
        self.exchange_for_minecraft(tokens).await
    }

    /// Xbox Live → XSTS → Minecraft services → entitlement → profile.
    async fn exchange_for_minecraft(&self, ms: MsTokenResponse) -> Result<Account, AuthError> {
        // Xbox Live user token.
        let xbl = self
            .post_json::<XboxResponse>(
                &self.endpoints.xbox_authenticate,
                "Xbox Live",
                &json!({
                    "Properties": {
                        "AuthMethod": "RPS",
                        "SiteName": "user.auth.xboxlive.com",
                        "RpsTicket": format!("d={}", ms.access_token),
                    },
                    "RelyingParty": "http://auth.xboxlive.com",
                    "TokenType": "JWT",
                }),
                None,
            )
            .await?;
        let user_hash = xbl
            .display_claims
            .xui
            .first()
            .map(|c| c.uhs.clone())
            .ok_or(AuthError::BadResponse {
                stage: "Xbox Live",
                reason: "no user hash in DisplayClaims".into(),
            })?;

        // XSTS authorization (this is where child/no-account errors surface).
        let xsts = self
            .post_json_raw(
                &self.endpoints.xsts_authorize,
                "XSTS",
                &json!({
                    "Properties": {
                        "SandboxId": "RETAIL",
                        "UserTokens": [xbl.token],
                    },
                    "RelyingParty": "rp://api.minecraftservices.com/",
                    "TokenType": "JWT",
                }),
                None,
            )
            .await?;
        let xsts: XboxResponse = match xsts {
            RawResponse::Ok(body) => {
                serde_json::from_str(&body).map_err(|e| AuthError::BadResponse {
                    stage: "XSTS",
                    reason: e.to_string(),
                })?
            }
            RawResponse::Err { status, body } => {
                // Microsoft encodes the specific reason in XErr.
                let xerr = serde_json::from_str::<XstsErrorResponse>(&body)
                    .map(|e| e.xerr)
                    .unwrap_or(0);
                return Err(match xerr {
                    2148916233 => AuthError::NoXboxAccount,
                    2148916238 => AuthError::ChildAccount,
                    _ => AuthError::Rejected {
                        stage: "XSTS",
                        status,
                        reason: body,
                    },
                });
            }
        };
        let xuid = xsts
            .display_claims
            .xui
            .first()
            .and_then(|c| c.xid.clone())
            .unwrap_or_default();

        // Minecraft services login.
        let mc: McLoginResponse = self
            .post_json(
                &self.endpoints.minecraft_login,
                "Minecraft services",
                &json!({ "identityToken": format!("XBL3.0 x={user_hash};{}", xsts.token) }),
                None,
            )
            .await?;
        let mc_token = Secret::new(mc.access_token);

        // Profile (name + uuid). This doubles as the ownership check: the
        // profile endpoint answers 404 for an account without Java Edition.
        // The store entitlement list (`entitlements/mcstore`) is deliberately
        // not consulted — it comes back empty for accounts that have the game
        // through Game Pass, so gating on it rejects paying players.
        let mut profile = match self
            .get_json::<MinecraftProfile>(
                &self.endpoints.minecraft_profile,
                "Minecraft profile",
                &mc_token,
            )
            .await
        {
            Ok(profile) => profile,
            Err(AuthError::Rejected { status: 404, .. }) => {
                return Err(AuthError::NoMinecraftEntitlement)
            }
            Err(e) => return Err(e),
        };
        if profile.xuid.is_empty() {
            profile.xuid = xuid;
        }

        let expires_at_secs = now_secs()
            + if mc.expires_in > 0 {
                mc.expires_in
            } else {
                86_400
            };
        Ok(Account {
            profile,
            minecraft_token: mc_token,
            expires_at_secs,
            refresh_token: Secret::new(ms.refresh_token),
        })
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        stage: &'static str,
        body: &serde_json::Value,
        bearer: Option<&Secret<String>>,
    ) -> Result<T, AuthError> {
        match self.post_json_raw(url, stage, body, bearer).await? {
            RawResponse::Ok(text) => {
                serde_json::from_str(&text).map_err(|e| AuthError::BadResponse {
                    stage,
                    reason: e.to_string(),
                })
            }
            RawResponse::Err { status, body } => Err(AuthError::Rejected {
                stage,
                status,
                reason: body,
            }),
        }
    }

    async fn post_json_raw(
        &self,
        url: &str,
        stage: &'static str,
        body: &serde_json::Value,
        bearer: Option<&Secret<String>>,
    ) -> Result<RawResponse, AuthError> {
        let mut request = self.client.post(url).json(body);
        if let Some(token) = bearer {
            request = request.bearer_auth(token.expose());
        }
        let response = request.send().await.map_err(|e| AuthError::Network {
            stage,
            reason: e.to_string(),
        })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Ok(if status.is_success() {
            RawResponse::Ok(text)
        } else {
            RawResponse::Err {
                status: status.as_u16(),
                body: text,
            }
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        stage: &'static str,
        bearer: &Secret<String>,
    ) -> Result<T, AuthError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(bearer.expose())
            .send()
            .await
            .map_err(|e| AuthError::Network {
                stage,
                reason: e.to_string(),
            })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AuthError::Rejected {
                stage,
                status: status.as_u16(),
                reason: text,
            });
        }
        serde_json::from_str(&text).map_err(|e| AuthError::BadResponse {
            stage,
            reason: e.to_string(),
        })
    }
}

enum RawResponse {
    Ok(String),
    Err { status: u16, body: String },
}

fn describe_ms_error(body: &str) -> String {
    serde_json::from_str::<MsErrorResponse>(body)
        .map(|e| {
            if e.error_description.is_empty() {
                e.error
            } else {
                e.error_description
            }
        })
        .unwrap_or_else(|_| body.to_string())
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_debug_never_reveals_tokens() {
        let account = Account {
            profile: MinecraftProfile {
                id: "uuid".into(),
                name: "Faerie".into(),
                xuid: "123".into(),
            },
            minecraft_token: Secret::new("MC_SECRET_TOKEN".into()),
            expires_at_secs: 0,
            refresh_token: Secret::new("REFRESH_SECRET".into()),
        };
        let debug = format!("{account:?}");
        assert!(!debug.contains("MC_SECRET_TOKEN"));
        assert!(!debug.contains("REFRESH_SECRET"));
        assert!(debug.contains("Faerie"));
    }

    #[test]
    fn microsoft_error_bodies_become_readable_text() {
        let body = r#"{"error":"invalid_grant","error_description":"AADSTS70000: expired"}"#;
        assert!(describe_ms_error(body).contains("AADSTS70000"));
        assert_eq!(describe_ms_error("not json"), "not json");
    }
}
