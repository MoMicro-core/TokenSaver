# TokenSaver

A Claude Code hook powered by a local LLM. Every prompt you send is automatically enriched with the relevant files, a structured task plan, and persistent project memory — before Claude ever sees it.

You keep using Claude Code exactly as before. TokenSaver runs invisibly in the background.

---

## Why It Saves Tokens

Most of a Claude Code session's token cost isn't your prompt — it's **Claude exploring your repo to find the relevant code.** When you ask it to *"fix the login redirect,"* Claude doesn't know where that lives, so it lists directories, greps for keywords, and reads whole files (often 5–10 of them) just to locate the right spot. Every one of those tool results stays in the context window and is re-processed on every following turn. That discovery phase routinely costs tens of thousands of tokens before a single line is edited.

TokenSaver removes that phase by doing the discovery **locally, for free:**

- A fast Rust scanner ranks candidate files, and a small **local** model (`qwen2.5-coder:0.5b` via Ollama) decides which ones actually matter — none of that touches Claude's token budget.
- The result is injected as a compact, budget-capped context block (`max_tokens`, default 8000) that hands Claude the answer instead of the search.
- Claude jumps straight to reading the 1–3 right files and spends its tokens **editing** instead of **searching.**

The win grows with repo size. It helps least on trivial prompts where Claude wouldn't have explored anyway.

---

## How It Works

```
You type: "fix login redirect after session expiry"
                        │
                        ▼
          Claude Code fires UserPromptSubmit hook
                        │
                        ▼
          tokensaver process
            │
            ├── 1. Fast scan — finds candidate files in your repo
            │
            ├── 2. Loads project memory
            │       memory.md    — architecture, conventions, constraints
            │       changelog.md — history of recent changes
            │       tasks.jsonl  — active and completed tasks
            │
            ├── 3. Local LLM (Qwen2.5-Coder via Ollama)
            │       decides which files are truly relevant,
            │       writes a clear task plan for Claude,
            │       updates memory with new facts,
            │       logs the task to changelog and tasks
            │
            └── 4. Injects structured context alongside your prompt
                        │
                        ▼
          Claude receives your original prompt +

          Task:
          Fix login redirect after an expired JWT session.

          Relevant Files:
          - src/auth/session.ts
          - src/middleware/auth.ts

          Relevant Symbols:
          - validateSession() [session.ts:34]
          - requireAuth() [auth.ts:12]

          Constraints (from project memory):
          - Authentication uses JWT, not cookies
          - Do not modify database schema automatically

          Instructions:
          Work only with the listed files first.
          Explain before editing any file not listed above.
          Avoid unrelated refactors.
```

Your prompt reaches Claude unchanged. The context rides next to it, invisibly.

---

## Requirements

- [Rust](https://rustup.rs) (to build from source)
- [Ollama](https://ollama.com) running locally

---

## Installation

```bash
# 1. Install Ollama
brew install ollama
ollama serve                        # keep this running in background

# 2. Pull ONE of the two recommended models — pick based on your RAM:
#
#    qwen2.5-coder:0.5b  ~400 MB on disk, ~1 GB RAM   — fast, good for ≤8 GB machines
#    qwen2.5-coder:1.5b  ~1 GB   on disk, ~2 GB RAM   — noticeably better decisions, recommended for 16 GB+
#
ollama pull qwen2.5-coder:0.5b      # default — change in config.toml if you pulled 1.5b instead

# 3. Build TokenSaver
git clone https://github.com/MoMicro-core/TokenSaver
cd TokenSaver
cargo install --path .

# 4. Check everything is connected
tokensaver llm-status
```

---

## Setup (per project)

Run once in any repo you want to enhance:

```bash
cd your-project
tokensaver init
```

This creates:
- `.tokensaver/config.toml` — configuration
- `.tokensaver/memory.md` — persistent project facts
- `.tokensaver/changelog.md` — history of tasks and changes
- `.tokensaver/tasks.jsonl` — task log
- `.claude/settings.json` — hooks Claude Code into TokenSaver automatically

Commit `.tokensaver/` to share memory across your team.

---

## Project Memory

The local LLM automatically updates memory as you work. You can also manage it manually:

```bash
# Add facts the LLM should always know
tokensaver remember "Backend uses FastAPI with JWT authentication"
tokensaver remember "Do not modify database schema without a migration file"
tokensaver remember "All API routes require the requireAuth() middleware"

# View all facts
tokensaver memory

# Remove a fact by ID
tokensaver forget abc123
```

---

## Changelog & Tasks

The LLM logs what it worked on after each prompt:

```bash
tokensaver changelog          # show recent task history
tokensaver tasks              # show active tasks
tokensaver tasks --all        # show all tasks including completed
```

---

## Configuration

`.tokensaver/config.toml` — every value is optional and falls back to the default shown. Run `tokensaver config` to print the effective configuration.

```toml
[llm]
enabled  = true
provider = "ollama"                # "ollama" for local | "openai" for any OpenAI-compatible API
model    = "qwen2.5-coder:0.5b"    # model name as the provider knows it
endpoint = "http://localhost:11434"
# api_key = ""                     # cloud only — or set TOKENSAVER_API_KEY / OPENAI_API_KEY env var
timeout_secs = 30                  # fall back to deterministic mode after this

[prompt]
max_tokens       = 8000  # token budget for the injected context
include_snippets = true  # include short code excerpts alongside file paths
snippet_lines    = 20    # max lines per file snippet

[analyzer]
max_files   = 20
max_symbols = 50
languages   = ["typescript", "javascript", "python", "rust", "go"]
exclude     = ["node_modules", "dist", "build", ".git", "target"]

[memory]
auto_inject = true
max_facts   = 100
```

**Switching local models:** install any model with `ollama pull <model>` then set it in `config.toml`. Good alternatives: `qwen2.5-coder:1.5b`, `llama3.2`, `phi3.5`.

**Using a cloud / OpenAI-compatible API instead of Ollama:** set `provider = "openai"`, point `endpoint` at the API (e.g. `https://api.openai.com` or `https://openrouter.ai/api`), pick a `model` (e.g. `gpt-4o-mini`), and provide a key via `TOKENSAVER_API_KEY` / `OPENAI_API_KEY` (preferred) or the `api_key` field. This trades the zero-cost local model for a remote one — useful if you want sharper file selection than a 0.5b can give.

---

## Debugging

```bash
# Check Ollama (or cloud provider) and model status
tokensaver llm-status

# Print the effective configuration (defaults + your config.toml)
tokensaver config

# See which files the fast scanner would pick
tokensaver analyze "fix login redirect"

# See the full additionalContext that would be injected
tokensaver context "fix login redirect"

# Show raw LLM output + what was filtered (essential when the LLM does something weird)
tokensaver debug "fix login redirect"

# Run a fixed set of test prompts to gauge LLM quality
tokensaver benchmark

# Simulate the full hook manually
echo '{
  "session_id": "test",
  "cwd": "/path/to/your/project",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "fix login redirect"
}' | tokensaver process

# Enable debug / trace logging (TOKENSAVER_LOG goes to stderr only — never breaks the hook)
TOKENSAVER_LOG=debug tokensaver context "fix login redirect"
TOKENSAVER_LOG=trace tokensaver debug   "fix login redirect"
```

### When the LLM gives strange results

Run `tokensaver debug "<your query>"`. It shows the raw output of both LLM calls,
which files were dropped because they were hallucinated, and which facts were
filtered out by sanitisation. If outputs are consistently poor on `0.5b` and you
have the RAM, switch to `1.5b` per the install step above.

---

## Fallback Behavior

If Ollama is not running or the model times out, TokenSaver automatically falls back to deterministic mode — keyword-based file selection, no LLM — and still injects useful context. Your Claude Code session is never blocked.

---

## Memory Files

| File | Purpose | Commit? |
|------|---------|---------|
| `memory.md` | Architecture, conventions, constraints — permanent facts | Yes |
| `changelog.md` | Append-only history of tasks and changes | Yes |
| `tasks.jsonl` | Active and completed task log | Yes |
| `config.toml` | Per-project configuration | Yes |

---

## Roadmap

- [x] Claude Code hook integration (`UserPromptSubmit` → `additionalContext`)
- [x] Persistent project memory (`memory.md`)
- [x] Local LLM via Ollama (Qwen2.5-Coder-0.5B default)
- [x] OpenAI-compatible cloud providers (OpenAI, OpenRouter, …)
- [x] LLM-structured task plan injected into every prompt
- [x] Automatic memory updates (remember / forget) after each prompt
- [x] Changelog and task tracking (`changelog.md`, `tasks.jsonl`)
- [x] Deterministic fallback when Ollama is unavailable
- [ ] Pre-built binaries (macOS, Linux, Windows)
- [ ] Homebrew formula

---

## License

MIT
