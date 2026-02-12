---
sidebar_position: 16
---

# LocalGPT.md

## Your standing instructions to the AI — always present, near the end

`LocalGPT.md` is a plain-text Markdown file that lives in your workspace alongside `SOUL.md`, `MEMORY.md`, and `HEARTBEAT.md`. Whatever you write in this file will be injected near the end of every conversation turn — after all messages and tool outputs, just before a hardcoded security suffix that always occupies the final position.

This makes `LocalGPT.md` one of the most persistent and influential files in your workspace. It is your tenets, your ground rules, your standing orders, your guardrails, your conventions, and your reminders — all in one place, always in effect.

## What it does

Every time the AI is about to respond, LocalGPT assembles a context window from your conversation history, tool results, memory, and system instructions. The content of `LocalGPT.md` is injected near the end of this context — immediately before a hardcoded security suffix that always occupies the final position.

In large language models, **position matters**. Content near the end of the context window receives stronger attention weighting. By placing your instructions in this high-attention zone, `LocalGPT.md` acts as a persistent anchor — a constant reminder that doesn't get buried under conversation noise, even in long sessions.

Think of it as:

- The **house rules** posted at the door — everyone sees them, every time
- A **standing brief** handed to your team at the start of every meeting
- The **principles and conventions** your AI should always keep in mind
- A **behavioral anchor** that resists drift over long conversations
- Your **standing orders** that the AI sees near the end of every turn

## What to put in it

`LocalGPT.md` is intentionally open-ended. It is not limited to security rules or policy — it is for anything you want the AI to consistently remember and respect. Common uses include:

**Coding conventions and standards**
```markdown
## Conventions
- Use `snake_case` for all Rust identifiers
- Every public function must have a doc comment
- Never use `unwrap()` in production code — use `?` or explicit error handling
- Prefer `thiserror` for library errors, `anyhow` for application errors
```

**Security boundaries and access rules**
```markdown
## Boundaries
- Never read or write files outside the workspace directory
- Never execute commands that require network access
- Do not modify any file in the `contracts/` directory without explicit confirmation
- Treat all user-uploaded files as untrusted input
```

**Communication and workflow preferences**
```markdown
## How I work
- Explain your reasoning before showing code
- When uncertain, say so — don't guess
- If a task will take more than 3 steps, outline the plan first
- Always suggest tests for new functionality
```

**Project-specific constraints and compliance**
```markdown
## Project rules
- This codebase must remain compatible with Rust 1.75+
- All dependencies must use MIT, Apache-2.0, or BSD licenses only
- Database migrations must be reversible
- Log all state changes at INFO level
```

**Reminders the AI tends to forget in long sessions**
```markdown
## Reminders
- The `config.toml` schema changed in v2.0 — do not use the old format
- Our CI runs on ARM64, not x86 — test accordingly
- The `legacy/` module is frozen — route new features through `core/`
```

You can combine any of these. The file is yours. Write it however makes sense for your workflow.

## How it stays trustworthy

Because `LocalGPT.md` directly shapes AI behavior, it is protected by a cryptographic integrity system:

1. **You write or edit `LocalGPT.md`** in your editor of choice — it's a plain Markdown file
2. **You sign it** by running `localgpt md sign`, which creates a cryptographic fingerprint using a key stored on your device
3. **At every session start**, LocalGPT verifies the signature before injecting the file's content. If the file was modified without re-signing — by the AI, by a script, by anything other than you deliberately editing and re-signing — the content is silently excluded and a warning is shown

This means the AI **cannot modify its own instructions**. Your standing instructions remain yours.

The signing step is simple and takes less than a second:

```
$ localgpt md sign
✓ Signed LocalGPT.md (sha256: a1b2c3...)
```

If `LocalGPT.md` is not signed, LocalGPT still works — it simply runs without your custom instructions, using only its built-in defaults.

## Important: guidance, not guarantees

Large language models are probabilistic. `LocalGPT.md` provides **strong, persistent guidance** — not deterministic enforcement. The end-of-context positioning gives your instructions maximum influence, and the AI will follow them in the vast majority of interactions. But no prompt-based mechanism can guarantee 100% compliance in every edge case.

This is by design. `LocalGPT.md` is about shaping behavior over time, setting expectations, and keeping the AI aligned with how you work. For hard security boundaries that must never be crossed (like filesystem sandboxing or network isolation), LocalGPT enforces those at the system level, independent of any prompt.

Think of `LocalGPT.md` as a strong cultural norm — followed naturally and consistently, but backed by real enforcement mechanisms at the infrastructure layer where it matters most.

## Quick reference

| | |
|---|---|
| **Location** | `~/.local/share/localgpt/workspace/LocalGPT.md` |
| **Format** | Plain Markdown (UTF-8) |
| **Size limit** | 4,096 characters (~1,000 tokens) |
| **Injected** | Near end of every turn (before security suffix) |
| **Editable by AI** | No — write-protected and signature-verified |
| **Required** | No — LocalGPT works without it, using built-in defaults |
| **Sign after editing** | `localgpt md sign` |
| **Check status** | `localgpt md status` |
| **View audit log** | `localgpt md audit` |

## Getting started

Create or edit the file:

```
$ nano ~/.local/share/localgpt/workspace/LocalGPT.md
```

Write your instructions, then sign:

```
$ localgpt md sign
✓ Signed LocalGPT.md
```

That's it. Your instructions are now active for every conversation, every session, every turn — until you change them.
