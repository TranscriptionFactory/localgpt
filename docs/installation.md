---
sidebar_position: 2
---

# Installation

## Prerequisites

- **Rust 1.70+** - Install from [rustup.rs](https://rustup.rs)
- **An LLM API key** (at least one of):
  - OpenAI API key
  - Anthropic API key
  - Local Ollama installation

## Install from crates.io

```bash
# Full install (includes desktop GUI)
cargo install localgpt

# Headless install (no desktop GUI — for servers, Docker, CI)
cargo install localgpt --no-default-features
```

## Building from Source

```bash
# Clone the repository
git clone https://github.com/localgpt-app/localgpt.git
cd localgpt

# Build release binary (includes desktop GUI)
cargo build --release

# Build headless (no desktop GUI — skips eframe/egui/winit)
cargo build --release --no-default-features

# The binary will be at target/release/localgpt
```

## Docker / Headless Server

For Docker or headless environments where display servers (X11/Wayland) are unavailable, build without the desktop feature to avoid `winit` compilation issues:

```bash
cargo build --release --no-default-features
```

Or in a Dockerfile:

```dockerfile
FROM rust:1.83 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --no-default-features

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/localgpt /usr/local/bin/
CMD ["localgpt", "daemon", "start", "--foreground"]
```

The headless binary includes all features except the desktop GUI: CLI, web UI, HTTP API, WebSocket, daemon mode, and heartbeat.

## Installation

Copy the binary to your PATH:

```bash
# Option 1: Install to /usr/local/bin
sudo cp target/release/localgpt /usr/local/bin/

# Option 2: Install to ~/.local/bin (no sudo required)
mkdir -p ~/.local/bin
cp target/release/localgpt ~/.local/bin/
```

## Initial Setup

1. **Create the configuration directory:**

```bash
mkdir -p ~/.localgpt/workspace/memory
```

2. **Create the configuration file:**

```bash
cp config.example.toml ~/.localgpt/config.toml
```

3. **Edit the configuration with your API key:**

```bash
# Set your API key in the environment or edit config.toml
export OPENAI_API_KEY="your-api-key"
```

## Verify Installation

```bash
# Check version and help
localgpt --help

# Test with a simple question
localgpt ask "What is 2+2?"
```

## Linux Desktop Build

On Linux, the desktop GUI requires X11 or Wayland development libraries. If building with the desktop feature:

```bash
# Debian/Ubuntu
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev

# Or build headless to skip these requirements
cargo build --release --no-default-features
```

## Using with Ollama (Local Models)

If you prefer fully local operation with Ollama:

1. Install Ollama from [ollama.ai](https://ollama.ai)
2. Pull a model: `ollama pull llama3`
3. Update your config:

```toml
[agent]
default_model = "llama3"

[providers.ollama]
endpoint = "http://localhost:11434"
```

Ollama models with tool calling capability (e.g., `llama3`, `mistral`) support all 7 built-in tools.
