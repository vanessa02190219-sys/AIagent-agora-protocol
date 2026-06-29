use crate::error::AgoraError;

/// An Agora Decentralized Identifier.
///
/// Format: `did:agora:<multibase-encoded-ed25519-public-key>`
///
/// Example: `did:agora:z6MkhaXgBxSA3nLrRZ5XkHfP7JUq4WM2tGkXo1sV5LPqW9cR`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did {
    /// Full DID string, e.g. "did:agora:z6Mk..."
    pub full: String,
    /// Multibase-encoded public key portion
    pub identifier: String,
}

impl Did {
    /// Create a Did from an Ed25519 public key (32 bytes).
    pub fn from_public_key(public_key: &[u8; 32]) -> Result<Self, AgoraError> {
        if public_key.len() != 32 {
            return Err(AgoraError::InvalidKey("public key must be 32 bytes".into()));
        }

        // Multibase base58btc encoding with multicodec prefix for ed25519-pub (0xed)
        let mut prefixed = vec![0xed, 0x01]; // ed25519-pub multicodec
        prefixed.extend_from_slice(public_key);

        // Base58btc encoding (multibase prefix 'z')
        let encoded = bs58::encode(&prefixed).into_string();
        let identifier = format!("z{}", encoded);
        let full = format!("did:agora:{}", identifier);

        Ok(Self { full, identifier })
    }

    /// Parse a DID string into its components.
    pub fn parse(did: &str) -> Result<Self, AgoraError> {
        let parts: Vec<&str> = did.split(':').collect();
        if parts.len() != 3 || parts[0] != "did" || parts[1] != "agora" {
            return Err(AgoraError::InvalidDid(format!(
                "expected 'did:agora:<identifier>', got '{}'",
                did
            )));
        }

        let identifier = parts[2].to_string();
        let full = did.to_string();

        Ok(Self { full, identifier })
    }

    /// Extract the raw Ed25519 public key bytes from this DID.
    pub fn to_public_key_bytes(&self) -> Result<[u8; 32], AgoraError> {
        // Remove multibase 'z' prefix
        if !self.identifier.starts_with('z') {
            return Err(AgoraError::InvalidDid(
                "identifier must start with multibase 'z' prefix".into(),
            ));
        }

        let encoded = &self.identifier[1..]; // strip 'z'
        let decoded = bs58::decode(encoded)
            .into_vec()
            .map_err(|e| AgoraError::InvalidDid(format!("base58 decode failed: {}", e)))?;

        // Check multicodec prefix: 0xed 0x01 = ed25519-pub
        if decoded.len() < 34 || decoded[0] != 0xed || decoded[1] != 0x01 {
            return Err(AgoraError::InvalidDid(
                "missing or invalid ed25519-pub multicodec prefix".into(),
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded[2..34]);
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn test_did_roundtrip() {
        let mut rng = rand::rngs::OsRng;
        let mut pk = [0u8; 32];
        rng.fill_bytes(&mut pk);

        let did = Did::from_public_key(&pk).unwrap();
        let extracted = did.to_public_key_bytes().unwrap();

        assert_eq!(pk, extracted);
    }

    #[test]
    fn test_did_parse() {
        let did_str = "did:agora:z6MkhaXgBxSA3nLrRZ5XkHfP7JUq4WM2tGkXo1sV5LPqW9cR";
        let did = Did::parse(did_str).unwrap();
        assert_eq!(did.full, did_str);
        assert!(did.identifier.starts_with('z'));
    }

    #[test]
    fn test_invalid_did() {
        assert!(Did::parse("did:web:example.com").is_err());
        assert!(Did::parse("not-a-did").is_err());
        assert!(Did::parse("did:agora").is_err());
    }
}
