//! TOTP (Time-based One-Time Password) generation for Spotify authentication.
//!
//! Implements RFC 6238 TOTP using HMAC-SHA1.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use thiserror::Error;

type HmacSha1 = Hmac<Sha1>;

/// TOTP generation errors
#[derive(Debug, Error)]
pub enum TotpError {
    /// The provided secret key has an invalid length for HMAC
    #[error("Invalid HMAC key length")]
    InvalidKeyLength,
}

/// Generate a TOTP code using HMAC-SHA1 (RFC 6238).
///
/// # Arguments
///
/// * `secret` - The decoded secret key bytes
/// * `server_time_seconds` - Server time in seconds (from Spotify server-time endpoint)
///
/// # Returns
///
/// A 6-digit TOTP code as a zero-padded string.
///
/// # Errors
///
/// Returns [`TotpError::InvalidKeyLength`] if the secret key is invalid for HMAC-SHA1.
pub fn generate_totp(secret: &[u8], server_time_seconds: u64) -> Result<String, TotpError> {
    const PERIOD: u64 = 30;
    const DIGITS: u32 = 6;

    // Calculate counter: floor(time / period)
    let counter = server_time_seconds / PERIOD;

    // Convert counter to big-endian 8-byte array
    let counter_bytes = counter.to_be_bytes();

    // Compute HMAC-SHA1
    let mut mac = HmacSha1::new_from_slice(secret).map_err(|_| TotpError::InvalidKeyLength)?;
    mac.update(&counter_bytes);
    let result = mac.finalize().into_bytes();

    // Dynamic truncation (RFC 4226)
    // Get offset from last 4 bits of the last byte
    let offset = (result[19] & 0x0F) as usize;

    // Extract 4 bytes starting at offset and mask high bit
    let binary = u32::from_be_bytes([
        result[offset] & 0x7F,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);

    // Generate 6-digit code
    let code = binary % 10u32.pow(DIGITS);

    Ok(format!("{code:06}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_totp_format() {
        // Test with a dummy secret
        let secret = b"test_secret_key!";
        let result = generate_totp(secret, 1_700_000_000);

        assert!(result.is_ok());
        let code = result.expect("TOTP generation should succeed");
        assert_eq!(code.len(), 6, "TOTP should be 6 digits");
        assert!(
            code.chars().all(|c| c.is_ascii_digit()),
            "TOTP should only contain digits"
        );
    }

    #[test]
    fn test_generate_totp_same_period() {
        // Same 30-second period should produce same code
        // 1_700_000_010 / 30 = 56_666_667 (floor)
        // 1_700_000_020 / 30 = 56_666_667 (floor)
        let secret = b"test_secret_key!";
        let code1 = generate_totp(secret, 1_700_000_010).expect("TOTP generation should succeed");
        let code2 = generate_totp(secret, 1_700_000_020).expect("TOTP generation should succeed");

        assert_eq!(code1, code2, "Same period should produce same code");
    }

    #[test]
    fn test_generate_totp_different_period() {
        // Different 30-second periods should produce different codes
        let secret = b"test_secret_key!";
        let code1 = generate_totp(secret, 1_700_000_000).expect("TOTP generation should succeed");
        let code2 = generate_totp(secret, 1_700_000_030).expect("TOTP generation should succeed");

        assert_ne!(code1, code2, "Different periods should produce different codes");
    }
}
