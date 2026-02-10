// src/agent/secret_scanner.rs
//
// Secret detection and redaction for tool outputs.
// Scans text for common secret patterns and redacts them.

use once_cell::sync::Lazy;
use regex::RegexSet;

/// A detected secret match with its kind.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SecretMatch {
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
}

/// Secret pattern kinds, indexed to match PATTERN_STRINGS order.
const PATTERN_KINDS: &[&str] = &[
    "AWS Access Key",
    "GitHub PAT",
    "Private Key",
    "Anthropic API Key",
    "OpenAI API Key",
];

const PATTERN_STRINGS: &[&str] = &[
    r"AKIA[0-9A-Z]{16}",
    r"gh[pous]_[A-Za-z0-9_]{36,255}",
    r"-----BEGIN[A-Z ]*PRIVATE KEY-----",
    r"sk-ant-[A-Za-z0-9\-_]{20,}",
    r"sk-[A-Za-z0-9]{20,}",
];

static SECRET_REGEX_SET: Lazy<RegexSet> =
    Lazy::new(|| RegexSet::new(PATTERN_STRINGS).expect("hardcoded secret patterns must compile"));

static SECRET_REGEXES: Lazy<Vec<regex::Regex>> = Lazy::new(|| {
    PATTERN_STRINGS
        .iter()
        .map(|p| regex::Regex::new(p).expect("hardcoded secret pattern must compile"))
        .collect()
});

/// Scan text for secrets and redact them.
/// Returns the redacted text and a list of matches found.
pub fn redact_secrets(text: &str) -> (String, Vec<SecretMatch>) {
    // Quick check: does any pattern match at all?
    let matching_indices: Vec<usize> = SECRET_REGEX_SET.matches(text).into_iter().collect();
    if matching_indices.is_empty() {
        return (text.to_string(), Vec::new());
    }

    let mut result = text.to_string();
    let mut all_matches = Vec::new();

    // For each matching pattern, find and replace all occurrences
    for &idx in &matching_indices {
        let re = &SECRET_REGEXES[idx];
        let kind = PATTERN_KINDS[idx];

        // Collect matches on current result
        let found: Vec<(usize, usize)> = re
            .find_iter(&result)
            .map(|m| (m.start(), m.end()))
            .collect();

        // Replace in reverse order to preserve earlier offsets
        for &(start, end) in found.iter().rev() {
            all_matches.push(SecretMatch { kind, start, end });
            let redacted = format!("[REDACTED:{}]", kind);
            result.replace_range(start..end, &redacted);
        }
    }

    (result, all_matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_aws_key() {
        let input = "key: AKIAIOSFODNN7EXAMPLE";
        let (redacted, matches) = redact_secrets(input);
        assert!(redacted.contains("[REDACTED:AWS Access Key]"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, "AWS Access Key");
    }

    #[test]
    fn redacts_github_pat() {
        let input = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop";
        let (redacted, matches) = redact_secrets(input);
        assert!(redacted.contains("[REDACTED:GitHub PAT]"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn redacts_private_key_header() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIE...";
        let (redacted, matches) = redact_secrets(input);
        assert!(redacted.contains("[REDACTED:Private Key]"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn redacts_anthropic_key() {
        let input = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwx";
        let (redacted, matches) = redact_secrets(input);
        assert!(redacted.contains("[REDACTED:Anthropic API Key]"));
        assert!(!matches.is_empty());
    }

    #[test]
    fn no_false_positives_on_normal_text() {
        let input = "This is a normal text without any secrets.";
        let (redacted, matches) = redact_secrets(input);
        assert_eq!(redacted, input);
        assert!(matches.is_empty());
    }

    #[test]
    fn multiple_secrets_redacted() {
        let input =
            "aws: AKIAIOSFODNN7EXAMPLE github: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop";
        let (_, matches) = redact_secrets(input);
        assert!(matches.len() >= 2);
    }
}
