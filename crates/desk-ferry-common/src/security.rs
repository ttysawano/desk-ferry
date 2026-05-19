use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::{
    error::{DeskFerryError, Result},
    protocol::{AuthChallenge, AuthResponse, AuthResult, CURRENT_PROTOCOL_VERSION},
};

type HmacSha256 = Hmac<Sha256>;

pub const NONCE_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Psk(pub Vec<u8>);

impl Psk {
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self> {
        let bytes = bytes.as_ref();
        if bytes.is_empty() {
            return Err(DeskFerryError::SecurityPolicy(
                "PSK must not be empty".to_string(),
            ));
        }
        Ok(Self(bytes.to_vec()))
    }
}

pub fn generate_nonce() -> [u8; NONCE_BYTES] {
    let mut nonce = [0_u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

pub fn encode_nonce(nonce: &[u8]) -> String {
    hex::encode(nonce)
}

pub fn decode_nonce(value: &str) -> Result<Vec<u8>> {
    let nonce = hex::decode(value).map_err(|_| {
        DeskFerryError::Authentication("auth challenge nonce is not valid hex".to_string())
    })?;
    if nonce.is_empty() {
        return Err(DeskFerryError::Authentication(
            "auth challenge nonce is empty".to_string(),
        ));
    }
    Ok(nonce)
}

pub fn compute_psk_hmac(psk: &Psk, nonce: &[u8], host_name: &str) -> Result<String> {
    if host_name.trim().is_empty() {
        return Err(DeskFerryError::Authentication(
            "host name must not be empty".to_string(),
        ));
    }

    let mut mac = HmacSha256::new_from_slice(&psk.0)
        .map_err(|_| DeskFerryError::Authentication("invalid PSK".to_string()))?;
    mac.update(nonce);
    mac.update(host_name.as_bytes());
    mac.update(&CURRENT_PROTOCOL_VERSION.to_be_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn verify_psk_hmac(psk: &Psk, nonce: &[u8], host_name: &str, hmac_hex: &str) -> Result<()> {
    let expected = compute_psk_hmac(psk, nonce, host_name)?;
    let expected_bytes = hex::decode(expected)
        .map_err(|_| DeskFerryError::Authentication("invalid expected HMAC".to_string()))?;
    let actual_bytes = hex::decode(hmac_hex)
        .map_err(|_| DeskFerryError::Authentication("invalid HMAC encoding".to_string()))?;

    if expected_bytes.len() != actual_bytes.len() {
        return Err(DeskFerryError::Authentication(
            "HMAC verification failed".to_string(),
        ));
    }

    let mut diff = 0_u8;
    for (left, right) in expected_bytes.iter().zip(actual_bytes.iter()) {
        diff |= left ^ right;
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(DeskFerryError::Authentication(
            "HMAC verification failed".to_string(),
        ))
    }
}

pub fn certificate_fingerprint_sha256(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    hex::encode(digest)
}

pub fn normalize_fingerprint(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub fn verify_fingerprint(cert_der: &[u8], expected: &str) -> Result<()> {
    let actual = certificate_fingerprint_sha256(cert_der);
    let expected = normalize_fingerprint(expected);
    if expected.is_empty() {
        return Err(DeskFerryError::SecurityPolicy(
            "server certificate fingerprint is required".to_string(),
        ));
    }
    if actual == expected {
        Ok(())
    } else {
        Err(DeskFerryError::SecurityPolicy(
            "server certificate fingerprint mismatch".to_string(),
        ))
    }
}

pub fn new_auth_challenge() -> AuthChallenge {
    let nonce = generate_nonce();
    AuthChallenge {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        nonce: encode_nonce(&nonce),
    }
}

pub fn auth_response(
    psk: &Psk,
    challenge: &AuthChallenge,
    host_name: &str,
) -> Result<AuthResponse> {
    let nonce = decode_nonce(&challenge.nonce)?;
    Ok(AuthResponse {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        host_name: host_name.to_string(),
        hmac: compute_psk_hmac(psk, &nonce, host_name)?,
    })
}

pub fn verify_auth_response(
    psk: &Psk,
    challenge: &AuthChallenge,
    response: &AuthResponse,
) -> Result<()> {
    let nonce = decode_nonce(&challenge.nonce)?;
    verify_psk_hmac(psk, &nonce, &response.host_name, &response.hmac)
}

pub fn auth_result_success() -> AuthResult {
    AuthResult {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        success: true,
        reason: "authenticated".to_string(),
    }
}

pub fn auth_result_failure() -> AuthResult {
    AuthResult {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        success: false,
        reason: "authentication_failed".to_string(),
    }
}
