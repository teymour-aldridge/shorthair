use rand::{distributions::Alphanumeric, Rng};
use uuid::Uuid;
use uuid_b64::UuidB64;

pub fn short_random(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// Encodes the provided Uuid string (i.e. a [uuid::Uuid]) as a base64 encoded
/// string. This is necessary to use a Uuid as a Tabbycat URL string.
pub fn base64_str_of_uuid_str(public_id: &str) -> String {
    UuidB64::from(Uuid::parse_str(&public_id).unwrap()).to_string()
}
