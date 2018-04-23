use hex;
use openssl::hash::{hash, MessageDigest};

use errors::*;

pub fn generate_api_key(master_key: &str, api_endpoint: &str) -> Result<String> {
    generate_api_key_impl(master_key, api_endpoint).chain_err(|| ErrorKind::GenerateDeviceApiKey)
}

pub fn generate_api_key_impl(master_key: &str, api_endpoint: &str) -> Result<String> {
    let data = format!("{}{}", master_key, api_endpoint);
    let hashed = hash(MessageDigest::md5(), data.as_bytes())?;
    Ok(hex::encode(hashed))
}
