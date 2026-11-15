//! Strava tokens are stored encrypted with Fernet, keyed by `TOKEN_ENCRYPTION_KEY`; the same key and format as
//! the Python version, so tokens it stored stay readable.

use fernet::Fernet;
use pi_common::{ApiResult, AppError};

/// Encrypts and decrypts stored tokens. The empty string stands for "no token" both ways.
#[derive(Clone)]
pub struct TokenCipher(Fernet);

impl TokenCipher {
    /// `key` is a Fernet key (URL-safe base64 of 32 bytes), as `Fernet.generate_key()` makes.
    pub fn new(key: &str) -> Result<Self, String> {
        Fernet::new(key)
            .map(Self)
            .ok_or_else(|| "TOKEN_ENCRYPTION_KEY is not a valid Fernet key".to_owned())
    }

    pub fn encrypt(&self, value: &str) -> String {
        if value.is_empty() {
            String::new()
        } else {
            self.0.encrypt(value.as_bytes())
        }
    }

    pub fn decrypt(&self, value: &str) -> ApiResult<String> {
        if value.is_empty() {
            return Ok(String::new());
        }
        let bytes = self
            .0
            .decrypt(value)
            .map_err(|_| AppError::internal("a stored token cannot be decrypted"))?;
        String::from_utf8(bytes).map_err(AppError::internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made by the Python version's `cryptography` with a throw-away key.
    const KEY: &str = "CE7oj2eOJ-Es3tI_KD7y5xlo370XPgjPvEERoJu3mD4=";
    const FROM_PYTHON: &str = "gAAAAABq-b05CaLOcYwc-_J_tn0VRsuS0zXBUEkVyKu6jaTKjXmdtskfN83LXYuPezEN2CW3j08nsi_v4lB5FKyLtG8Anuz9T_xO0XqR1IJkV7Mnb4TpKBs=";

    #[test]
    fn reads_what_python_wrote_and_round_trips() {
        let c = TokenCipher::new(KEY).unwrap();
        assert_eq!(c.decrypt(FROM_PYTHON).unwrap(), "access-token-123");
        assert_eq!(c.decrypt(&c.encrypt("refresh-9")).unwrap(), "refresh-9");
        assert_eq!(
            (c.encrypt(""), c.decrypt("").unwrap()),
            (String::new(), String::new())
        );
    }

    #[test]
    fn a_wrong_key_or_garbage_fails() {
        let other = TokenCipher::new(&Fernet::generate_key()).unwrap();
        assert!(other.decrypt(FROM_PYTHON).is_err());
        assert!(TokenCipher::new("nope").is_err());
    }
}
