//! What every new instance starts with beyond the game itself.

use faerie_instances::servers::{self, ServerEntry};
use faerie_instances::Instance;

/// Servers on the multiplayer list of every instance the launcher creates.
/// Applied when an instance is created and whenever the Optimized preset is
/// installed; an entry the player already has for the address is left as
/// they set it.
pub const SERVERS: &[ServerEntry] = &[ServerEntry {
    name: "Faeries SMP",
    address: "faeriessmp.com",
    // The server sends its own resource pack; answering the prompt on every
    // join is the thing players complain about.
    accept_packs: true,
}];

/// Put the default servers on an instance's list. Never fails the caller:
/// a server-list problem is logged, not turned into a failed instance.
pub fn apply_servers(instance: &Instance) {
    for server in SERVERS {
        match servers::ensure_server(&instance.dir, server) {
            Ok(true) => tracing::info!(
                "instance {}: added {} ({}) to the server list",
                instance.id,
                server.name,
                server.address
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!(
                "instance {}: could not add {} to the server list: {e}",
                instance.id,
                server.name
            ),
        }
    }
}
