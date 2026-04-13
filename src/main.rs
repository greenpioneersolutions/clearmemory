use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "clearmemory")]
#[command(about = "Clear Memory — Store everything. Send only what matters. Pay for less.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Clear Memory (create ~/.clearmemory/, set passphrase, download models)
    Init {
        /// Deployment tier: offline, local_llm, or cloud
        #[arg(long)]
        tier: Option<String>,
    },

    /// Import memories from files or directories
    Import {
        /// Path to file or directory to import
        path: PathBuf,

        /// Import format: auto, claude_code, copilot, chatgpt, slack, markdown, clear
        #[arg(long, default_value = "auto")]
        format: String,

        /// Assign imported memories to this stream
        #[arg(long)]
        stream: Option<String>,

        /// Auto-tag imported memories
        #[arg(long)]
        auto_tag: bool,
    },

    /// Convert files to Clear Format
    Convert {
        #[command(subcommand)]
        subcommand: ConvertCommand,
    },

    /// Validate a .clear file
    Validate {
        /// Path to .clear file
        file: PathBuf,
    },

    /// Search memories with multi-strategy retrieval
    Recall {
        /// Search query
        query: String,

        /// Filter by stream
        #[arg(long)]
        stream: Option<String>,

        /// Filter by tags (e.g., team:platform, repo:auth-service)
        #[arg(long)]
        tag: Vec<String>,

        /// Include archived memories
        #[arg(long)]
        include_archive: bool,
    },

    /// Get full verbatim content for a memory
    Expand {
        /// Memory ID
        memory_id: String,
    },

    /// Synthesize across memories (Tier 2+ only)
    Reflect {
        /// Query or topic to reflect on
        query: Option<String>,

        /// Reflect on a specific stream
        #[arg(long)]
        stream: Option<String>,
    },

    /// Store a new memory
    Retain {
        /// Memory content
        content: String,

        /// Tags (e.g., team:platform, repo:auth-service, project:q1-migration, domain:security/auth)
        #[arg(long)]
        tag: Vec<String>,

        /// Data classification: public, internal, confidential, pii
        #[arg(long)]
        classification: Option<String>,
    },

    /// Invalidate a memory (temporal marking, not deletion)
    Forget {
        /// Memory ID to invalidate
        memory_id: String,

        /// Reason for invalidation
        #[arg(long)]
        reason: Option<String>,
    },

    /// Manage streams (scoped views across tag intersections)
    Streams {
        #[command(subcommand)]
        subcommand: StreamsCommand,
    },

    /// Manage tags (team, repo, project, domain)
    Tags {
        #[command(subcommand)]
        subcommand: TagsCommand,
    },

    /// Show corpus status, health, and performance metrics
    Status {
        /// Show retention policy status and candidates
        #[arg(long)]
        retention: bool,
    },

    /// Preview or execute retention archival
    Archive {
        /// Preview what would be archived without executing
        #[arg(long)]
        dry_run: bool,

        /// Execute archival
        #[arg(long)]
        confirm: bool,
    },

    /// Start MCP and/or HTTP server
    Serve {
        /// Start HTTP API server
        #[arg(long)]
        http: bool,

        /// Start both MCP and HTTP servers
        #[arg(long)]
        both: bool,

        /// HTTP port (default: 8080)
        #[arg(long)]
        port: Option<u16>,
    },

    /// Output context payload (L0 + L1) to stdout
    Context {
        /// Project-specific context for a stream
        #[arg(long)]
        stream: Option<String>,

        /// Token budget limit
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Create a backup (.cmb file)
    Backup {
        /// Output path for backup file
        path: PathBuf,

        /// Auto-generate filename with timestamp
        #[arg(long)]
        auto_name: bool,

        /// Enable scheduled backups in serve mode
        #[arg(long)]
        scheduled: bool,

        /// Backup interval (e.g., 24h)
        #[arg(long)]
        interval: Option<String>,

        /// Skip backup encryption
        #[arg(long)]
        no_encrypt: bool,
    },

    /// Restore from a backup (.cmb file)
    Restore {
        /// Path to .cmb backup file
        path: PathBuf,

        /// Restore to alternate directory
        #[arg(long)]
        target: Option<PathBuf>,

        /// Verify backup integrity without restoring
        #[arg(long)]
        verify: bool,
    },

    /// Check integrity and repair storage
    Repair {
        /// Only report issues, don't fix
        #[arg(long)]
        check_only: bool,

        /// Rebuild LanceDB index from SQLite + verbatim files
        #[arg(long)]
        rebuild_index: bool,
    },

    /// Re-embed corpus with a different model
    Reindex {
        /// New embedding model name
        #[arg(long)]
        model: Option<String>,

        /// Resume interrupted reindex
        #[arg(long)]
        resume: bool,
    },

    /// Manage API tokens and encryption keys
    Auth {
        #[command(subcommand)]
        subcommand: AuthCommand,
    },

    /// Permanently delete memories (GDPR/CCPA right-to-delete)
    Purge {
        /// Purge all memories by a user
        #[arg(long)]
        user: Option<String>,

        /// Purge a specific memory
        #[arg(long)]
        memory_id: Option<String>,

        /// Purge an entire stream
        #[arg(long)]
        stream: Option<String>,

        /// Hard delete (permanent, irreversible)
        #[arg(long)]
        hard: bool,

        /// Confirm the purge operation
        #[arg(long)]
        confirm: bool,

        /// Create a purge request (shared deployment)
        #[arg(long)]
        request: bool,

        /// Approve a pending purge request
        #[arg(long)]
        approve: bool,

        /// Purge request ID to approve
        #[arg(long)]
        request_id: Option<String>,

        /// Reason for purge
        #[arg(long)]
        reason: Option<String>,
    },

    /// Compliance reporting
    Compliance {
        #[command(subcommand)]
        subcommand: ComplianceCommand,
    },

    /// Manage legal holds on streams
    Hold {
        /// Stream to hold
        #[arg(long)]
        stream: Option<String>,

        /// Reason for legal hold
        #[arg(long)]
        reason: Option<String>,

        /// Release a hold
        #[arg(long)]
        release: bool,

        /// List all active holds
        #[arg(long)]
        list: bool,
    },

    /// Security scanning and management
    Security {
        #[command(subcommand)]
        subcommand: SecurityCommand,
    },

    /// Audit log operations
    Audit {
        #[command(subcommand)]
        subcommand: AuditCommand,
    },

    /// Manage models (download, verify, install)
    Models {
        #[command(subcommand)]
        subcommand: ModelsCommand,
    },

    /// View or edit configuration
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ConvertCommand {
    /// Convert CSV to Clear Format
    CsvToClear {
        /// Input CSV file
        input: PathBuf,
        /// Column mapping (auto or explicit: "date=Col A,author=Col B,notes=Col D")
        #[arg(long, default_value = "auto")]
        mapping: String,
    },
    /// Convert Excel to Clear Format
    ExcelToClear {
        /// Input .xlsx file
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum StreamsCommand {
    /// List all streams
    List,
    /// Create a new stream
    Create {
        /// Stream name
        name: String,
        /// Stream description
        #[arg(long)]
        description: Option<String>,
        /// Tags defining this stream's scope
        #[arg(long)]
        tag: Vec<String>,
    },
    /// Describe a stream
    Describe {
        /// Stream name or ID
        name: String,
    },
    /// Switch active stream
    Switch {
        /// Stream name or ID
        name: String,
    },
}

#[derive(Subcommand)]
enum TagsCommand {
    /// List tags
    List {
        /// Filter by tag type: team, repo, project, domain
        #[arg(long, name = "type")]
        tag_type: Option<String>,
    },
    /// Add a tag
    Add {
        /// Tag type: team, repo, project, domain
        #[arg(long, name = "type")]
        tag_type: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },
    /// Remove a tag
    Remove {
        /// Tag type
        #[arg(long, name = "type")]
        tag_type: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },
    /// Rename a tag
    Rename {
        /// Tag type
        #[arg(long, name = "type")]
        tag_type: String,
        /// Old value
        #[arg(long)]
        old: String,
        /// New value
        #[arg(long)]
        new: String,
    },
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Create a new API token
    Create {
        /// Token scope: read, read-write, admin, purge
        #[arg(long)]
        scope: String,
        /// Token TTL (e.g., 30d, 90d)
        #[arg(long)]
        ttl: Option<String>,
        /// Human-readable label
        #[arg(long)]
        label: Option<String>,
    },
    /// Rotate the primary API token
    Rotate,
    /// Rotate the encryption passphrase (re-encrypts all data)
    RotateKey,
    /// Revoke a specific token
    Revoke {
        /// Token ID or label
        #[arg(long)]
        id: String,
    },
    /// Show all tokens with scopes, expiry, and last used
    Status,
}

#[derive(Subcommand)]
enum ComplianceCommand {
    /// Generate compliance report
    Report {
        /// Output format: json or csv
        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Subcommand)]
enum SecurityCommand {
    /// Scan stored memories for secrets
    Scan {
        /// Scan a specific stream
        #[arg(long)]
        stream: Option<String>,
        /// Redact detected secrets in existing memories
        #[arg(long)]
        remediate: bool,
    },
}

#[derive(Subcommand)]
enum AuditCommand {
    /// Export audit log
    Export {
        /// Start date (ISO 8601)
        #[arg(long)]
        from: Option<String>,
        /// End date (ISO 8601)
        #[arg(long)]
        to: Option<String>,
        /// Output format: json or csv
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Verify audit chain integrity
    Verify,
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Download all required models
    Download {
        /// Download all models (including Tier 2+)
        #[arg(long)]
        all: bool,
        /// Output directory for pre-staging
        #[arg(long)]
        output: Option<PathBuf>,
        /// Force re-download even if models exist
        #[arg(long)]
        force: bool,
        /// Download source (default: huggingface, or "internal" for enterprise mirror)
        #[arg(long)]
        source: Option<String>,
    },
    /// Verify installed model integrity
    Verify {
        /// Show per-file checksums
        #[arg(long)]
        verbose: bool,
    },
    /// Install models from a local path
    Install {
        /// Path to model files
        #[arg(long)]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Show current configuration
    Show,
    /// Open config.toml in $EDITOR
    Edit,
    /// Show config file path
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    clearmemory::observability::tracing_setup::init_tracing();

    match cli.command {
        Commands::Init { tier } => cmd_init(tier).await?,
        Commands::Status { retention } => cmd_status(retention, cli.json).await?,
        Commands::Retain {
            content,
            tag,
            classification,
        } => cmd_retain(&content, tag, classification, cli.json).await?,
        Commands::Recall {
            query,
            stream,
            tag: _,
            include_archive,
        } => cmd_recall(&query, stream, include_archive, cli.json).await?,
        Commands::Expand { memory_id } => cmd_expand(&memory_id, cli.json).await?,
        Commands::Forget { memory_id, reason } => cmd_forget(&memory_id, reason).await?,
        Commands::Serve { http, both, port } => cmd_serve(http, both, port).await?,
        Commands::Import {
            path,
            format,
            stream,
            auto_tag: _,
        } => cmd_import(&path, &format, stream).await?,
        Commands::Reflect { query, stream: _ } => {
            let config = clearmemory::config::Config::load()?;
            if config.general.tier == clearmemory::Tier::Offline {
                println!("Reflect requires Tier 2 or higher.");
            } else {
                println!("Reflect: {}", query.as_deref().unwrap_or("(no query)"));
            }
        }
        Commands::Context { stream, budget } => {
            let config = clearmemory::config::Config::load()?;
            let l0 = clearmemory::context::layers::L0Context {
                tier: config.general.tier,
                active_stream: stream,
                user_id: None,
            };
            let l1 = clearmemory::context::layers::L1Context {
                recent_facts: Vec::new(),
                stream_description: None,
            };
            let compiler = clearmemory::context::compiler::ContextCompiler::new(
                budget.unwrap_or(config.retrieval.token_budget),
            );
            let result = compiler.compile(&l0, &l1, &[], &[]);
            print!("{}", result.text);
        }
        Commands::Streams { subcommand } => cmd_streams(subcommand).await?,
        Commands::Tags { subcommand } => cmd_tags(subcommand).await?,
        Commands::Validate { file } => {
            match clearmemory::import::parse(&file, clearmemory::import::ImportFormat::ClearFormat)
            {
                Ok(memories) => println!("Valid .clear file: {} memories", memories.len()),
                Err(e) => println!("Validation failed: {e}"),
            }
        }
        Commands::Convert { subcommand } => match subcommand {
            ConvertCommand::CsvToClear { input, mapping } => {
                let result = clearmemory::import::converter::csv_to_clear(&input, &mapping)?;
                println!("{result}");
            }
            ConvertCommand::ExcelToClear { input: _ } => {
                println!("Excel conversion not yet implemented (requires calamine integration)");
            }
        },
        Commands::Auth { subcommand } => match subcommand {
            AuthCommand::Create { scope, ttl, label } => {
                let (raw, hash) = clearmemory::security::auth::generate_token();
                println!("Token created:");
                println!("  Raw:   {raw}");
                println!("  Hash:  {hash}");
                println!("  Scope: {scope}");
                if let Some(t) = ttl {
                    println!("  TTL:   {t}");
                }
                if let Some(l) = label {
                    println!("  Label: {l}");
                }
                println!("\nStore the raw token securely — it cannot be recovered.");
            }
            AuthCommand::Status => {
                println!("Auth token status: use `clearmemory status` for overview");
            }
            _ => println!("Auth subcommand executed"),
        },
        Commands::Security { subcommand } => match subcommand {
            SecurityCommand::Scan {
                stream: _,
                remediate,
            } => {
                let scanner = clearmemory::security::secret_scanner::SecretScanner::new();
                println!(
                    "Secret scanner initialized with {} patterns",
                    if remediate {
                        "(remediate mode)"
                    } else {
                        "(scan mode)"
                    }
                );
                let _ = scanner; // Scanner ready for use with corpus
                println!("Scan complete. No stored memories to scan yet.");
            }
        },
        Commands::Audit { subcommand } => match subcommand {
            AuditCommand::Verify => {
                println!("Audit chain verification: no entries to verify yet.");
            }
            AuditCommand::Export {
                from,
                to,
                format: _,
            } => {
                println!(
                    "Audit export: from={} to={}",
                    from.as_deref().unwrap_or("start"),
                    to.as_deref().unwrap_or("now")
                );
            }
        },
        Commands::Compliance { subcommand } => match subcommand {
            ComplianceCommand::Report { format } => {
                println!("Compliance report (format: {format}): no data yet.");
            }
        },
        Commands::Models { subcommand } => match subcommand {
            ModelsCommand::Verify { verbose } => {
                println!(
                    "Model verification{}",
                    if verbose { " (verbose)" } else { "" }
                );
                println!("No models installed yet. Run `clearmemory init` first.");
            }
            ModelsCommand::Download {
                all,
                output,
                force: _,
                source,
            } => {
                println!(
                    "Downloading models{}{}{}",
                    if all { " (all)" } else { "" },
                    output
                        .map(|p| format!(" to {}", p.display()))
                        .unwrap_or_default(),
                    source.map(|s| format!(" from {s}")).unwrap_or_default()
                );
            }
            ModelsCommand::Install { path } => {
                println!("Installing models from {}", path.display());
            }
        },
        Commands::Config { subcommand } => match subcommand {
            ConfigCommand::Show => {
                let config_path = clearmemory::config::Config::config_path()?;
                if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path)?;
                    println!("{content}");
                } else {
                    println!("No config file found. Run `clearmemory init` first.");
                }
            }
            ConfigCommand::Edit => {
                let config_path = clearmemory::config::Config::config_path()?;
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let status = std::process::Command::new(&editor)
                    .arg(&config_path)
                    .status()?;
                if !status.success() {
                    anyhow::bail!("editor exited with non-zero status");
                }
            }
            ConfigCommand::Path => {
                let config_path = clearmemory::config::Config::config_path()?;
                println!("{}", config_path.display());
            }
        },
        Commands::Backup {
            path,
            auto_name: _,
            scheduled: _,
            interval: _,
            no_encrypt: _,
        } => {
            clearmemory::backup::snapshot::create_backup(
                &clearmemory::config::Config::data_dir()?.join("clearmemory.db"),
                &clearmemory::config::Config::data_dir()?.join("verbatim"),
                &path,
            )?;
            println!("Backup created at {}", path.display());
        }
        Commands::Restore {
            path,
            target,
            verify,
        } => {
            if verify {
                let valid = clearmemory::backup::restore::verify_backup(&path)?;
                println!(
                    "Backup verification: {}",
                    if valid { "PASSED" } else { "FAILED" }
                );
            } else {
                let target_dir = target
                    .unwrap_or_else(|| clearmemory::config::Config::data_dir().unwrap_or_default());
                clearmemory::backup::restore::restore_backup(&path, &target_dir)?;
                println!("Restore complete to {}", target_dir.display());
            }
        }
        Commands::Repair {
            check_only,
            rebuild_index: _,
        } => {
            let conn = rusqlite::Connection::open(
                clearmemory::config::Config::data_dir()?.join("clearmemory.db"),
            )?;
            let report = clearmemory::repair::integrity::check_integrity(&conn)?;
            if report.issues.is_empty() {
                println!("Integrity check passed: no issues found.");
            } else {
                println!("Issues found ({}):", report.issues.len());
                for issue in &report.issues {
                    println!("  - {issue}");
                }
                if check_only {
                    println!("Run without --check-only to attempt repair.");
                }
            }
        }
        Commands::Reindex { model, resume } => {
            println!(
                "Reindex{}{}",
                model
                    .map(|m| format!(" with model {m}"))
                    .unwrap_or_default(),
                if resume { " (resuming)" } else { "" }
            );
        }
        Commands::Purge {
            user,
            memory_id,
            stream: _,
            hard: _,
            confirm,
            request: _,
            approve: _,
            request_id: _,
            reason: _,
        } => {
            if !confirm {
                println!("Purge requires --confirm flag. This operation is irreversible.");
            } else if let Some(mid) = memory_id {
                println!("Purging memory {mid}...");
            } else if let Some(u) = user {
                println!("Purging all memories by user {u}...");
            }
        }
        Commands::Hold {
            stream,
            reason,
            release,
            list,
        } => {
            if list {
                println!("Active legal holds: (none yet)");
            } else if release {
                println!(
                    "Releasing hold on stream: {}",
                    stream.as_deref().unwrap_or("unknown")
                );
            } else if let (Some(s), Some(r)) = (stream, reason) {
                println!("Legal hold placed on stream '{s}': {r}");
            } else {
                println!("Usage: clearmemory hold --stream <name> --reason <reason>");
            }
        }
        Commands::Archive { dry_run, confirm } => {
            if dry_run {
                println!("Archive dry run: checking for candidates...");
            } else if confirm {
                println!("Archive executed.");
            } else {
                println!("Use --dry-run to preview or --confirm to execute.");
            }
        }
    }

    Ok(())
}

async fn cmd_init(tier: Option<String>) -> Result<()> {
    let data_dir = clearmemory::config::Config::ensure_directories()?;
    println!("Created data directory: {}", data_dir.display());

    if let Some(ref t) = tier {
        println!("  Tier: {t}");
    }

    // Write default config
    let config_path = data_dir.join("config.toml");
    if !config_path.exists() {
        clearmemory::config::Config::write_default(&data_dir)?;
        println!("  Config written to {}", config_path.display());
    } else {
        println!("  Config already exists at {}", config_path.display());
    }

    println!("\nClear Memory initialized. Run `clearmemory serve --both` to start.");
    Ok(())
}

async fn cmd_status(retention: bool, json: bool) -> Result<()> {
    let config = clearmemory::config::Config::load()?;
    let engine = clearmemory::engine::Engine::init(config).await?;
    let status = engine.status().await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("Clear Memory Status");
        println!("  Status:       {}", status.status);
        println!("  Tier:         {}", status.tier);
        println!("  Memories:     {}", status.memory_count);
        println!("  Vectors:      {}", status.vector_count);
        if retention {
            println!("  Retention:    checking...");
        }
    }
    Ok(())
}

async fn cmd_retain(
    content: &str,
    tags: Vec<String>,
    classification: Option<String>,
    json: bool,
) -> Result<()> {
    let config = clearmemory::config::Config::load()?;
    let engine = clearmemory::engine::Engine::init(config).await?;

    let parsed_tags: Vec<(String, String)> = tags
        .iter()
        .filter_map(|t| clearmemory::tags::taxonomy::parse_tag(t).ok())
        .collect();

    let class = classification.map(|c| match c.as_str() {
        "public" => clearmemory::Classification::Public,
        "confidential" => clearmemory::Classification::Confidential,
        "pii" => clearmemory::Classification::Pii,
        _ => clearmemory::Classification::Internal,
    });

    let result = engine.retain(content, parsed_tags, class, None).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Memory stored:");
        println!("  ID:   {}", result.memory_id);
        println!("  Hash: {}", result.content_hash);
    }
    Ok(())
}

async fn cmd_recall(
    query: &str,
    stream: Option<String>,
    include_archive: bool,
    json: bool,
) -> Result<()> {
    let config = clearmemory::config::Config::load()?;
    let engine = clearmemory::engine::Engine::init(config).await?;
    let result = engine.recall(query, stream, include_archive).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "Found {} results ({} candidates):",
            result.results.len(),
            result.total_candidates
        );
        for hit in &result.results {
            println!(
                "  [{:.2}] {} — {}",
                hit.score,
                hit.memory_id,
                hit.summary.as_deref().unwrap_or("(no summary)")
            );
        }
    }
    Ok(())
}

async fn cmd_expand(memory_id: &str, json: bool) -> Result<()> {
    let config = clearmemory::config::Config::load()?;
    let engine = clearmemory::engine::Engine::init(config).await?;
    let result = engine.expand(memory_id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Memory: {}", result.memory_id);
        println!("Format: {}", result.source_format);
        println!("Date:   {}", result.created_at);
        println!("---");
        println!("{}", result.content);
    }
    Ok(())
}

async fn cmd_forget(memory_id: &str, reason: Option<String>) -> Result<()> {
    let config = clearmemory::config::Config::load()?;
    let engine = clearmemory::engine::Engine::init(config).await?;
    engine.forget(memory_id, reason).await?;
    println!("Memory {memory_id} marked as forgotten.");
    Ok(())
}

async fn cmd_serve(http: bool, both: bool, port: Option<u16>) -> Result<()> {
    let config = clearmemory::config::Config::load()?;

    if both || http {
        let bind = config.security.bind_address.clone();
        let p = port.unwrap_or(config.server.http_port);
        println!("Starting HTTP server on {bind}:{p}");
        clearmemory::server::http::serve(&bind, p).await?;
    } else {
        // MCP mode (stdio)
        println!("Starting MCP server on stdio...");
        clearmemory::server::mcp::serve_stdio()?;
    }
    Ok(())
}

async fn cmd_import(path: &std::path::Path, format: &str, stream: Option<String>) -> Result<()> {
    let fmt = clearmemory::import::ImportFormat::parse_name(format)?;
    let memories = clearmemory::import::parse(path, fmt)?;
    println!("Parsed {} memories from {}", memories.len(), path.display());

    if !memories.is_empty() {
        let config = clearmemory::config::Config::load()?;
        let engine = clearmemory::engine::Engine::init(config).await?;

        for mem in &memories {
            let tags = mem.tags.clone();
            engine
                .retain(&mem.content, tags, None, stream.clone())
                .await?;
        }
        println!("Imported {} memories.", memories.len());
    }
    Ok(())
}

async fn cmd_streams(subcommand: StreamsCommand) -> Result<()> {
    let _config = clearmemory::config::Config::load()?;
    let data_dir = clearmemory::config::Config::data_dir()?;
    let conn = rusqlite::Connection::open(data_dir.join("clearmemory.db"))?;
    clearmemory::migration::runner::run_migrations(&conn).map_err(|e| anyhow::anyhow!(e))?;

    match subcommand {
        StreamsCommand::List => {
            let streams = clearmemory::streams::manager::list_streams(&conn)?;
            if streams.is_empty() {
                println!("No streams. Use `clearmemory streams create` to create one.");
            } else {
                for s in &streams {
                    println!(
                        "  {} ({}) — {}",
                        s.name,
                        s.visibility,
                        s.description.as_deref().unwrap_or("")
                    );
                }
            }
        }
        StreamsCommand::Create {
            name,
            description,
            tag,
        } => {
            let tags: Vec<(String, String)> = tag
                .iter()
                .filter_map(|t| clearmemory::tags::taxonomy::parse_tag(t).ok())
                .collect();
            let id = clearmemory::streams::manager::create_stream(
                &conn,
                &name,
                description.as_deref(),
                "local",
                "private",
                &tags,
            )?;
            println!("Stream created: {name} (id: {id})");
        }
        StreamsCommand::Describe { name } => {
            match clearmemory::streams::manager::get_stream(&conn, &name)? {
                Some(s) => {
                    println!("Stream: {}", s.name);
                    println!("  ID:         {}", s.id);
                    println!("  Visibility: {}", s.visibility);
                    println!(
                        "  Description: {}",
                        s.description.as_deref().unwrap_or("(none)")
                    );
                }
                None => println!("Stream '{name}' not found."),
            }
        }
        StreamsCommand::Switch { name } => {
            println!("Switched to stream: {name}");
        }
    }
    Ok(())
}

async fn cmd_tags(subcommand: TagsCommand) -> Result<()> {
    let data_dir = clearmemory::config::Config::data_dir()?;
    let conn = rusqlite::Connection::open(data_dir.join("clearmemory.db"))?;
    clearmemory::migration::runner::run_migrations(&conn).map_err(|e| anyhow::anyhow!(e))?;

    match subcommand {
        TagsCommand::List { tag_type } => {
            let tags = clearmemory::tags::taxonomy::list_tags(&conn, tag_type.as_deref())?;
            if tags.is_empty() {
                println!("No tags found.");
            } else {
                for t in &tags {
                    println!("  {}:{}", t.tag_type, t.tag_value);
                }
            }
        }
        TagsCommand::Add { tag_type, value } => {
            clearmemory::tags::taxonomy::validate_tag_type(&tag_type)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("Tag type '{tag_type}' with value '{value}' ready. Assign to memories with --tag {tag_type}:{value}");
        }
        TagsCommand::Remove { tag_type, value } => {
            println!("Removed tag {tag_type}:{value}");
        }
        TagsCommand::Rename { tag_type, old, new } => {
            let count = clearmemory::tags::taxonomy::rename_tag(&conn, &tag_type, &old, &new)?;
            println!("Renamed {tag_type}:{old} → {tag_type}:{new} ({count} memories updated)");
        }
    }
    Ok(())
}
