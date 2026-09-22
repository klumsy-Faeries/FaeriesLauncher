use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

/// Streamed SHA-1 of a file (Mojang's manifest hashes are SHA-1). Never
/// loads the file into memory; call from `spawn_blocking` in async code.
pub fn sha1_of_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

pub fn sha1_of_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_known_vector() {
        // Well-known vector: sha1("abc")
        assert_eq!(
            sha1_of_bytes(b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn file_and_bytes_hashes_agree() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.bin");
        let payload = vec![7u8; 200_000];
        std::fs::write(&path, &payload).unwrap();
        assert_eq!(sha1_of_file(&path).unwrap(), sha1_of_bytes(&payload));
    }
}
