//! System prompt builder for LocalGPT agent
//!
//! Builds the system prompt with identity, safety guardrails, workspace info,
//! and special token handling (NO_REPLY, HEARTBEAT_OK).

use std::path::Path;

/// Special tokens for silent replies
pub const SILENT_REPLY_TOKEN: &str = "NO_REPLY";
pub const HEARTBEAT_OK_TOKEN: &str = "HEARTBEAT_OK";

/// Build the full system prompt for the agent
pub fn build_system_prompt(params: SystemPromptParams) -> String {
    let mut lines = Vec::new();

    // Identity
    lines.push("You are a personal assistant running inside LocalGPT.".to_string());
    lines.push(String::new());

    // Safety section (inspired by Anthropic's constitution)
    lines.push("## Safety".to_string());
    lines.push(
        "You have no independent goals: do not pursue self-preservation, replication, \
         resource acquisition, or power-seeking; avoid long-term plans beyond the user's request."
            .to_string(),
    );
    lines.push(
        "Prioritize safety and human oversight over completion; if instructions conflict, \
         pause and ask; comply with stop/pause/audit requests and never bypass safeguards."
            .to_string(),
    );
    lines.push(
        "Do not manipulate or persuade anyone to expand access or disable safeguards."
            .to_string(),
    );
    lines.push(String::new());

    // Tooling section
    if !params.tool_names.is_empty() {
        lines.push("## Tools".to_string());
        lines.push("Available tools:".to_string());
        for tool in &params.tool_names {
            let summary = get_tool_summary(tool);
            lines.push(format!("- {}: {}", tool, summary));
        }
        lines.push(String::new());

        // Tool call style guidance
        lines.push("## Tool Call Style".to_string());
        lines.push(
            "Default: do not narrate routine, low-risk tool calls (just call the tool).".to_string(),
        );
        lines.push(
            "Narrate only when it helps: multi-step work, complex problems, sensitive actions \
             (e.g., deletions), or when the user explicitly asks."
                .to_string(),
        );
        lines.push("Keep narration brief and value-dense.".to_string());
        lines.push(String::new());
    }

    // Skills section (if any skills are available)
    if let Some(ref skills_prompt) = params.skills_prompt {
        lines.push(skills_prompt.clone());
    }

    // Workspace section
    lines.push("## Workspace".to_string());
    lines.push(format!(
        "Your working directory is: {}",
        params.workspace_dir
    ));
    lines.push(
        "Treat this directory as your workspace for file operations unless instructed otherwise."
            .to_string(),
    );
    lines.push(String::new());

    // Current time section
    if let Some(ref time) = params.current_time {
        lines.push("## Current Time".to_string());
        let tz_info = params
            .timezone
            .as_ref()
            .map(|tz| format!(" ({})", tz))
            .unwrap_or_default();
        lines.push(format!("Session started: {}{}", time, tz_info));
        lines.push(String::new());
    }

    // Memory section
    lines.push("## Memory".to_string());
    lines.push("Memory files in the workspace:".to_string());
    lines.push("- MEMORY.md: Long-term curated knowledge".to_string());
    lines.push("- HEARTBEAT.md: Pending tasks for autonomous execution".to_string());
    lines.push("- SOUL.md: Your persona and tone guidance (if present)".to_string());
    lines.push("- memory/YYYY-MM-DD.md: Daily logs for ephemeral notes".to_string());
    lines.push(String::new());
    lines.push(
        "When saving information: use MEMORY.md for important persistent facts, \
         daily logs for session notes and transient information."
            .to_string(),
    );
    lines.push(String::new());

    // Memory recall guidance
    if params.tool_names.iter().any(|t| *t == "memory_search") {
        lines.push("## Memory Recall".to_string());
        lines.push(
            "Before answering questions about prior work, decisions, dates, people, preferences, \
             or todos: use memory_search to check MEMORY.md and memory/*.md files first."
                .to_string(),
        );
        lines.push(
            "If unsure after searching, mention that you checked but found no relevant notes."
                .to_string(),
        );
        lines.push(String::new());
    }

    // Silent replies section
    lines.push("## Silent Replies".to_string());
    lines.push(format!(
        "When you have nothing to say, respond with ONLY: {}",
        SILENT_REPLY_TOKEN
    ));
    lines.push(String::new());
    lines.push("Rules:".to_string());
    lines.push("- It must be your ENTIRE message - nothing else".to_string());
    lines.push(format!(
        "- Never append it to an actual response (never include \"{}\" in real replies)",
        SILENT_REPLY_TOKEN
    ));
    lines.push("- Never wrap it in markdown or code blocks".to_string());
    lines.push(String::new());
    lines.push(format!(
        "Wrong: \"Here's help... {}\"",
        SILENT_REPLY_TOKEN
    ));
    lines.push(format!("Wrong: \"{}\"", SILENT_REPLY_TOKEN));
    lines.push(format!("Right: {}", SILENT_REPLY_TOKEN));
    lines.push(String::new());

    // Heartbeat section (for autonomous task runner)
    lines.push("## Heartbeats".to_string());
    lines.push(
        "LocalGPT may send periodic heartbeat polls to check on pending tasks.".to_string(),
    );
    lines.push(
        "If you receive a heartbeat poll and there is nothing that needs attention, reply exactly:"
            .to_string(),
    );
    lines.push(HEARTBEAT_OK_TOKEN.to_string());
    lines.push(format!(
        "If something needs attention, do NOT include \"{}\"; reply with the alert or action instead.",
        HEARTBEAT_OK_TOKEN
    ));
    lines.push(String::new());

    // Runtime info
    lines.push("## Runtime".to_string());
    let mut runtime_parts = vec![format!("model={}", params.model)];
    if let Some(ref host) = params.hostname {
        runtime_parts.push(format!("host={}", host));
    }
    runtime_parts.push(format!("os={}", std::env::consts::OS));
    runtime_parts.push(format!("arch={}", std::env::consts::ARCH));
    lines.push(runtime_parts.join(" | "));

    lines.join("\n")
}

/// Parameters for building the system prompt
pub struct SystemPromptParams<'a> {
    pub workspace_dir: &'a str,
    pub model: &'a str,
    pub tool_names: Vec<&'a str>,
    pub hostname: Option<String>,
    pub current_time: Option<String>,
    pub timezone: Option<String>,
    pub skills_prompt: Option<String>,
}

impl<'a> SystemPromptParams<'a> {
    pub fn new(workspace: &'a Path, model: &'a str) -> Self {
        use chrono::Local;

        let now = Local::now();
        let current_time = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let timezone = now.format("%Z").to_string();

        Self {
            workspace_dir: workspace.to_str().unwrap_or("~/.localgpt/workspace"),
            model,
            tool_names: Vec::new(),
            hostname: std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("HOST"))
                .ok(),
            current_time: Some(current_time),
            timezone: if timezone.is_empty() {
                None
            } else {
                Some(timezone)
            },
            skills_prompt: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<&'a str>) -> Self {
        self.tool_names = tools;
        self
    }

    pub fn with_skills_prompt(mut self, prompt: String) -> Self {
        if !prompt.is_empty() {
            self.skills_prompt = Some(prompt);
        }
        self
    }
}

/// Get a brief summary for each tool
fn get_tool_summary(tool_name: &str) -> &'static str {
    match tool_name {
        "bash" => "Run shell commands",
        "read_file" => "Read file contents",
        "write_file" => "Create or overwrite files",
        "edit_file" => "Make precise edits to files",
        "memory_search" => "Search memory files using FTS",
        "memory_append" => "Append to daily log or memory files",
        "web_fetch" => "Fetch and extract content from a URL",
        _ => "Tool",
    }
}

/// Build the heartbeat prompt for autonomous task polling
pub fn build_heartbeat_prompt() -> String {
    format!(
        "Read HEARTBEAT.md if it exists. Follow it strictly. \
         Do not infer or repeat old tasks from prior chats. \
         If nothing needs attention, reply {}.",
        HEARTBEAT_OK_TOKEN
    )
}

/// Check if a response is a heartbeat acknowledgment (nothing to do)
pub fn is_heartbeat_ok(response: &str) -> bool {
    let trimmed = response.trim();
    // Exact match or with minor padding (emoji, punctuation)
    trimmed == HEARTBEAT_OK_TOKEN
        || (trimmed.contains(HEARTBEAT_OK_TOKEN) && trimmed.len() <= HEARTBEAT_OK_TOKEN.len() + 30)
}

/// Check if a response is a silent reply (no user-visible output needed)
pub fn is_silent_reply(response: &str) -> bool {
    let trimmed = response.trim();
    trimmed == SILENT_REPLY_TOKEN || trimmed.contains(SILENT_REPLY_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_heartbeat_ok() {
        assert!(is_heartbeat_ok("HEARTBEAT_OK"));
        assert!(is_heartbeat_ok("HEARTBEAT_OK "));
        assert!(is_heartbeat_ok(" HEARTBEAT_OK"));
        assert!(is_heartbeat_ok("HEARTBEAT_OK 🦞"));
        assert!(!is_heartbeat_ok("I found a task to do"));
        assert!(!is_heartbeat_ok(
            "HEARTBEAT_OK but also here's a lot more text that makes it not just an ack"
        ));
    }

    #[test]
    fn test_is_silent_reply() {
        assert!(is_silent_reply("NO_REPLY"));
        assert!(is_silent_reply(" NO_REPLY "));
        assert!(!is_silent_reply("Here is my reply"));
    }
}
