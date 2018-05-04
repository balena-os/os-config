use hex;
use rand::{thread_rng, Rng};

pub fn generate_api_key() -> String {
    let mut buf = [0u8; 16];
    thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}
