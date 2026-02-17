// Tool input filtering infrastructure.
//
// Any tool can declare a "filterable field" and reuse the same deny/allow
// pattern infrastructure. Configured per-tool in config.toml under
// [tools.filters.<tool_name>].

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// A reusable filter config that can be applied to any tool's primary input.
/// Configured per-tool in config.toml under [tools.filters.<tool_name>].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolFilter {
    /// Regex patterns that block execution (checked first, highest priority)
    #[serde(default)]
    pub deny_patterns: Vec<String>,

    /// If non-empty, input must match at least one pattern to proceed
    #[serde(default)]
    pub allow_patterns: Vec<String>,

    /// Case-insensitive substring matches that block execution
    #[serde(default)]
    pub deny_substrings: Vec<String>,
}

/// Compiled version of ToolFilter with pre-built regexes.
/// Created once at startup, used on every tool call.
#[derive(Debug, Clone)]
pub struct CompiledToolFilter {
    pub deny_patterns: Vec<(String, Regex)>,
    pub allow_patterns: Vec<(String, Regex)>,
    pub deny_substrings: Vec<String>,
}

impl CompiledToolFilter {
    /// Compile a ToolFilter config into regexes. Fails fast on invalid patterns.
    pub fn compile(filter: &ToolFilter) -> Result<Self> {
        let deny_patterns = filter
            .deny_patterns
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map(|re| (p.clone(), re))
                    .map_err(|e| anyhow::anyhow!("Bad deny pattern '{}': {}", p, e))
            })
            .collect::<Result<Vec<_>>>()?;

        let allow_patterns = filter
            .allow_patterns
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map(|re| (p.clone(), re))
                    .map_err(|e| anyhow::anyhow!("Bad allow pattern '{}': {}", p, e))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            deny_patterns,
            allow_patterns,
            deny_substrings: filter.deny_substrings.clone(),
        })
    }

    /// Compile an empty filter (permits everything). Used when no config is set.
    pub fn permissive() -> Self {
        Self {
            deny_patterns: Vec::new(),
            allow_patterns: Vec::new(),
            deny_substrings: Vec::new(),
        }
    }

    /// Check whether a given input value is permitted.
    /// `tool_name` and `field_name` are used for log messages only.
    ///
    /// Evaluation order:
    ///   1. deny_substrings (case-insensitive)
    ///   2. deny_patterns (regex)
    ///   3. allow_patterns (regex, only if non-empty)
    ///
    /// Returns Ok(()) if allowed, Err with reason if blocked.
    pub fn check(&self, value: &str, tool_name: &str, field_name: &str) -> Result<()> {
        let value_lower = value.to_lowercase();

        // 1. Deny substrings
        for substring in &self.deny_substrings {
            if value_lower.contains(&substring.to_lowercase()) {
                warn!(
                    "Tool '{}' blocked: {} contains denied substring '{}'",
                    tool_name, field_name, substring
                );
                return Err(anyhow::anyhow!(
                    "Blocked: {} contains denied substring '{}'",
                    field_name,
                    substring
                ));
            }
        }

        // 2. Deny patterns
        for (pattern_str, re) in &self.deny_patterns {
            if re.is_match(value) {
                warn!(
                    "Tool '{}' blocked: {} matches denied pattern '{}'",
                    tool_name, field_name, pattern_str
                );
                return Err(anyhow::anyhow!(
                    "Blocked: {} matches denied pattern '{}'",
                    field_name,
                    pattern_str
                ));
            }
        }

        // 3. Allow patterns (if non-empty, value must match at least one)
        if !self.allow_patterns.is_empty() {
            let allowed = self.allow_patterns.iter().any(|(_, re)| re.is_match(value));
            if !allowed {
                warn!(
                    "Tool '{}' blocked: {} does not match any allowed pattern",
                    tool_name, field_name
                );
                return Err(anyhow::anyhow!(
                    "Blocked: {} does not match any allowed pattern",
                    field_name
                ));
            }
        }

        Ok(())
    }

    /// Merge hardcoded deny defaults into this filter.
    /// Deduplicates entries: hardcoded values that already exist are skipped.
    pub fn merge_hardcoded(
        mut self,
        deny_substrings: &[&str],
        deny_patterns: &[&str],
    ) -> Result<Self> {
        let existing_subs: std::collections::HashSet<String> = self
            .deny_substrings
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        for sub in deny_substrings {
            if !existing_subs.contains(&sub.to_lowercase()) {
                self.deny_substrings.push(sub.to_string());
            }
        }

        let existing_patterns: std::collections::HashSet<String> =
            self.deny_patterns.iter().map(|(s, _)| s.clone()).collect();
        for pat in deny_patterns {
            if !existing_patterns.contains(*pat) {
                let re = Regex::new(pat)
                    .map_err(|e| anyhow::anyhow!("Bad hardcoded deny pattern '{}': {}", pat, e))?;
                self.deny_patterns.push((pat.to_string(), re));
            }
        }

        Ok(self)
    }

    /// Returns true if this filter has no rules (permits everything)
    pub fn is_empty(&self) -> bool {
        self.deny_patterns.is_empty()
            && self.allow_patterns.is_empty()
            && self.deny_substrings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter(deny: &[&str], allow: &[&str], deny_sub: &[&str]) -> CompiledToolFilter {
        let filter = ToolFilter {
            deny_patterns: deny.iter().map(|s| s.to_string()).collect(),
            allow_patterns: allow.iter().map(|s| s.to_string()).collect(),
            deny_substrings: deny_sub.iter().map(|s| s.to_string()).collect(),
        };
        CompiledToolFilter::compile(&filter).unwrap()
    }

    #[test]
    fn permissive_allows_everything() {
        let f = CompiledToolFilter::permissive();
        assert!(f.check("rm -rf /", "bash", "command").is_ok());
    }

    #[test]
    fn deny_substring_blocks() {
        let f = make_filter(&[], &[], &["sudo"]);
        assert!(f.check("sudo rm -rf /", "bash", "command").is_err());
        assert!(f.check("ls -la", "bash", "command").is_ok());
    }

    #[test]
    fn deny_substring_case_insensitive() {
        let f = make_filter(&[], &[], &["SUDO"]);
        assert!(f.check("sudo apt install", "bash", "command").is_err());
    }

    #[test]
    fn deny_pattern_blocks() {
        let f = make_filter(&[r"^sudo\b"], &[], &[]);
        assert!(f.check("sudo rm -rf /", "bash", "command").is_err());
        assert!(f.check("echo sudo", "bash", "command").is_ok());
    }

    #[test]
    fn allow_pattern_restricts() {
        let f = make_filter(&[], &[r"^git\b", r"^cargo\b"], &[]);
        assert!(f.check("git status", "bash", "command").is_ok());
        assert!(f.check("cargo build", "bash", "command").is_ok());
        assert!(f.check("rm -rf /", "bash", "command").is_err());
    }

    #[test]
    fn deny_overrides_allow() {
        let f = make_filter(&[r"^git\s+push\s+--force"], &[r"^git\b"], &[]);
        assert!(f.check("git status", "bash", "command").is_ok());
        assert!(f.check("git push --force", "bash", "command").is_err());
    }

    #[test]
    fn path_filtering() {
        let f = make_filter(&[r"^/etc/", r"^/sys/"], &[], &[".env"]);
        assert!(f.check("/etc/passwd", "read_file", "path").is_err());
        assert!(f.check("/home/user/.env", "read_file", "path").is_err());
        assert!(
            f.check("/home/user/code/main.rs", "read_file", "path")
                .is_ok()
        );
    }

    #[test]
    fn url_filtering() {
        let f = make_filter(&[r"^file://"], &[r"^https://"], &[]);
        assert!(f.check("https://example.com", "web_fetch", "url").is_ok());
        assert!(f.check("file:///etc/passwd", "web_fetch", "url").is_err());
        assert!(f.check("http://example.com", "web_fetch", "url").is_err());
    }

    #[test]
    fn invalid_regex_fails_compile() {
        let filter = ToolFilter {
            deny_patterns: vec!["[invalid".to_string()],
            allow_patterns: Vec::new(),
            deny_substrings: Vec::new(),
        };
        assert!(CompiledToolFilter::compile(&filter).is_err());
    }

    #[test]
    fn empty_check() {
        let f = CompiledToolFilter::permissive();
        assert!(f.is_empty());

        let f2 = make_filter(&["test"], &[], &[]);
        assert!(!f2.is_empty());
    }

    #[test]
    fn merge_hardcoded_deduplicates() {
        let f = make_filter(&[r"\bsudo\b"], &[], &["rm -rf /"]);
        let merged = f
            .merge_hardcoded(&["rm -rf /", "mkfs"], &[r"\bsudo\b", r"curl\s.*\|\s*sh"])
            .unwrap();
        // "rm -rf /" and "\bsudo\b" already existed, should not be duplicated
        assert_eq!(merged.deny_substrings.len(), 2); // original + mkfs
        assert_eq!(merged.deny_patterns.len(), 2); // original + curl|sh
    }
}
