use crate::crypto::{self, SymmetricKey};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub profile_name: String,
    pub username: String,
    pub password: String,
    pub updated_at: i64,
}

pub fn init_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", path))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS credentials (
            profile_name TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            encrypted_password BLOB NOT NULL,
            nonce BLOB NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        [],
    )
    .context("Failed to initialize credentials table")?;

    Ok(conn)
}

pub fn save_credential(
    conn: &Connection,
    profile_name: &str,
    username: &str,
    password: &str,
    key: &SymmetricKey,
) -> Result<()> {
    let (encrypted_password, nonce) = crypto::encrypt(password, key)?;
    let now = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO credentials (profile_name, username, encrypted_password, nonce, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(profile_name) DO UPDATE SET
            username = excluded.username,
            encrypted_password = excluded.encrypted_password,
            nonce = excluded.nonce,
            updated_at = excluded.updated_at;",
        params![profile_name, username, encrypted_password, nonce, now],
    )
    .with_context(|| format!("Failed to save credential for profile: {}", profile_name))?;

    Ok(())
}

pub fn get_credential(
    conn: &Connection,
    profile_name: &str,
    key: &SymmetricKey,
) -> Result<Option<Credential>> {
    let mut stmt = conn.prepare(
        "SELECT username, encrypted_password, nonce, updated_at
         FROM credentials
         WHERE profile_name = ?1;",
    )?;

    let row = stmt
        .query_row(params![profile_name], |row| {
            let username: String = row.get(0)?;
            let enc_pass: Vec<u8> = row.get(1)?;
            let nonce: Vec<u8> = row.get(2)?;
            let updated_at: i64 = row.get(3)?;
            Ok((username, enc_pass, nonce, updated_at))
        })
        .optional()?;

    match row {
        Some((username, enc_pass, nonce, updated_at)) => {
            let password = crypto::decrypt(&enc_pass, &nonce, key).with_context(|| {
                format!("Failed to decrypt password for profile: {}", profile_name)
            })?;

            Ok(Some(Credential {
                profile_name: profile_name.to_string(),
                username,
                password,
                updated_at,
            }))
        }
        None => Ok(None),
    }
}

pub fn has_credential(conn: &Connection, profile_name: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM credentials WHERE profile_name = ?1 LIMIT 1;")?;
    let exists = stmt.exists(params![profile_name])?;
    Ok(exists)
}

pub fn delete_credential(conn: &Connection, profile_name: &str) -> Result<bool> {
    let rows_affected = conn.execute(
        "DELETE FROM credentials WHERE profile_name = ?1;",
        params![profile_name],
    )?;
    Ok(rows_affected > 0)
}

pub fn list_saved_profiles(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT profile_name FROM credentials ORDER BY profile_name ASC;")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut profiles = Vec::new();
    for row in rows {
        profiles.push(row?);
    }
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use rand::rngs::OsRng;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE credentials (
                profile_name TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                encrypted_password BLOB NOT NULL,
                nonce BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_credential_save_and_retrieve() {
        let conn = setup_test_db();
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);

        assert!(!has_credential(&conn, "office.ovpn").unwrap());

        save_credential(&conn, "office.ovpn", "vpnuser", "secret_pass_123", &key).unwrap();

        assert!(has_credential(&conn, "office.ovpn").unwrap());

        let cred = get_credential(&conn, "office.ovpn", &key)
            .unwrap()
            .expect("should find credential");
        assert_eq!(cred.profile_name, "office.ovpn");
        assert_eq!(cred.username, "vpnuser");
        assert_eq!(cred.password, "secret_pass_123");

        // Test update
        save_credential(&conn, "office.ovpn", "vpnuser2", "new_pass_456", &key).unwrap();
        let cred2 = get_credential(&conn, "office.ovpn", &key).unwrap().unwrap();
        assert_eq!(cred2.username, "vpnuser2");
        assert_eq!(cred2.password, "new_pass_456");

        // Test delete
        let deleted = delete_credential(&conn, "office.ovpn").unwrap();
        assert!(deleted);
        assert!(!has_credential(&conn, "office.ovpn").unwrap());
    }
}
