use std::time::Duration;

/// Build the launcher's shared HTTP client. One client for the whole
/// process: connection pooling, rustls, identifiable user agent.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("FaeriesLauncher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()
        .expect("HTTP client construction only fails on invalid TLS config")
}
