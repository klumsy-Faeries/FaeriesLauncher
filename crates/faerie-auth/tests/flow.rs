//! End-to-end auth chain against a local mock of Microsoft/Xbox/Mojang.
//! No real credentials, no network — every stage is exercised, including the
//! device-code polling loop and each specific failure mode.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use faerie_auth::device_code::DeviceCodePrompt;
use faerie_auth::{AuthError, AuthFlow, Endpoints};
use faerie_core::Secret;
use test_support::{Request, Response, TestServer};

const CLIENT_ID: &str = "00000000-face-0000-0000-000000000000";

fn json_ok(body: &str) -> Response {
    Response::ok(body.as_bytes().to_vec()).with_header("Content-Type", "application/json")
}

fn json_err(status: u16, body: &str) -> Response {
    json_ok(body).with_status(status)
}

/// A mock of the whole chain. `pending_polls` token requests return
/// authorization_pending before success.
fn mock_chain(pending_polls: usize) -> impl Fn(&Request) -> Response + Send + Sync + 'static {
    let polls = Arc::new(AtomicUsize::new(0));
    move |req: &Request| match req.path.as_str() {
        "/devicecode" => json_ok(
            r#"{"device_code":"DEV-HANDLE","user_code":"FAERIE1",
                "verification_uri":"https://microsoft.com/link",
                "expires_in":900,"interval":1,"message":"Go type the code"}"#,
        ),
        "/token" => {
            let n = polls.fetch_add(1, Ordering::SeqCst);
            if n < pending_polls {
                json_err(400, r#"{"error":"authorization_pending"}"#)
            } else {
                json_ok(
                    r#"{"access_token":"MS_ACCESS","refresh_token":"MS_REFRESH","expires_in":3600}"#,
                )
            }
        }
        "/xbox/authenticate" => {
            // The Microsoft access token must be forwarded as an RpsTicket.
            assert!(
                req.body_string().contains("d=MS_ACCESS"),
                "xbox stage did not receive the MS token: {}",
                req.body_string()
            );
            json_ok(r#"{"Token":"XBL_TOKEN","DisplayClaims":{"xui":[{"uhs":"USERHASH"}]}}"#)
        }
        "/xsts/authorize" => {
            assert!(
                req.body_string().contains("XBL_TOKEN"),
                "xsts stage did not receive the Xbox token"
            );
            json_ok(
                r#"{"Token":"XSTS_TOKEN","DisplayClaims":{"xui":[{"uhs":"USERHASH","xid":"XUID-42"}]}}"#,
            )
        }
        "/mc/login" => {
            // Identity token format: XBL3.0 x=<userhash>;<xsts token>
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&req.body_string()).unwrap()
                    ["identityToken"],
                "XBL3.0 x=USERHASH;XSTS_TOKEN"
            );
            json_ok(r#"{"access_token":"MC_TOKEN","expires_in":86400}"#)
        }
        "/mc/profile" => {
            assert_eq!(req.header("authorization"), Some("Bearer MC_TOKEN"));
            json_ok(r#"{"id":"UUID-1234","name":"FaeriePlayer"}"#)
        }
        _ => json_err(404, r#"{"error":"not_found"}"#),
    }
}

fn flow(server: &TestServer) -> AuthFlow {
    AuthFlow::new(
        faerie_net::build_client(),
        Endpoints::all_at(&server.url("")),
        CLIENT_ID.to_string(),
    )
}

#[tokio::test]
async fn full_device_code_chain_produces_an_account() {
    let server = TestServer::start(mock_chain(2)).await;
    let flow = flow(&server);

    let prompt: DeviceCodePrompt = flow.start_device_code().await.unwrap();
    assert_eq!(prompt.user_code, "FAERIE1");
    assert_eq!(prompt.verification_uri, "https://microsoft.com/link");

    let mut waits = 0;
    let account = flow
        .complete_device_code(&prompt, |_| {
            waits += 1;
            true
        })
        .await
        .unwrap();

    assert_eq!(account.profile.name, "FaeriePlayer");
    assert_eq!(account.profile.id, "UUID-1234");
    assert_eq!(
        account.profile.xuid, "XUID-42",
        "xuid comes from the XSTS claims"
    );
    assert_eq!(account.minecraft_token.expose(), "MC_TOKEN");
    assert_eq!(account.refresh_token.expose(), "MS_REFRESH");
    assert!(account.expires_at_secs > 0);
    assert!(waits >= 3, "polled through the pending responses");
}

#[tokio::test]
async fn refresh_token_yields_a_fresh_account() {
    let server = TestServer::start(mock_chain(0)).await;
    let account = flow(&server)
        .refresh(&Secret::new("STORED_REFRESH".into()))
        .await
        .unwrap();
    assert_eq!(account.profile.name, "FaeriePlayer");
    assert_eq!(account.minecraft_token.expose(), "MC_TOKEN");
}

#[tokio::test]
async fn missing_client_id_is_an_actionable_error() {
    let server = TestServer::start(mock_chain(0)).await;
    let flow = AuthFlow::new(
        faerie_net::build_client(),
        Endpoints::all_at(&server.url("")),
        String::new(),
    );
    let error = flow.start_device_code().await.unwrap_err();
    assert!(matches!(error, AuthError::NoClientId));
    assert!(error.to_string().contains("Azure application"));
}

#[tokio::test]
async fn account_without_xbox_profile_is_named_precisely() {
    let server = TestServer::start(|req: &Request| match req.path.as_str() {
        "/token" => json_ok(r#"{"access_token":"A","refresh_token":"R","expires_in":3600}"#),
        "/xbox/authenticate" => json_ok(r#"{"Token":"T","DisplayClaims":{"xui":[{"uhs":"H"}]}}"#),
        // XErr 2148916233 = the Microsoft account has no Xbox Live account.
        "/xsts/authorize" => json_err(401, r#"{"XErr":2148916233}"#),
        _ => json_err(404, "{}"),
    })
    .await;

    let error = flow(&server)
        .refresh(&Secret::new("R".into()))
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::NoXboxAccount), "got {error:?}");
    assert!(error.to_string().contains("xbox.com"));
}

#[tokio::test]
async fn child_account_gets_its_own_message() {
    let server = TestServer::start(|req: &Request| match req.path.as_str() {
        "/token" => json_ok(r#"{"access_token":"A","refresh_token":"R","expires_in":1}"#),
        "/xbox/authenticate" => json_ok(r#"{"Token":"T","DisplayClaims":{"xui":[{"uhs":"H"}]}}"#),
        "/xsts/authorize" => json_err(401, r#"{"XErr":2148916238}"#),
        _ => json_err(404, "{}"),
    })
    .await;

    let error = flow(&server)
        .refresh(&Secret::new("R".into()))
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::ChildAccount));
    assert!(error.to_string().contains("family"));
}

/// Minecraft services answers 404 on the profile endpoint for an account
/// that has no Java Edition profile. That is the ownership signal.
#[tokio::test]
async fn account_without_a_java_profile_is_rejected() {
    let server = TestServer::start(|req: &Request| match req.path.as_str() {
        "/token" => json_ok(r#"{"access_token":"A","refresh_token":"R","expires_in":1}"#),
        "/xbox/authenticate" => json_ok(r#"{"Token":"T","DisplayClaims":{"xui":[{"uhs":"H"}]}}"#),
        "/xsts/authorize" => {
            json_ok(r#"{"Token":"X","DisplayClaims":{"xui":[{"uhs":"H","xid":"1"}]}}"#)
        }
        "/mc/login" => json_ok(r#"{"access_token":"MC","expires_in":1}"#),
        "/mc/profile" => json_err(
            404,
            r#"{"path":"/minecraft/profile","errorType":"NOT_FOUND","error":"NOT_FOUND"}"#,
        ),
        _ => json_err(404, "{}"),
    })
    .await;

    let error = flow(&server)
        .refresh(&Secret::new("R".into()))
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::NoMinecraftEntitlement));
}

/// Game Pass accounts have an empty store entitlement list but a perfectly
/// good profile. Sign-in must not consult the store list at all.
#[tokio::test]
async fn game_pass_account_signs_in_without_store_entitlements() {
    let server = TestServer::start(|req: &Request| match req.path.as_str() {
        "/token" => json_ok(r#"{"access_token":"A","refresh_token":"R","expires_in":1}"#),
        "/xbox/authenticate" => json_ok(r#"{"Token":"T","DisplayClaims":{"xui":[{"uhs":"H"}]}}"#),
        "/xsts/authorize" => {
            json_ok(r#"{"Token":"X","DisplayClaims":{"xui":[{"uhs":"H","xid":"1"}]}}"#)
        }
        "/mc/login" => json_ok(r#"{"access_token":"MC","expires_in":1}"#),
        "/mc/entitlements" => {
            panic!("the store entitlement list is empty for Game Pass and must not gate sign-in")
        }
        "/mc/profile" => json_ok(r#"{"id":"UUID-GP","name":"GamePassPlayer"}"#),
        _ => json_err(404, "{}"),
    })
    .await;

    let account = flow(&server)
        .refresh(&Secret::new("R".into()))
        .await
        .unwrap();
    assert_eq!(account.profile.name, "GamePassPlayer");
}

#[tokio::test]
async fn declined_sign_in_stops_polling_immediately() {
    let server = TestServer::start(|req: &Request| match req.path.as_str() {
        "/devicecode" => json_ok(
            r#"{"device_code":"D","user_code":"C","verification_uri":"u",
                "expires_in":900,"interval":1,"message":""}"#,
        ),
        "/token" => json_err(400, r#"{"error":"authorization_declined"}"#),
        _ => json_err(404, "{}"),
    })
    .await;

    let flow = flow(&server);
    let prompt = flow.start_device_code().await.unwrap();
    let error = flow
        .complete_device_code(&prompt, |_| true)
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::DeclinedByUser));
}

#[tokio::test]
async fn user_cancelling_the_wait_aborts_the_flow() {
    let server = TestServer::start(mock_chain(usize::MAX)).await;
    let flow = flow(&server);
    let prompt = flow.start_device_code().await.unwrap();
    // Returning false from the wait callback means "user cancelled".
    let error = flow
        .complete_device_code(&prompt, |_| false)
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::DeclinedByUser));
}
