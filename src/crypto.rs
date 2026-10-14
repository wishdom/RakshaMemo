//! Python v1 compatibility: Argon2id v19, AES-256-GCM, no associated data.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result, anyhow, bail};
use argon2::{Algorithm, Argon2, Block, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::{TryRng, rngs::SysRng};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::model::VaultData;

pub const MAX_VAULT_BYTES: usize = 32 * 1024 * 1024;

/// The password does not open the vault (or the vault was altered). A distinct
/// type, so the interface can recognize and translate this common case.
#[derive(Debug)]
pub struct IncorrectPassword;

impl std::fmt::Display for IncorrectPassword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Incorrect password or damaged vault")
    }
}

impl std::error::Error for IncorrectPassword {}
const MAX_PASSWORD_BYTES: usize = 16 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct KdfParams {
    algo: String,
    time: u32,
    mem: u32,
    parallel: u32,
    length: usize,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            algo: "argon2id".into(),
            time: 3,
            mem: 65_536,
            parallel: 2,
            length: 32,
        }
    }
}

impl KdfParams {
    fn validate(&self) -> Result<Params> {
        if self.algo != "argon2id" || self.length != 32 {
            bail!("Unsupported vault key derivation algorithm or key length");
        }
        // Validate before allocation. Work and memory are capped independently.
        if !(1..=10).contains(&self.time)
            || !(1..=8).contains(&self.parallel)
            || self.mem < 8 * self.parallel
            || self.mem > 262_144
            || u64::from(self.mem) * u64::from(self.time) > 1_048_576
        {
            bail!("Vault key derivation parameters exceed supported safety limits");
        }
        Params::new(self.mem, self.time, self.parallel, Some(self.length))
            .map_err(|e| anyhow!("Invalid vault key derivation parameters: {e}"))
    }
}

#[derive(Serialize, Deserialize)]
struct CipherEnvelope {
    alg: String,
    nonce: String,
    blob: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u32,
    #[serde(default)]
    kdf: KdfParams,
    salt: String,
    cipher: CipherEnvelope,
}

/// Retains a derived key, never the master password. The key is wiped on drop.
#[derive(Clone)]
pub(crate) struct VaultCipher {
    key: Zeroizing<[u8; 32]>,
    salt: Vec<u8>,
    params: KdfParams,
}

impl VaultCipher {
    pub(crate) fn new(password: &str) -> Result<Self> {
        if password.is_empty() {
            bail!("A master password is required");
        }
        let mut salt = vec![0u8; 16];
        SysRng
            .try_fill_bytes(&mut salt)
            .context("Operating system random generator failed")?;
        let params = KdfParams::default();
        let key = derive_key(password, &salt, &params)?;
        Ok(Self { key, salt, params })
    }

    pub(crate) fn open(password: &str, bytes: &[u8]) -> Result<(Self, VaultData)> {
        if bytes.len() > MAX_VAULT_BYTES {
            bail!("Vault exceeds the 32 MiB file size limit");
        }
        let envelope: Envelope =
            serde_json::from_slice(bytes).context("Invalid encrypted vault file")?;
        if envelope.v != 1 {
            bail!("Unsupported vault version: {}", envelope.v);
        }
        if envelope.cipher.alg != "aes-256-gcm" {
            bail!("Unsupported vault encryption algorithm");
        }
        envelope.kdf.validate()?;
        if !(12..=88).contains(&envelope.salt.len()) {
            bail!("Invalid vault salt length");
        }
        let salt = STANDARD
            .decode(&envelope.salt)
            .context("Invalid vault salt encoding")?;
        if !(8..=64).contains(&salt.len()) {
            bail!("Invalid vault salt length");
        }
        if envelope.cipher.nonce.len() != 16 {
            bail!("Invalid vault nonce length");
        }
        let nonce = STANDARD
            .decode(envelope.cipher.nonce)
            .context("Invalid vault nonce encoding")?;
        if nonce.len() != 12 {
            bail!("Invalid vault nonce length");
        }
        let ciphertext = STANDARD
            .decode(envelope.cipher.blob)
            .context("Invalid vault ciphertext encoding")?;
        if ciphertext.len() < 16 {
            bail!("Invalid vault authentication tag");
        }
        let key = derive_key(password, &salt, &envelope.kdf)?;
        let cipher = Aes256Gcm::new_from_slice(key.as_ref())
            .map_err(|_| anyhow!("Invalid encryption key"))?;
        // The length was checked above, so the conversion cannot fail.
        let nonce =
            Nonce::try_from(nonce.as_slice()).map_err(|_| anyhow!("Invalid vault nonce length"))?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(&nonce, ciphertext.as_ref())
                .map_err(|_| IncorrectPassword)?,
        );
        let mut data: VaultData =
            serde_json::from_slice(&plaintext).context("Unsupported vault contents")?;
        data.normalize();
        Ok((
            Self {
                key,
                salt,
                params: envelope.kdf,
            },
            data,
        ))
    }

    /// Whether `password` derives this vault's key. Re-runs Argon2 with the
    /// stored salt and parameters; neither key is kept beyond the comparison.
    pub(crate) fn matches(&self, password: &str) -> Result<bool> {
        let key = derive_key(password, &self.salt, &self.params)?;
        // Compare without an early exit, so timing reveals nothing.
        let difference = key
            .iter()
            .zip(self.key.iter())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b));
        Ok(difference == 0)
    }

    pub(crate) fn encrypt(&self, data: &VaultData) -> Result<Vec<u8>> {
        let plaintext =
            Zeroizing::new(serde_json::to_vec(data).context("Could not serialize vault")?);
        if plaintext.len() > MAX_VAULT_BYTES * 3 / 4 - 1024 {
            bail!("Vault exceeds the 32 MiB file size limit");
        }
        let mut nonce = [0u8; 12];
        SysRng
            .try_fill_bytes(&mut nonce)
            .context("Operating system random generator failed")?;
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref())
            .map_err(|_| anyhow!("Invalid encryption key"))?;
        let ciphertext = cipher
            .encrypt(&Nonce::from(nonce), plaintext.as_slice())
            .map_err(|_| anyhow!("Vault encryption failed"))?;
        let envelope = Envelope {
            v: 1,
            kdf: self.params.clone(),
            salt: STANDARD.encode(&self.salt),
            cipher: CipherEnvelope {
                alg: "aes-256-gcm".into(),
                nonce: STANDARD.encode(nonce),
                blob: STANDARD.encode(ciphertext),
            },
        };
        let bytes = serde_json::to_vec(&envelope)?;
        if bytes.len() > MAX_VAULT_BYTES {
            bail!("Vault exceeds the 32 MiB file size limit");
        }
        Ok(bytes)
    }
}

fn derive_key(password: &str, salt: &[u8], params: &KdfParams) -> Result<Zeroizing<[u8; 32]>> {
    if password.len() > MAX_PASSWORD_BYTES {
        bail!("Master password exceeds the 16 KiB limit");
    }
    let parameters = params.validate()?;
    // argon2 0.5's allocation helper does not wipe its memory matrix. Supply
    // our own zeroizing matrix, including the error path.
    let mut memory = Zeroizing::new(vec![Block::default(); parameters.block_count()]);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into_with_memory(
            password.as_bytes(),
            salt,
            key.as_mut(),
            memory.as_mut_slice(),
        )
        .map_err(|e| anyhow!("Could not derive vault key: {e}"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_nonces_and_authenticated_roundtrip() {
        let cipher = VaultCipher::new("correct horse battery staple").unwrap();
        let mut data = VaultData::new();
        data.notes.items.push(crate::model::Note {
            content: "Private 日本語 🔐".into(),
            ..Default::default()
        });
        let first = cipher.encrypt(&data).unwrap();
        let second = cipher.encrypt(&data).unwrap();
        let a: Envelope = serde_json::from_slice(&first).unwrap();
        let b: Envelope = serde_json::from_slice(&second).unwrap();
        assert_ne!(a.cipher.nonce, b.cipher.nonce);
        assert_ne!(a.cipher.blob, b.cipher.blob);
        let (_, decoded) = VaultCipher::open("correct horse battery staple", &first).unwrap();
        assert_eq!(decoded.notes.items[0].content, "Private 日本語 🔐");
        assert!(VaultCipher::open("wrong password", &first).is_err());
        let mut changed: serde_json::Value = serde_json::from_slice(&first).unwrap();
        let mut blob = STANDARD
            .decode(changed["cipher"]["blob"].as_str().unwrap())
            .unwrap();
        blob[0] ^= 1;
        changed["cipher"]["blob"] = STANDARD.encode(blob).into();
        assert!(
            VaultCipher::open(
                "correct horse battery staple",
                &serde_json::to_vec(&changed).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn matches_only_the_password_that_derived_the_key() {
        let cipher = VaultCipher::new("correct horse battery staple").unwrap();
        assert!(cipher.matches("correct horse battery staple").unwrap());
        assert!(!cipher.matches("correct horse battery stapler").unwrap());
        assert!(!cipher.matches("").unwrap());
    }

    #[test]
    fn rejects_dangerous_or_unsupported_envelopes_before_kdf() {
        let original = serde_json::json!({"v":1,"kdf":{"algo":"argon2id","time":3,"mem":65536,"parallel":2,"length":32},"salt":"AAAAAAAAAAAAAAAAAAAAAA==","cipher":{"alg":"aes-256-gcm","nonce":"AAAAAAAAAAAAAAAA","blob":"AAAAAAAAAAAAAAAAAAAAAA=="}});
        for (field, value) in [
            ("mem", serde_json::json!(4_000_000_000u32)),
            ("time", serde_json::json!(100)),
            ("parallel", serde_json::json!(100)),
            ("length", serde_json::json!(16)),
            ("algo", serde_json::json!("argon2d")),
        ] {
            let mut changed = original.clone();
            changed["kdf"][field] = value;
            assert!(VaultCipher::open("password", &serde_json::to_vec(&changed).unwrap()).is_err());
        }
        let mut wrong_version = original.clone();
        wrong_version["v"] = 2.into();
        assert!(
            VaultCipher::open("password", &serde_json::to_vec(&wrong_version).unwrap()).is_err()
        );
        let mut wrong_nonce = original;
        wrong_nonce["cipher"]["nonce"] = "AA==".into();
        assert!(VaultCipher::open("password", &serde_json::to_vec(&wrong_nonce).unwrap()).is_err());
    }

    #[test]
    fn opens_real_python_vaults() {
        // These fixtures were generated by the original Python encrypt_json
        // using argon2-cffi + cryptography and the original default parameters.
        let (_, data) = VaultCipher::open(
            "Compatibility test 🔐",
            include_bytes!("../tests/fixtures/python-v1.json"),
        )
        .unwrap();
        assert_eq!(
            data.notes.items[0].content,
            "Rust ↔ Python\n秘密のノート 🔐"
        );
        assert_eq!(data.passwords[0].password, "pāsswörd-日本語");
        assert_eq!(data.unknown_fields["future_metadata"]["keep"], true);
        let (_, legacy) = VaultCipher::open(
            "Compatibility test 🔐",
            include_bytes!("../tests/fixtures/python-legacy.json"),
        )
        .unwrap();
        assert_eq!(legacy.notes.items[0].title, "Imported Note");
        assert_eq!(
            legacy.notes.items[0].content,
            "Legacy note\n移行してください"
        );
    }
}
