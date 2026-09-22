//! Microsoft account authentication for Minecraft (§18).
//!
//! Flow: Microsoft device code → Xbox Live → XSTS → Minecraft services →
//! profile. Tokens never touch disk in plaintext and never reach logs: they
//! live in [`faerie_core::Secret`] wrappers and are stored through the OS
//! credential manager ([`store`]).
//!
//! Every endpoint is injectable ([`Endpoints`]) so the whole chain is tested
//! hermetically against a local server.
//!
//! We never ask for, see, or store a Microsoft password: the user signs in on
//! Microsoft's own page and we only ever hold the resulting tokens.

pub mod device_code;
pub mod endpoints;
pub mod flow;
pub mod store;

pub use endpoints::Endpoints;
pub use flow::{Account, AuthFlow, MinecraftProfile};
pub use store::AccountStore;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(
        "no Microsoft client ID is configured. Register an Azure application and have \
             Mojang approve it for the Minecraft API, then set it in Settings → Accounts."
    )]
    NoClientId,

    #[error("network error talking to {stage}: {reason}")]
    Network { stage: &'static str, reason: String },

    #[error("{stage} rejected the request ({status}): {reason}")]
    Rejected {
        stage: &'static str,
        status: u16,
        reason: String,
    },

    #[error("the sign-in was not completed in time")]
    DeviceCodeExpired,

    #[error("the sign-in was declined")]
    DeclinedByUser,

    #[error(
        "this Microsoft account has no Xbox Live profile. Sign in at xbox.com once, then retry."
    )]
    NoXboxAccount,

    #[error("this account is a child account and must be added to a Microsoft family group first")]
    ChildAccount,

    #[error("this account does not own Minecraft: Java Edition")]
    NoMinecraftEntitlement,

    #[error("could not read a response from {stage}: {reason}")]
    BadResponse { stage: &'static str, reason: String },

    #[error("secure credential storage failed: {0}")]
    Keyring(String),

    #[error("account data at {path} could not be read: {reason}")]
    StoreUnreadable {
        path: std::path::PathBuf,
        reason: String,
    },
}
