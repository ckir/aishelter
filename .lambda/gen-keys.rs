use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

fn main() {
    for i in 1..=2 {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        let pub_hex = hex::encode(verifying_key.to_bytes());
        let id = format!("agent-test-{}", i);
        println!("{}|{}", id, pub_hex);
    }
}
