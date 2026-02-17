pub mod web_search;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

use super::providers::ToolSchema;
use crate::config::{Config, SearchProviderType};
use crate::memory::MemoryManager;

use web_search::{SearchRouter, WebSearchTool};

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

/// Create the safe (mobile-compatible) tools: memory search, memory get, web fetch, web search.
///
/// Dangerous tools (bash, read_file, write_file, edit_file) are provided by the CLI crate.
/// Use `Agent::new_with_tools()` to supply the full tool set.
pub fn create_safe_tools(
    config: &Config,
    memory: Option<Arc<MemoryManager>>,
) -> Result<Vec<Box<dyn Tool>>> {
    let workspace = config.workspace_path();

    // Use indexed memory search if MemoryManager is provided, otherwise fallback to grep-based
    let memory_search_tool: Box<dyn Tool> = if let Some(ref mem) = memory {
        Box::new(MemorySearchToolWithIndex::new(Arc::clone(mem)))
    } else {
        Box::new(MemorySearchTool::new(workspace.clone()))
    };

    let mut tools: Vec<Box<dyn Tool>> = vec![
        memory_search_tool,
        Box::new(MemoryGetTool::new(workspace)),
        Box::new(WebFetchTool::new(config.tools.web_fetch_max_bytes)),
    ];

    // Conditionally add web search tool
    if let Some(ref ws_config) = config.tools.web_search
        && !matches!(ws_config.provider, SearchProviderType::None)
    {
        match SearchRouter::from_config(ws_config) {
            Ok(router) => tools.push(Box::new(WebSearchTool::new(Arc::new(router)))),
            Err(e) => tracing::warn!("Web search init failed: {e}"),
        }
    }

    Ok(tools)
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
        if memory_file.exists()
            && let Ok(content) = fs::read_to_string(&memory_file)
        {
            for (i, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&query.to_lowercase()) {
                    results.push(format!("MEMORY.md:{}: {}", i + 1, line));
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }

        // Search daily logs
        let memory_dir = self.workspace.join("memory");
        if memory_dir.exists()
            && let Ok(entries) = fs::read_dir(&memory_dir)
        {
            for entry in entries.filter_map(|e| e.ok()) {
                if results.len() >= limit {
                    break;
                }

                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false)
                    && let Ok(content) = fs::read_to_string(&path)
                {
                    let filename = path.file_name().unwrap().to_string_lossy();
                    for (i, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&query.to_lowercase()) {
                            results.push(format!("memory/{}:{}: {}", filename, i + 1, line));
                            if results.len() >= limit {
                                break;
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
}

impl WebFetchTool {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            max_bytes,
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
        "web_search" => args
            .get("query")
            .and_then(|v| v.as_str())
            .map(|s| format!("\"{}\"", s)),
        _ => None,
    }
}
