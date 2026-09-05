//! Symmetric sealing for remote-source credentials (M3U embedded logins, OAuth
//! tokens). Ciphertext is safe to persist in the database; the key lives only on
//! the server filesystem and is never logged or serialized.

use std::fs;

use base64::Engine;
use chacha20poly1305::{
    AeadCore, ChaCha20Poly1305, KeyInit,
    aead::{Aead, OsRng},
};

use crate::{
    config::Config,
    errors::{AppError, AppResult},
};

fn key(config: &Config) -> AppResult<[u8; 32]> {
    let path = config.data_dir.join("secrets/remote-sources.key");
    if path.exists() {
        return fs::read(&path)?
            .try_into()
            .map_err(|_| AppError::config("invalid remote-source encryption key"));
    }
    use rand::RngCore;
    let mut material = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut material);
    fs::write(&path, material)?;
    Ok(material)
}

/// Encrypts `plaintext` and returns a base64 payload of `nonce || ciphertext`.
pub fn seal(config: &Config, plaintext: &str) -> AppResult<String> {
    let cipher = ChaCha20Poly1305::new((&key(config)?).into());
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| AppError::config("unable to encrypt remote-source secret"))?;
    let mut payload = nonce.to_vec();
    payload.extend(ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(payload))
}

/// Reverses [`seal`]. Returns a configuration error if the payload is malformed
/// or was produced with a different key.
pub fn open(config: &Config, encoded: &str) -> AppResult<String> {
    let payload = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| AppError::config("invalid encrypted remote-source secret"))?;
    if payload.len() < 13 {
        return Err(AppError::config("invalid encrypted remote-source secret"));
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = ChaCha20Poly1305::new((&key(config)?).into());
    let plaintext = cipher
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| AppError::config("unable to decrypt remote-source secret"))?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::config("invalid remote-source secret encoding"))
}
