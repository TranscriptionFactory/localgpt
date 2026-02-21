# Port Directory/Command Permissions to Multi-Crate Workspace

## Context

Security hardening commit `46c9d72` (15 items) was written against old flat `src/` layout. Upstream restructured into `crates/`. This plan ports only the **directory/command permission** subset for a focused, clean PR.

**Branches:**
- `security-hardening-legacy` — pre-rebase commit (`3cd65c1`) for `cargo install` of old version (DONE)
- `feat/security-permissions` — new branch off `main` for this work

**Scope:** Hardcoded deny filters, tool filter infrastructure, path scoping, symlink resolution. Excludes rate limiting, secret scanning, approval callbacks, server auth, telegram TTL, output truncation (separate PRs later).

---

## Commit 1: Add tool filter infrastructure to core

**New files in `crates/core/src/agent/`:**

- `tool_filters.rs` — `ToolFilter` (serde config struct), `CompiledToolFilter` (runtime), `merge_hardcoded()`. From `46c9d72:src/agent/tool_filters.rs`.
- `hardcoded_filters.rs` — Const deny lists: bash (dangerous commands, sudo, pipe-to-shell) + web_fetch (SSRF: localhost, metadata, private IPs, file://). From `46c9d72:src/agent/hardcoded_filters.rs`.
- `path_utils.rs` — `resolve_real_path()` (tilde expand + canonicalize, handles nonexistent files), `check_path_allowed()` (enforce allowed_directories). Extracted from `46c9d72:src/agent/tools.rs`.

**Modified:**
- `crates/core/src/agent/mod.rs` — register `pub mod tool_filters, hardcoded_filters, path_utils;`
- `crates/core/src/config/mod.rs` — add to `SecurityConfig`: `allowed_directories: Vec<String>`. Add to `ToolsConfig`: `filters: HashMap<String, ToolFilter>`. Add `use std::collections::HashMap;` and `use crate::agent::tool_filters::ToolFilter;`.
- `crates/core/src/security/audit.rs` — add `PathDenied` variant to `AuditAction`

---

## Commit 2: Apply filters + path scoping to CLI tools

**Modified: `crates/cli/src/tools.rs`**

- **BashTool** += `filter: CompiledToolFilter`, `strict_policy: bool`
  - In `execute()`: `self.filter.check(command)?` before execution
  - Strict mode: error (not just warn) on protected file references

- **ReadFileTool, WriteFileTool, EditFileTool** += `filter: CompiledToolFilter`, `allowed_directories: Vec<PathBuf>`
  - In `execute()`: resolve symlinks → check path scoping → filter check → existing logic
  - Audit `PathDenied` on scoping violation

- **`create_cli_tools()`**: compile per-tool filters from config, merge hardcoded defaults for bash, canonicalize `allowed_directories`, pass new params to constructors

---

## Commit 3: Apply hardcoded filters to WebFetchTool in core

**Modified: `crates/core/src/agent/tools/mod.rs`**

- `WebFetchTool` += `filter: CompiledToolFilter`
- In `execute()`: `self.filter.check(url)?` before fetching
- In `create_safe_tools()`: compile web_fetch filter from config + merge hardcoded SSRF deny patterns

---

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo check -p localgpt-mobile --target aarch64-apple-ios  # no platform deps in core
cargo clippy --workspace
cargo run -- ask "what is 2+2"  # smoke test
```

Manual test: set `security.allowed_directories = ["/tmp"]` in config, attempt file read outside `/tmp` → should deny.

## Key files to modify
- `crates/core/src/agent/mod.rs`
- `crates/core/src/agent/tools/mod.rs`
- `crates/core/src/config/mod.rs`
- `crates/core/src/security/audit.rs`
- `crates/cli/src/tools.rs`

## Key files to create
- `crates/core/src/agent/tool_filters.rs`
- `crates/core/src/agent/hardcoded_filters.rs`
- `crates/core/src/agent/path_utils.rs`
