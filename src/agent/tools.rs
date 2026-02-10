use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

use super::providers::ToolSchema;
use super::hardcoded_filters;
use super::tool_filters::CompiledToolFilter;
use crate::agent::tool_filters::ToolFilter;
use crate::config::Config;
use crate::memory::MemoryManager;

// --- Helper: look up and compile a filter for a given tool name ---

fn compile_filter_for(
    filters: &HashMap<String, ToolFilter>,
    tool_name: &str,
) -> anyhow::Result<CompiledToolFilter> {
    match filters.get(tool_name) {
        Some(filter) => CompiledToolFilter::compile(filter),
        None => Ok(CompiledToolFilter::permissive()),
    }
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> ToolSchema;
    async fn execute(&self, arguments: &str) -> Result<String>;
}

pub fn create_default_tools(
    config: &Config,
    memory: Option<Arc<MemoryManager>>,
) -> Result<Vec<Box<dyn Tool>>> {
    let workspace = config.workspace_path();
    let state_dir = workspace
        .parent()
        .unwrap_or_else(|| std::path::Path::new("~/.localgpt"))
        .to_path_buf();
    let filters = &config.tools.filters;

    // Compile user filters then merge hardcoded defaults
    let bash_filter = compile_filter_for(filters, "bash")?.merge_hardcoded(
        hardcoded_filters::BASH_DENY_SUBSTRINGS,
        hardcoded_filters::BASH_DENY_PATTERNS,
    )?;

    let web_fetch_filter = compile_filter_for(filters, "web_fetch")?.merge_hardcoded(
        hardcoded_filters::WEB_FETCH_DENY_SUBSTRINGS,
        hardcoded_filters::WEB_FETCH_DENY_PATTERNS,
    )?;

    // Pre-canonicalize allowed directories for path scoping
    let allowed_directories: Vec<PathBuf> = config
        .security
        .allowed_directories
        .iter()
        .filter_map(|d| {
            let expanded = shellexpand::tilde(d).to_string();
            std::fs::canonicalize(&expanded).ok()
        })
        .collect();

    // Use indexed memory search if MemoryManager is provided, otherwise fallback to grep-based
    let memory_search_tool: Box<dyn Tool> = if let Some(ref mem) = memory {
        Box::new(MemorySearchToolWithIndex::new(Arc::clone(mem)))
    } else {
        Box::new(MemorySearchTool::new(workspace.clone()))
    };

    Ok(vec![
        Box::new(BashTool::new(
            config.tools.bash_timeout_ms,
            state_dir.clone(),
            bash_filter,
            config.security.strict_policy,
            workspace.clone(),
            config.security.env_deny_patterns.clone(),
        )),
        Box::new(ReadFileTool::new(
            compile_filter_for(filters, "read_file")?,
            allowed_directories.clone(),
        )),
        Box::new(WriteFileTool::new(
            state_dir.clone(),
            compile_filter_for(filters, "write_file")?,
            allowed_directories.clone(),
        )),
        Box::new(EditFileTool::new(
            state_dir,
            compile_filter_for(filters, "edit_file")?,
            allowed_directories,
        )),
        memory_search_tool,
        Box::new(MemoryGetTool::new(workspace)),
        Box::new(WebFetchTool::new(
            config.tools.web_fetch_max_bytes,
            web_fetch_filter,
        )),
    ])
}

/// Resolve a path to its real (canonical) form.
/// Expands tilde, then canonicalizes. For new files (that don't exist yet),
/// canonicalizes the parent directory and appends the filename.
fn resolve_real_path(path: &str) -> Result<PathBuf> {
    let expanded = shellexpand::tilde(path).to_string();
    let p = PathBuf::from(&expanded);

    // Try canonicalize directly (works for existing paths)
    if let Ok(canonical) = fs::canonicalize(&p) {
        return Ok(canonical);
    }

    // For new files: canonicalize parent, append filename
    if let (Some(parent), Some(filename)) = (p.parent(), p.file_name()) {
        if let Ok(canonical_parent) = fs::canonicalize(parent) {
            return Ok(canonical_parent.join(filename));
        }
    }

    // Fallback: return the expanded path as-is
    Ok(p)
}

/// Check whether a resolved path is within one of the allowed directories.
/// If `allowed_dirs` is empty, all paths are allowed (unrestricted mode).
fn check_path_allowed(real_path: &std::path::Path, allowed_dirs: &[PathBuf]) -> Result<()> {
    if allowed_dirs.is_empty() {
        return Ok(());
    }

    for dir in allowed_dirs {
        if real_path.starts_with(dir) {
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "Path denied: {} is outside allowed directories",
        real_path.display()
    ))
}

pub struct BashTool {
    default_timeout_ms: u64,
    state_dir: PathBuf,
    filter: CompiledToolFilter,
    strict_policy: bool,
    workspace_path: PathBuf,
    env_deny_patterns: Vec<String>,
}

impl BashTool {
    pub fn new(
        default_timeout_ms: u64,
        state_dir: PathBuf,
        filter: CompiledToolFilter,
        strict_policy: bool,
        workspace_path: PathBuf,
        env_deny_patterns: Vec<String>,
    ) -> Self {
        Self {
            default_timeout_ms,
            state_dir,
            filter,
            strict_policy,
            workspace_path,
            env_deny_patterns,
        }
    }

    /// Check if an env var name matches any deny pattern.
    /// Patterns use simple glob: `*_KEY` means ends_with("_KEY"),
    /// `SECRET_*` means starts_with("SECRET_"), `*SECRET*` means contains("SECRET").
    fn env_var_denied(&self, name: &str) -> bool {
        let name_upper = name.to_uppercase();
        self.env_deny_patterns.iter().any(|pattern| {
            let p = pattern.to_uppercase();
            if let Some(inner) = p.strip_prefix('*').and_then(|s| s.strip_suffix('*')) {
                // *FOO* → contains
                name_upper.contains(inner)
            } else if let Some(suffix) = p.strip_prefix('*') {
                // *_KEY → ends_with
                name_upper.ends_with(suffix)
            } else if let Some(prefix) = p.strip_suffix('*') {
                // SECRET_* → starts_with
                name_upper.starts_with(prefix)
            } else {
                // Exact match
                name_upper == p
            }
        })
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "bash".to_string(),
            description: "Execute a bash command and return the output".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": format!("Optional timeout in milliseconds (default: {})", self.default_timeout_ms)
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing command"))?;

        // Filter on the command string (hardcoded + user-configured)
        self.filter.check(command, "bash", "command")?;

        let timeout_ms = args["timeout_ms"]
            .as_u64()
            .unwrap_or(self.default_timeout_ms);

        // Best-effort protected file check for bash commands
        let suspicious = crate::security::check_bash_command(command);
        if !suspicious.is_empty() {
            let detail = format!(
                "Bash command references protected files: {:?} (cmd: {})",
                suspicious,
                &command[..command.len().min(200)]
            );
            let _ = crate::security::append_audit_entry_with_detail(
                &self.state_dir,
                crate::security::AuditAction::WriteBlocked,
                "",
                "tool:bash",
                Some(&detail),
            );

            if self.strict_policy {
                anyhow::bail!(
                    "Blocked: bash command references protected files: {:?}",
                    suspicious
                );
            }

            tracing::warn!("Bash command may modify protected files: {:?}", suspicious);
        }

        debug!(
            "Executing bash command (timeout: {}ms): {}",
            timeout_ms, command
        );

        // Build environment with sensitive vars filtered out
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(command);

        if !self.env_deny_patterns.is_empty() {
            // Clear inherited env and re-add only non-denied vars
            cmd.env_clear();
            for (key, value) in std::env::vars() {
                if !self.env_var_denied(&key) {
                    cmd.env(&key, &value);
                }
            }
        }

        // Run command with timeout
        let timeout_duration = std::time::Duration::from_millis(timeout_ms);
        let output = tokio::time::timeout(timeout_duration, cmd.output())
            .await
            .map_err(|_| anyhow::anyhow!("Command timed out after {}ms", timeout_ms))??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();

        if !stdout.is_empty() {
            result.push_str(&stdout);
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n\nSTDERR:\n");
            }
            result.push_str(&stderr);
        }

        if result.is_empty() {
            result = format!(
                "Command completed with exit code: {}",
                output.status.code().unwrap_or(-1)
            );
        }

        // Post-exec integrity: re-verify HMAC if command may have touched workspace
        let workspace_str = self.workspace_path.to_string_lossy();
        let references_workspace = command.contains(workspace_str.as_ref())
            || command.contains("LocalGPT.md")
            || command.contains("localgpt.md");

        if references_workspace {
            let state_dir = self.workspace_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("~/.localgpt"));

            match crate::security::load_and_verify_policy(&self.workspace_path, state_dir) {
                crate::security::PolicyVerification::TamperDetected => {
                    let _ = crate::security::append_audit_entry(
                        state_dir,
                        crate::security::AuditAction::TamperDetected,
                        "",
                        "post_exec_check",
                    );
                    if self.strict_policy {
                        return Err(anyhow::anyhow!(
                            "Security policy tamper detected after bash execution. \
                             The command may have modified LocalGPT.md."
                        ));
                    }
                    result.push_str(
                        "\n\n[WARNING: Security policy tamper detected after execution]"
                    );
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

// Read File Tool
pub struct ReadFileTool {
    filter: CompiledToolFilter,
    allowed_directories: Vec<PathBuf>,
}

impl ReadFileTool {
    pub fn new(filter: CompiledToolFilter, allowed_directories: Vec<PathBuf>) -> Self {
        Self {
            filter,
            allowed_directories,
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from (0-indexed)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;

        // Resolve symlinks before any checks
        let real_path = resolve_real_path(path)?;
        let path_str = real_path.to_string_lossy().to_string();

        // Check path scoping
        check_path_allowed(&real_path, &self.allowed_directories)?;

        // Filter on the resolved path
        self.filter.check(&path_str, "read_file", "path")?;

        debug!("Reading file: {}", path_str);

        let content = fs::read_to_string(&real_path)?;

        // Handle offset and limit
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().map(|l| l as usize);

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.min(total_lines);
        let end = limit
            .map(|l| (start + l).min(total_lines))
            .unwrap_or(total_lines);

        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4}\t{}", start + i + 1, line))
            .collect();

        Ok(selected.join("\n"))
    }
}

// Write File Tool
pub struct WriteFileTool {
    state_dir: PathBuf,
    filter: CompiledToolFilter,
    allowed_directories: Vec<PathBuf>,
}

impl WriteFileTool {
    pub fn new(
        state_dir: PathBuf,
        filter: CompiledToolFilter,
        allowed_directories: Vec<PathBuf>,
    ) -> Self {
        Self {
            state_dir,
            filter,
            allowed_directories,
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "write_file".to_string(),
            description: "Write content to a file (creates or overwrites)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing content"))?;

        // Resolve symlinks before any checks
        let real_path = resolve_real_path(path)?;
        let path_str = real_path.to_string_lossy().to_string();

        // Check path scoping
        check_path_allowed(&real_path, &self.allowed_directories)?;

        // Filter on the resolved path
        self.filter.check(&path_str, "write_file", "path")?;

        // Check protected files
        if let Some(name) = real_path.file_name().and_then(|n| n.to_str()) {
            if crate::security::is_workspace_file_protected(name) {
                let detail = format!("Agent attempted write to {}", real_path.display());
                let _ = crate::security::append_audit_entry_with_detail(
                    &self.state_dir,
                    crate::security::AuditAction::WriteBlocked,
                    "",
                    "tool:write_file",
                    Some(&detail),
                );
                anyhow::bail!(
                    "Cannot write to protected file: {}. This file is managed by the security system. \
                     Use `localgpt security sign` to update the security policy.",
                    real_path.display()
                );
            }
        }

        debug!("Writing file: {}", real_path.display());

        // Create parent directories if needed
        if let Some(parent) = real_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&real_path, content)?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            real_path.display()
        ))
    }
}

// Edit File Tool
pub struct EditFileTool {
    state_dir: PathBuf,
    filter: CompiledToolFilter,
    allowed_directories: Vec<PathBuf>,
}

impl EditFileTool {
    pub fn new(
        state_dir: PathBuf,
        filter: CompiledToolFilter,
        allowed_directories: Vec<PathBuf>,
    ) -> Self {
        Self {
            state_dir,
            filter,
            allowed_directories,
        }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "edit_file".to_string(),
            description: "Edit a file by replacing old_string with new_string".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The text to replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement text"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let old_string = args["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing old_string"))?;
        let new_string = args["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing new_string"))?;
        let replace_all = args["replace_all"].as_bool().unwrap_or(false);

        // Resolve symlinks before any checks
        let real_path = resolve_real_path(path)?;
        let path_str = real_path.to_string_lossy().to_string();

        // Check path scoping
        check_path_allowed(&real_path, &self.allowed_directories)?;

        // Filter on the resolved path
        self.filter.check(&path_str, "edit_file", "path")?;

        // Check protected files
        if let Some(name) = real_path.file_name().and_then(|n| n.to_str()) {
            if crate::security::is_workspace_file_protected(name) {
                let detail = format!("Agent attempted edit to {}", real_path.display());
                let _ = crate::security::append_audit_entry_with_detail(
                    &self.state_dir,
                    crate::security::AuditAction::WriteBlocked,
                    "",
                    "tool:edit_file",
                    Some(&detail),
                );
                anyhow::bail!(
                    "Cannot edit protected file: {}. This file is managed by the security system.",
                    real_path.display()
                );
            }
        }

        debug!("Editing file: {}", real_path.display());

        let content = fs::read_to_string(&real_path)?;

        let (new_content, count) = if replace_all {
            let count = content.matches(old_string).count();
            (content.replace(old_string, new_string), count)
        } else if content.contains(old_string) {
            (content.replacen(old_string, new_string, 1), 1)
        } else {
            return Err(anyhow::anyhow!("old_string not found in file"));
        };

        fs::write(&real_path, &new_content)?;

        Ok(format!("Replaced {} occurrence(s) in {}", count, path_str))
    }
}

// Memory Search Tool
pub struct MemorySearchTool {
    workspace: PathBuf,
}

impl MemorySearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "memory_search".to_string(),
            description: "Search the memory index for relevant information".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        debug!("Memory search: {} (limit: {})", query, limit);

        // Simple grep-based search for now
        // TODO: Use proper memory index
        let mut results = Vec::new();

        let memory_file = self.workspace.join("MEMORY.md");
        if memory_file.exists() {
            if let Ok(content) = fs::read_to_string(&memory_file) {
                for (i, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&query.to_lowercase()) {
                        results.push(format!("MEMORY.md:{}: {}", i + 1, line));
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        // Search daily logs
        let memory_dir = self.workspace.join("memory");
        if memory_dir.exists() {
            if let Ok(entries) = fs::read_dir(&memory_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if results.len() >= limit {
                        break;
                    }

                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        if let Ok(content) = fs::read_to_string(&path) {
                            let filename = path.file_name().unwrap().to_string_lossy();
                            for (i, line) in content.lines().enumerate() {
                                if line.to_lowercase().contains(&query.to_lowercase()) {
                                    results.push(format!(
                                        "memory/{}:{}: {}",
                                        filename,
                                        i + 1,
                                        line
                                    ));
                                    if results.len() >= limit {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok("No results found".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}

// Memory Search Tool with Index - uses MemoryManager for hybrid FTS+vector search
pub struct MemorySearchToolWithIndex {
    memory: Arc<MemoryManager>,
}

impl MemorySearchToolWithIndex {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemorySearchToolWithIndex {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn schema(&self) -> ToolSchema {
        let description = if self.memory.has_embeddings() {
            "Search the memory index using hybrid semantic + keyword search for relevant information"
        } else {
            "Search the memory index for relevant information"
        };

        ToolSchema {
            name: "memory_search".to_string(),
            description: description.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        let search_type = if self.memory.has_embeddings() {
            "hybrid"
        } else {
            "FTS"
        };
        debug!(
            "Memory search ({}): {} (limit: {})",
            search_type, query, limit
        );

        let results = self.memory.search(query, limit)?;

        if results.is_empty() {
            return Ok("No results found".to_string());
        }

        // Format results with relevance scores
        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                let preview: String = chunk.content.chars().take(200).collect();
                let preview = preview.replace('\n', " ");
                format!(
                    "{}. {} (lines {}-{}, score: {:.3})\n   {}{}",
                    i + 1,
                    chunk.file,
                    chunk.line_start,
                    chunk.line_end,
                    chunk.score,
                    preview,
                    if chunk.content.len() > 200 { "..." } else { "" }
                )
            })
            .collect();

        Ok(formatted.join("\n\n"))
    }
}

// Memory Get Tool - efficient snippet fetching after memory_search
pub struct MemoryGetTool {
    workspace: PathBuf,
}

impl MemoryGetTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        // Handle paths relative to workspace
        if path.starts_with("memory/") || path == "MEMORY.md" || path == "HEARTBEAT.md" {
            self.workspace.join(path)
        } else {
            PathBuf::from(shellexpand::tilde(path).to_string())
        }
    }
}

#[async_trait]
impl Tool for MemoryGetTool {
    fn name(&self) -> &str {
        "memory_get"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "memory_get".to_string(),
            description: "Safe snippet read from MEMORY.md or memory/*.md with optional line range; use after memory_search to pull only the needed lines and keep context small.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file (e.g., 'MEMORY.md' or 'memory/2024-01-15.md')"
                    },
                    "from": {
                        "type": "integer",
                        "description": "Starting line number (1-indexed, default: 1)"
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Number of lines to read (default: 50)"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;

        let from = args["from"].as_u64().unwrap_or(1).max(1) as usize;
        let lines_count = args["lines"].as_u64().unwrap_or(50) as usize;

        let resolved_path = self.resolve_path(path);

        debug!(
            "Memory get: {} (from: {}, lines: {})",
            resolved_path.display(),
            from,
            lines_count
        );

        if !resolved_path.exists() {
            return Ok(format!("File not found: {}", path));
        }

        let content = fs::read_to_string(&resolved_path)?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Convert from 1-indexed to 0-indexed
        let start = (from - 1).min(total_lines);
        let end = (start + lines_count).min(total_lines);

        if start >= total_lines {
            return Ok(format!(
                "Line {} is past end of file ({} lines)",
                from, total_lines
            ));
        }

        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4}\t{}", start + i + 1, line))
            .collect();

        let header = format!(
            "# {} (lines {}-{} of {})\n",
            path,
            start + 1,
            end,
            total_lines
        );
        Ok(header + &selected.join("\n"))
    }
}

// Web Fetch Tool
pub struct WebFetchTool {
    client: reqwest::Client,
    max_bytes: usize,
    filter: CompiledToolFilter,
}

impl WebFetchTool {
    pub fn new(max_bytes: usize, filter: CompiledToolFilter) -> Self {
        Self {
            client: reqwest::Client::new(),
            max_bytes,
            filter,
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "web_fetch".to_string(),
            description: "Fetch content from a URL".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: Value = serde_json::from_str(arguments)?;
        let url = args["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        // Filter on the URL
        self.filter.check(url, "web_fetch", "url")?;

        debug!("Fetching URL: {}", url);

        let response = self
            .client
            .get(url)
            .header("User-Agent", "LocalGPT/0.1")
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        // Truncate if too long
        let truncated = if body.len() > self.max_bytes {
            format!(
                "{}...\n\n[Truncated, {} bytes total]",
                &body[..self.max_bytes],
                body.len()
            )
        } else {
            body
        };

        Ok(format!("Status: {}\n\n{}", status, truncated))
    }
}

/// Extract relevant detail from tool arguments for display.
/// Returns a human-readable summary of the key argument (file path, command, query, URL).
pub fn extract_tool_detail(tool_name: &str, arguments: &str) -> Option<String> {
    let args: Value = serde_json::from_str(arguments).ok()?;

    match tool_name {
        "edit_file" | "write_file" | "read_file" => args
            .get("path")
            .or_else(|| args.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "bash" => args.get("command").and_then(|v| v.as_str()).map(|s| {
            if s.len() > 60 {
                format!("{}...", &s[..57])
            } else {
                s.to_string()
            }
        }),
        "memory_search" => args
            .get("query")
            .and_then(|v| v.as_str())
            .map(|s| format!("\"{}\"", s)),
        "web_fetch" => args
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}
