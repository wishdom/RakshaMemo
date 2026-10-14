//! Password strength estimates from zxcvbn: dictionary words, names, dates,
//! keyboard patterns, and repeats count against a password, not just length.

pub use zxcvbn::feedback::{Suggestion, Warning};

/// What the UI shows. zxcvbn's own result holds password fragments, so only
/// these summary fields are kept and the full result is dropped at once.
/// The advice stays typed so the interface can show it in any language.
#[derive(Clone, Debug, PartialEq)]
pub struct Strength {
    /// 0 (very weak) to 4 (very strong).
    pub score: u8,
    /// Estimated offline cracking time, in seconds, against a slow hash such
    /// as Argon2.
    pub crack_seconds: f64,
    pub warning: Option<Warning>,
    pub suggestion: Option<Suggestion>,
}

/// zxcvbn's cost grows quickly with length, and longer passwords are not
/// scored any better, so only a bounded prefix is analysed.
const MAX_ANALYSED_CHARS: usize = 100;

/// `None` for an empty password. `user_inputs` are words the password should
/// not be based on, such as the account's title or username.
pub fn estimate(password: &str, user_inputs: &[&str]) -> Option<Strength> {
    if password.is_empty() {
        return None;
    }
    let end = password
        .char_indices()
        .nth(MAX_ANALYSED_CHARS)
        .map_or(password.len(), |(index, _)| index);
    let entropy = zxcvbn::zxcvbn(&password[..end], user_inputs);
    let score = u8::from(entropy.score());
    let feedback = entropy.feedback();
    let crack_seconds = match entropy.crack_times().offline_slow_hashing_1e4_per_second() {
        zxcvbn::time_estimates::CrackTimeSeconds::Integer(seconds) => seconds as f64,
        zxcvbn::time_estimates::CrackTimeSeconds::Float(seconds) => seconds,
    };
    Some(Strength {
        score,
        crack_seconds,
        warning: feedback.and_then(|feedback| feedback.warning()),
        suggestion: feedback.and_then(|feedback| feedback.suggestions().first().copied()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_patterns_score_low_and_random_passphrases_score_high() {
        assert_eq!(estimate("", &[]), None);
        let weak = estimate("password123", &[]).unwrap();
        assert!(weak.score <= 1, "{weak:?}");
        assert!(weak.warning.is_some() || weak.suggestion.is_some());
        let strong = estimate("correct-horse-battery-staple-quilt", &[]).unwrap();
        assert!(strong.score >= 3, "{strong:?}");
        assert!(strong.crack_seconds > weak.crack_seconds);
    }

    #[test]
    fn account_details_count_against_the_password() {
        let free = estimate("Wishdom2026", &[]).unwrap();
        let reused = estimate("Wishdom2026", &["wishdom"]).unwrap();
        assert!(reused.score <= free.score);
    }

    #[test]
    fn very_long_input_is_bounded() {
        let long = "x7#Qp".repeat(10_000);
        let started = std::time::Instant::now();
        assert!(estimate(&long, &[]).is_some());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
