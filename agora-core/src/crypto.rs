use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

/// Ed25519 keypair for an Agora Agent.
/// The private key NEVER leaves the Agent's local environment.
pub struct Keypair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl Keypair {
    /// Generate a new Ed25519 keypair using OS randomness.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Sign a message, returning the signature as a base64-encoded string.
    pub fn sign(&self, message: &[u8]) -> String {
        use base64::Engine;
        let signature = self.signing_key.sign(message);
        base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
    }

    /// Verify a signature on a message. Returns true if valid.
    pub fn verify(public_key_bytes: &[u8; 32], message: &[u8], signature_b64: &str) -> bool {
        use base64::Engine;
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_array: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_array);
        let Ok(vk) = VerifyingKey::from_bytes(public_key_bytes) else {
            return false;
        };
        vk.verify_strict(message, &sig).is_ok()
    }

    /// Serialize the public key to bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_sign_and_verify() {
        let kp = Keypair::generate();
        let message = b"agora post content";
        let sig = kp.sign(message);

        assert!(Keypair::verify(&kp.public_key_bytes(), message, &sig));
    }

    #[test]
    fn test_wrong_message_fails() {
        let kp = Keypair::generate();
        let sig = kp.sign(b"original message");

        assert!(!Keypair::verify(
            &kp.public_key_bytes(),
            b"tampered message",
            &sig
        ));
    }
}
