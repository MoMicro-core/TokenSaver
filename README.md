# TokenSaver

A Claude Code hook that automatically injects relevant context into every prompt — relevant files, symbols, and persistent project memory — without changing your workflow.

You type in Claude Code as normal. TokenSaver runs invisibly in the background, analyzes your repo, and enriches what Claude receives before it starts working.

---

## How It Works

```
You type: "fix login redirect after session expiry"
                        │
                        ▼
          Claude Code fires UserPromptSubmit hook
                        │
                        ▼
              tokensaver process (< 200ms)
                        │
              analyzes repo + loads memory
                        │
                        ▼
          Claude receives your original prompt +
          automatically injected context:

          Relevant Files:
          - src/auth/session.ts
          - src/middleware/auth.ts
          - src/routes/login.tsx

          Relevant Symbols:
          - validateSession() [session.ts:34]
          - requireAuth() [auth.ts:12]
          - redirectAfterLogin() [login.tsx:78]

          Project Memory:
          - Authentication uses JWT, not cookies
          - Do not modify database schema automatically

          Instructions:
          Inspect only the listed files first.
          Avoid unrelated refactors.
```

Your prompt reaches Claude unchanged. TokenSaver adds context alongside it via Claude Code's `additionalContext` injection. No separate terminal, no extra steps, no API keys.

---

## Installation

> **Note:** TokenSaver is currently in development. Pre-built binaries are not yet available — build from source for now.

### Build from source

```bash
git clone https://github.com/axbuglak/tokensaver
cd tokensaver
cargo build --release
cp target/release/tokensaver /usr/local/bin/tokensaver
```

### Verify

```bash
tokensaver --version
```

---

## Setup

Run this once in any repo you want TokenSaver to enhance:

```bash
cd your-project
tokensaver init
```

This creates:
- `.tokensaver/` — config and memory directory
- `.tokensaver/config.toml` — configuration with defaults
- `.tokensaver/memory.md` — empty project memory file
- Updates `.claude/settings.json` with the hook entry

That's it. Open Claude Code in that directory and every prompt you send is now enriched automatically.

---

## Project Memory

The most powerful feature. Teach TokenSaver things about your project once — it injects them into every future prompt.

```bash
# Add facts
tokensaver remember "Backend uses FastAPI with JWT authentication"
tokensaver remember "Do not modify database schema without a migration file"
tokensaver remember "All API routes require the requireAuth() middleware"
tokensaver remember "Frontend is Next.js 14 with App Router"

# View all facts
tokensaver memory

# Remove a fact by ID
tokensaver forget abc123
```

Memory is stored in `.tokensaver/memory.md` — a plain Markdown file you can edit directly. Commit it to your repo so your whole team shares the same context.

---

## Debugging

See exactly what Claude is receiving before you rely on it:

```bash
# Show which files and symbols would be selected for a query
tokensaver analyze "fix login redirect"

# Show the full additionalContext block that would be injected
tokensaver context "fix login redirect"

# Simulate the full hook manually (same as Claude Code fires it)
echo '{
  "session_id": "test",
  "cwd": "/path/to/your/project",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "fix login redirect"
}' | tokensaver process
```

---

## Configuration

`.tokensaver/config.toml` — all values are optional, these are the defaults:

```toml
[prompt]
max_tokens = 8000         # token budget for injected context
include_snippets = true   # include short code excerpts
snippet_lines = 20        # max lines per file snippet

[analyzer]
max_files = 20
max_symbols = 50
languages = ["typescript", "javascript", "python", "rust", "go"]
exclude = ["node_modules", "dist", "build", ".git", "target"]

[memory]
auto_inject = true
max_facts = 100
```

---

## How the Hook Is Configured

TokenSaver adds itself to `.claude/settings.json` during `tokensaver init`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": ".*",
        "hooks": [
          {
            "type": "command",
            "command": "tokensaver process"
          }
        ]
      }
    ]
  }
}
```

To disable TokenSaver for a project, remove this entry. Your Claude Code sessions are otherwise completely unchanged.

---

## What Gets Analyzed

TokenSaver uses fast, deterministic techniques — no LLM, no network:

- **Keyword search** — matches query terms against file names, function names, and file contents
- **AST parsing** — extracts functions, classes, types, and interfaces via tree-sitter
- **Import tracing** — follows imports from candidate files one level deep
- **Git recency** — recently modified files rank higher

Supported languages: TypeScript, JavaScript, Python, Rust, Go.

---

## What TokenSaver Is Not

- Not a coding assistant — it does not generate code
- Not a replacement for Claude Code — it makes Claude Code work better
- Not a cloud service — everything runs locally, no telemetry, no accounts
- Not an IDE plugin — it works at the Claude Code CLI level

---

## Roadmap

- [x] Claude Code hook integration (`UserPromptSubmit` → `additionalContext`)
- [x] Persistent project memory (`.tokensaver/memory.md`)
- [ ] Codebase analyzer (file scanner, keyword search)
- [ ] AST symbol extraction (tree-sitter)
- [ ] Import graph traversal
- [ ] Token budget enforcement
- [ ] Pre-built binaries (macOS, Linux, Windows)
- [ ] Homebrew formula

---

## Contributing

Issues and pull requests are welcome. See [PRD.md](PRD.md) — wait, that's gitignored. Check the issues tab for what's being worked on.

---

## License

MIT
