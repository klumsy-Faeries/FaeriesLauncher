//! Reads the real account store on this machine, the same way a launch does.
//!
//! Ignored by default: it needs a launcher that has signed in, and it talks
//! to the user's credential manager. It prints token *lengths* only.
//!
//! ```text
//! cargo test -p faerie-auth --test live_store -- --ignored --nocapture
//! ```
//! Set `FAERIE_DATA_DIR` to point at a non-default data folder.

use std::path::PathBuf;

use faerie_auth::AccountStore;

fn config_dir() -> PathBuf {
    let root = std::env::var_os("FAERIE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("FaerieLauncher")))
        .expect("a data directory");
    root.join("config")
}

#[test]
#[ignore = "needs a signed-in launcher on this machine"]
fn stored_tokens_read_back_for_the_active_account() {
    let store = AccountStore::new(&config_dir());
    let active = store
        .active()
        .expect("accounts.json readable")
        .expect("an active account — sign in through the launcher first");

    let (minecraft, refresh) = store
        .secrets_for(&active.id)
        .expect("tokens reassemble from the credential manager");

    let minecraft = minecraft.expose();
    let refresh = refresh.expose();
    assert_eq!(
        minecraft.split('.').count(),
        3,
        "a Minecraft access token is a JWT (three dot-separated parts)"
    );
    assert!(!refresh.is_empty(), "refresh token present");
    assert!(
        minecraft.len() + refresh.len() > 1280,
        "these tokens are larger than one Windows credential, so this exercises chunking"
    );
    eprintln!(
        "account {} ({}): minecraft token {} chars, refresh token {} chars, expires {}",
        active.name,
        active.id,
        minecraft.len(),
        refresh.len(),
        if active.is_expired() {
            "EXPIRED"
        } else {
            "valid"
        }
    );
}
