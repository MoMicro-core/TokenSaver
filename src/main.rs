mod analyzer;
mod config;
mod context;
mod hook;
mod init;
mod llm;
mod memory;
mod tokens;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tokensaver", version, about = "Claude Code hook — local LLM context injection")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Called automatically by Claude Code on every prompt (reads stdin JSON, writes JSON to stdout)
    Process,

    /// Initialize TokenSaver in the current repository
    Init {
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Add a fact to project memory
    Remember { fact: String },

    /// Remove a memory fact by ID
    Forget { id: String },

    /// List all memory facts
    Memory,

    /// Show recent changelog entries
    Changelog {
        /// Number of entries to show (default: 10)
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// List tasks recorded by the local LLM
    Tasks {
        /// Show all tasks including completed ones
        #[arg(short, long)]
        all: bool,
    },

    /// Check whether Ollama is running and the configured model is available
    LlmStatus,

    /// Show which files and symbols would be selected for a query
    Analyze { query: String },

    /// Print the additionalContext block that would be injected for a query
    Context { query: String },

    /// Print the current configuration
    Config {
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Show raw LLM input/output for a query — use this when the LLM is making weird decisions
    Debug { query: String },

    /// Run a fixed set of test prompts to check LLM quality
    Benchmark,
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("TOKENSAVER_LOG"))
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Process                  => hook::run(),
        Command::Init { repo }            => commands::init(repo),
        Command::Remember { fact }        => commands::remember(&fact),
        Command::Forget { id }            => commands::forget(&id),
        Command::Memory                   => commands::list_memory(),
        Command::Changelog { limit }      => commands::changelog(limit),
        Command::Tasks { all }            => commands::tasks(all),
        Command::LlmStatus                => commands::llm_status(),
        Command::Analyze { query }        => commands::analyze(&query),
        Command::Context { query }        => commands::show_context(&query),
        Command::Config { repo }          => commands::print_config(repo),
        Command::Debug { query }          => commands::debug(&query),
        Command::Benchmark                => commands::benchmark(),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

mod commands {
    use anyhow::Result;
    use std::path::PathBuf;

    pub fn init(repo: Option<PathBuf>) -> Result<()> {
        let cwd = repo.unwrap_or(std::env::current_dir()?);
        crate::init::run(&cwd)
    }

    pub fn remember(fact: &str) -> Result<()> {
        crate::memory::store::append(&std::env::current_dir()?, fact)
    }

    pub fn forget(id: &str) -> Result<()> {
        crate::memory::store::remove(&std::env::current_dir()?, id)
    }

    pub fn list_memory() -> Result<()> {
        let facts = crate::memory::store::load(&std::env::current_dir()?)?;
        if facts.is_empty() {
            println!("no memory entries — add one with `tokensaver remember \"<fact>\"`");
        } else {
            for f in &facts {
                println!("[{}] ({}) {}", f.id, f.category, f.text);
            }
        }
        Ok(())
    }

    pub fn changelog(limit: usize) -> Result<()> {
        let output = crate::memory::changelog::recent(&std::env::current_dir()?, limit)?;
        if output.is_empty() {
            println!("no changelog entries yet");
        } else {
            println!("{output}");
        }
        Ok(())
    }

    pub fn tasks(show_all: bool) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let tasks = if show_all {
            crate::memory::tasks::load_all(&cwd)?
        } else {
            crate::memory::tasks::load_active(&cwd)?
        };

        if tasks.is_empty() {
            println!("no {} tasks", if show_all { "" } else { "active " });
        } else {
            for t in &tasks {
                let status = match t.status {
                    crate::memory::tasks::TaskStatus::Active    => "active   ",
                    crate::memory::tasks::TaskStatus::Completed => "completed",
                };
                println!("[{}] {} {}", t.id, status, t.description);
            }
        }
        Ok(())
    }

    pub fn llm_status() -> Result<()> {
        let config = crate::config::load(&std::env::current_dir()?)?;
        println!("{}", crate::llm::check_status(&config.llm));
        Ok(())
    }

    pub fn analyze(query: &str) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let config = crate::config::load(&cwd)?;
        let result = crate::analyzer::analyze(query, &cwd, &config)?;

        if result.files.is_empty() {
            println!("no relevant files found for: {query:?}");
            return Ok(());
        }

        println!("Relevant files ({}):", result.files.len());
        for f in &result.files {
            println!("  [{:.1}] {}", f.relevance_score, f.path.display());
        }
        if !result.symbols.is_empty() {
            println!("\nRelevant symbols ({}):", result.symbols.len());
            for s in &result.symbols {
                println!("  {}() [{}:{}]", s.name, s.file.display(), s.line);
            }
        }
        Ok(())
    }

    pub fn show_context(query: &str) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let config = crate::config::load(&cwd)?;
        let facts = crate::memory::store::load(&cwd)?;
        let candidates = crate::analyzer::analyze(query, &cwd, &config)?;
        let decision = crate::llm::decide(query, &candidates, &facts, &config.llm);
        let ctx = crate::context::build(&candidates, &decision, &facts, &config);

        if ctx.is_empty() {
            println!("(no context would be injected)");
        } else {
            println!("{ctx}");
        }
        Ok(())
    }

    pub fn print_config(repo: Option<PathBuf>) -> Result<()> {
        let cwd = repo.unwrap_or(std::env::current_dir()?);
        println!("{:#?}", crate::config::load(&cwd)?);
        Ok(())
    }

    pub fn debug(query: &str) -> Result<()> {
        let cwd        = std::env::current_dir()?;
        let config     = crate::config::load(&cwd)?;
        let facts      = crate::memory::store::load(&cwd)?;
        let candidates = crate::analyzer::analyze(query, &cwd, &config)?;

        println!("─────────────────────────────────────────────────────");
        println!(" DEBUG: \"{}\"", query);
        println!("─────────────────────────────────────────────────────\n");

        // Stage 1: deterministic analyzer
        println!("STAGE 1 — Deterministic analyzer");
        println!("  Candidates found: {}", candidates.files.len());
        for f in candidates.files.iter().take(10) {
            println!("    [{:.1}] {}", f.relevance_score, f.path.display());
        }

        if !config.llm.enabled {
            println!("\nSTAGE 2 — LLM disabled in config (using deterministic fallback).");
            return Ok(());
        }

        let trace = crate::llm::decide_with_trace(query, &candidates, &facts, &config.llm);

        println!("\nSTAGE 2 — LLM Call 1 (context selection)");
        println!("  Request type detected: {}", trace.request_type);
        println!("  Candidates passed in: {}", trace.candidates_in_prompt.len());
        println!("\n  Raw LLM output:");
        println!("    relevant_files: {:?}", trace.ctx_files_raw);
        println!("    task_plan:      \"{}\"", trace.ctx_plan_raw);
        println!("\n  After validation:");
        println!("    kept:    {:?}", trace.validated_files);
        let dropped: Vec<_> = trace.ctx_files_raw.iter()
            .filter(|f| !trace.validated_files.contains(f)).collect();
        if !dropped.is_empty() {
            println!("    DROPPED (hallucinated): {:?}", dropped);
        }

        println!("\nSTAGE 3 — LLM Call 2 (memory extraction)");
        println!("  Raw facts: {}", trace.raw_facts.len());
        for f in &trace.raw_facts {
            println!("    {} | {} | category={}", f.key, f.value, f.category);
        }
        println!("\n  After sanitize + cap to 2:");
        for f in &trace.sanitized_facts {
            println!("    ✓ {}", f.to_memory_string());
        }
        let filtered = trace.raw_facts.len().saturating_sub(trace.sanitized_facts.len());
        if filtered > 0 {
            println!("    ({} fact(s) filtered out)", filtered);
        }
        println!("\n  Changelog: \"{}\"", trace.changelog);

        Ok(())
    }

    pub fn benchmark() -> Result<()> {
        let cwd    = std::env::current_dir()?;
        let config = crate::config::load(&cwd)?;

        if !config.llm.enabled {
            println!("LLM disabled in config — nothing to benchmark.");
            return Ok(());
        }

        // Status check first
        println!("LLM status: {}\n", crate::llm::check_status(&config.llm));

        let prompts = [
            "fix the divide by zero bug",
            "combine these python files into one",
            "add a new health check endpoint",
            "refactor the auth middleware",
            "what does this function do",
        ];

        let facts      = crate::memory::store::load(&cwd)?;

        for (i, p) in prompts.iter().enumerate() {
            println!("─── Test {} / {}: {:?}", i + 1, prompts.len(), p);
            let candidates = crate::analyzer::analyze(p, &cwd, &config)?;
            let start      = std::time::Instant::now();
            let decision   = crate::llm::decide(p, &candidates, &facts, &config.llm);
            let ms         = start.elapsed().as_millis();

            match decision {
                None => println!("  ✗ LLM call failed or returned no usable output"),
                Some(d) => {
                    println!("  ✓ {} ms", ms);
                    println!("    plan:  {}", d.task_plan);
                    println!("    files: {:?}", d.relevant_files);
                    if !d.new_facts.is_empty() {
                        let strs: Vec<String> = d.new_facts.iter().map(|f| f.to_memory_string()).collect();
                        println!("    facts: {:?}", strs);
                    }
                }
            }
            println!();
        }

        Ok(())
    }
}
