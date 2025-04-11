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
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?m)^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$",
        )
        .unwrap()
    });
    RE.is_match(string)
}

#[cfg(test)]
mod test_email_regex {
    use crate::util::is_valid_email;

    #[test]
    fn test_simple_test_email() {
        assert!(is_valid_email("judge1@example.com"))
    }
}
