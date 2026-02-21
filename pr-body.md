## Summary

- Add tool filter infrastructure (`ToolFilter`/`CompiledToolFilter`) to core with deny/allow pattern matching, hardcoded defaults that config can extend but never remove, and `merge_hardcoded()` deduplication
- Add path scoping utilities (`resolve_real_path`, `check_path_allowed`) with symlink resolution to prevent traversal bypasses
- Apply hardcoded bash deny filters (sudo, pipe-to-shell, fork bomb, protected files) to CLI tools
- Enforce `security.allowed_directories` on all file tools with `PathDenied` audit logging
- Add glTF/GLB export tool to Gen mode

**Note:** xAI provider, Tavily/Perplexity search providers, hybrid native search, and SSRF hardcoded filters were removed from this PR as they already exist on main.

## What gets blocked (defaults, no config needed)

| Tool | Blocked | Why |
|------|---------|-----|
| `bash` | `sudo ...`, `curl \| sh`, `rm -rf /`, `chmod 777` | Hardcoded deny patterns |
| `read_file` | Outside `allowed_directories` (if configured) | Path scoping |
| `write_file` | Outside `allowed_directories` (if configured) | Path scoping |
| `edit_file` | Outside `allowed_directories` (if configured) | Path scoping |
| Symlink bypass | `ln -s /etc/passwd /tmp/x` then `read_file /tmp/x` | `resolve_real_path()` follows symlinks before checking |

## Config extension example

```toml
[security]
allowed_directories = ["/tmp", "~/projects"]

[tools.filters.bash]
deny_substrings = ["docker rm"]
deny_patterns = ["^npm\\s+publish"]
```

## New files

| File | Purpose |
|------|---------|
| `crates/core/src/agent/tool_filters.rs` | `ToolFilter`/`CompiledToolFilter` — deny/allow pattern engine (278 lines, 11 tests) |
| `crates/core/src/agent/path_utils.rs` | `resolve_real_path`, `check_path_allowed` — symlink-aware path validation (94 lines, 6 tests) |
| `crates/core/src/agent/hardcoded_filters.rs` | Bash deny substrings/patterns constants (56 lines, 4 tests) |

## Test plan

- [x] `cargo test --workspace` — 152 tests pass (21 new: 11 tool_filters, 6 path_utils, 4 hardcoded_filters)
- [x] `cargo clippy --workspace` — no errors
- [x] `cargo build --workspace` — clean
- [ ] Manual: set `allowed_directories = ["/tmp"]`, attempt file read outside `/tmp` → should deny
- [ ] Manual: `cargo run -- ask "what is 2+2"` smoke test
