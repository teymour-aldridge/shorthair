use once_cell::sync::Lazy;
use rand::{distributions::Alphanumeric, Rng};
use regex::Regex;

pub fn short_random(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

pub fn is_valid_email(string: &str) -> bool {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(include_str!("email_regex")).unwrap());
    RE.is_match(string)
}
