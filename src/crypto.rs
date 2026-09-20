use aes_gcm::aead::Aead;
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, Result, bail};
use rand::RngCore;
use rand::rngs::OsRng;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub const KEY_SIZE: usize = 32;
pub const NONCE_SIZE: usize = 12;

pub type SymmetricKey = [u8; KEY_SIZE];

/// Load symmetric key from path, or generate a new 32-byte key with 0600 permissions if not found.
pub fn load_or_generate_key(path: &Path) -> Result<SymmetricKey> {
    if path.exists() {
        let mut file = OpenOptions::new()
            .read(true)
            .open(path)
            .with_context(|| format!("Failed to open keyfile: {:?}", path))?;

        let mut key = [0u8; KEY_SIZE];
        let bytes_read = file.read(&mut key)?;
        if bytes_read != KEY_SIZE {
            bail!(
                "Keyfile {:?} is corrupted (expected {} bytes, got {})",
                path,
                KEY_SIZE,
                bytes_read
            );
        }

        // Ensure permissions are strictly 0600
        let metadata = fs::metadata(path)?;
        let mut perms = metadata.permissions();
        if perms.mode() & 0o777 != 0o600 {
            perms.set_mode(0o600);
            let _ = fs::set_permissions(path, perms);
        }

        Ok(key)
    } else {
        let mut key = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("Failed to create keyfile: {:?}", path))?;

        file.write_all(&key)?;
        file.flush()?;

        Ok(key)
    }
}

/// Encrypts plaintext using AES-256-GCM. Returns (ciphertext, nonce).
pub fn encrypt(plaintext: &str, key: &SymmetricKey) -> Result<(Vec<u8>, Vec<u8>)> {
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failure: {:?}", e))?;

    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decrypts ciphertext using AES-256-GCM.
pub fn decrypt(ciphertext: &[u8], nonce_bytes: &[u8], key: &SymmetricKey) -> Result<String> {
    if nonce_bytes.len() != NONCE_SIZE {
        bail!(
            "Invalid nonce length: expected {}, got {}",
            NONCE_SIZE,
            nonce_bytes.len()
        );
    }

    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let decrypted_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failure: {:?}", e))?;

    let plaintext =
        String::from_utf8(decrypted_bytes).context("Decrypted data is not valid UTF-8 string")?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut key = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key);

        let secret = "SuperSecretP@ssw0rd!123";
        let (ciphertext, nonce) = encrypt(secret, &key).expect("encryption should succeed");

        assert_ne!(ciphertext, secret.as_bytes());
        assert_eq!(nonce.len(), NONCE_SIZE);

        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("decryption should succeed");
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let mut key1 = [0u8; KEY_SIZE];
        let mut key2 = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key1);
        OsRng.fill_bytes(&mut key2);

        let secret = "MyPassword";
        let (ciphertext, nonce) = encrypt(secret, &key1).unwrap();

        let result = decrypt(&ciphertext, &nonce, &key2);
        assert!(result.is_err());
    }

    #[test]
    fn test_keyfile_creation_and_reload() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join(".key");

        let key1 = load_or_generate_key(&key_path).unwrap();
        assert!(key_path.exists());

        let key2 = load_or_generate_key(&key_path).unwrap();
        assert_eq!(key1, key2);

        let meta = fs::metadata(&key_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }
}
