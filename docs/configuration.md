---
sidebar_position: 14
---

# Configuration

LocalGPT is configured via a TOML file at `~/.localgpt/config.toml`.

## Quick Start

Create the config file:

```bash
mkdir -p ~/.localgpt
cat > ~/.localgpt/config.toml << 'EOF'
[agent]
default_model = "claude-cli/opus"
EOF
```

## Full Configuration Reference

```toml
# LocalGPT Configuration
# ~/.localgpt/config.toml

#──────────────────────────────────────────────────────────────────────────────
# Agent Settings
#──────────────────────────────────────────────────────────────────────────────

[agent]
# Default model to use for chat
# Prefix determines provider:
#   claude-cli/* → Claude CLI (uses installed claude command)
#   gpt-* / o1-* → OpenAI
#   claude-* → Anthropic API
#   else → Ollama
default_model = "claude-cli/opus"

# Context window size (in tokens)
# Common values: 128000 (GPT-4), 200000 (Claude), 8192 (older models)
context_window = 128000

# Reserve tokens for the response
# Ensures the model has room to generate a response
reserve_tokens = 8000

#──────────────────────────────────────────────────────────────────────────────
# Provider Configuration
#──────────────────────────────────────────────────────────────────────────────

[providers.openai]
# API key (supports environment variable expansion)
api_key = "${OPENAI_API_KEY}"

# API base URL (optional, for proxies or Azure)
base_url = "https://api.openai.com/v1"

[providers.anthropic]
# Anthropic API key
api_key = "${ANTHROPIC_API_KEY}"

# API base URL (optional)
base_url = "https://api.anthropic.com"

[providers.ollama]
# Ollama server endpoint
endpoint = "http://localhost:11434"

# Default model for Ollama
model = "llama3"

#──────────────────────────────────────────────────────────────────────────────
# Heartbeat Settings
#──────────────────────────────────────────────────────────────────────────────

[heartbeat]
# Enable automatic heartbeat
enabled = true

# How often to check HEARTBEAT.md
# Formats: "30m", "1h", "2h30m", "90s"
interval = "30m"

# Only run during these hours (optional)
# Prevents late-night activity
active_hours = { start = "09:00", end = "22:00" }

# Timezone for active hours (optional)
# Uses system timezone if not specified
# timezone = "America/Los_Angeles"

#──────────────────────────────────────────────────────────────────────────────
# Memory Settings
#──────────────────────────────────────────────────────────────────────────────

[memory]
# Where to store memory files
# Supports ~ for home directory
workspace = "~/.localgpt/workspace"

# Chunk size for indexing (tokens)
# Smaller = more precise search, larger = more context
chunk_size = 400

# Overlap between chunks (tokens)
# Ensures context isn't lost at chunk boundaries
chunk_overlap = 80

# Embedding provider for semantic search
# Options: "local" (fastembed), "openai", "gguf", "none"
embedding_provider = "local"

# Embedding model
# For "local": "BAAI/bge-small-en-v1.5" (default), or multilingual models
# For "openai": "text-embedding-3-small"
# For "gguf": path to .gguf file
embedding_model = "BAAI/bge-small-en-v1.5"

# Cache directory for downloaded embedding models
embedding_cache_dir = "~/.localgpt/cache/embeddings"

# Additional paths to index (outside workspace)
# external_paths = ["~/projects/notes"]

#──────────────────────────────────────────────────────────────────────────────
# HTTP Server Settings
#──────────────────────────────────────────────────────────────────────────────

[server]
# Enable HTTP server when daemon starts
enabled = true

# Port to listen on
port = 31327

# Bind address
# "127.0.0.1" = localhost only (secure)
# "0.0.0.0" = all interfaces (use with caution)
bind = "127.0.0.1"

#──────────────────────────────────────────────────────────────────────────────
# Logging Settings
#──────────────────────────────────────────────────────────────────────────────

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log file path
file = "~/.localgpt/logs/agent.log"
```

## Environment Variables

API keys and other sensitive values can reference environment variables:

```toml
api_key = "${OPENAI_API_KEY}"
```

This expands to the value of the `OPENAI_API_KEY` environment variable at runtime.

### Setting Environment Variables

**Bash/Zsh:**
```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
```

**Fish:**
```fish
set -gx OPENAI_API_KEY "sk-..."
```

**In ~/.bashrc or ~/.zshrc:**
```bash
export OPENAI_API_KEY="sk-..."
```

## Provider-Specific Configuration

### OpenAI

```toml
[agent]
default_model = "openai/gpt-4o"  # or openai/gpt-4o-mini, or alias: gpt

[providers.openai]
api_key = "${OPENAI_API_KEY}"
```

### Anthropic

```toml
[agent]
default_model = "anthropic/claude-opus-4-5"  # or anthropic/claude-sonnet-4-5, or alias: opus

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
```

### Claude CLI

If you have the `claude` CLI installed, LocalGPT can use it directly:

```toml
[agent]
default_model = "claude-cli/opus"  # or claude-cli/sonnet, claude-cli/haiku
```

No API key configuration needed - uses your existing Claude CLI authentication.

### Ollama (Local)

```toml
[agent]
default_model = "llama3"  # or mistral, codellama, etc.

[providers.ollama]
endpoint = "http://localhost:11434"
```

For fully local operation, only configure Ollama (no API keys needed). Tool calling is supported for Ollama models that have tool calling capability.

## Validate Configuration

Check your configuration:

```bash
localgpt config show
```

This displays the loaded configuration with sensitive values masked.

## Workspace Path Customization

LocalGPT supports multiple workspaces via environment variables (OpenClaw-compatible):

```bash
# Use a custom workspace directory (absolute path)
export LOCALGPT_WORKSPACE=~/my-project/ai-workspace
localgpt chat

# Use profile-based workspaces
export LOCALGPT_PROFILE=work    # uses ~/.localgpt/workspace-work
export LOCALGPT_PROFILE=home    # uses ~/.localgpt/workspace-home
```

Resolution order:
1. `LOCALGPT_WORKSPACE` env var (absolute path override)
2. `LOCALGPT_PROFILE` env var (creates `~/.localgpt/workspace-{profile}`)
3. `memory.workspace` from config file
4. Default: `~/.localgpt/workspace`

## Configuration Precedence

Configuration is loaded in this order (later overrides earlier):

1. Default values
2. `~/.localgpt/config.toml`
3. Environment variables
4. Command-line flags (`-m`, `--model`, etc.)

## Common Issues

### API Key Not Found

```
Error: OpenAI API key not configured
```

**Solution:** Set the environment variable or add to config:
```bash
export OPENAI_API_KEY="sk-..."
```

### Invalid Model

```
Error: Unknown model: gpt5
```

**Solution:** Check the model name. Valid prefixes:
- `gpt-*` for OpenAI
- `claude-*` for Anthropic
- Anything else for Ollama

### Permission Denied

```
Error: Cannot write to ~/.localgpt/workspace
```

**Solution:** Create the directory with proper permissions:
```bash
mkdir -p ~/.localgpt/workspace
chmod 700 ~/.localgpt
```
