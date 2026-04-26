use clap::Parser;
use cortex::config::Config;
use cortex::indexer::db;
use cortex::indexer::Indexer;
use cortex::query::{context, search};
use std::path::Path;

#[derive(Parser)]
#[command(name = "cortex", version, about = "Local-first code context engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Index a project directory
    Index {
        /// Path to the project directory
        path: String,
    },
    /// Search for symbols
    Search {
        /// Search query (symbol name)
        query: String,
        /// Path to the project directory
        #[arg(short, long)]
        path: String,
        /// Filter by kind (function, struct, impl, trait, enum, constant, module, class, method)
        #[arg(short = 'k', long)]
        kind: Option<String>,
    },
    /// Get code context for a symbol
    Context {
        /// Symbol name
        symbol: String,
        /// Path to the project directory
        #[arg(short, long)]
        path: String,
        /// File path (optional, used to disambiguate)
        file: Option<String>,
    },
    /// Start the MCP server
    Serve,
    /// Watch a project directory for changes
    Watch {
        /// Path to the project directory
        path: String,
    },
    /// Clear the index for a project
    Reset {
        /// Path to a specific project to reset
        path: String,
    },
    /// List all indexed projects with stats
    List,
    /// Clean index by project name or all projects
    Clean {
        /// Project name (substring match) or "all" to remove all indexes
        name: String,
    },
    /// Update cortex to the latest version
    Update,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cortex=info".into()),
        )
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> cortex::error::Result<()> {
    let config_path = Config::default_config_path();

    // Ensure ~/.cortex/ exists
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let config = Config::load(&config_path)?;

    // Auto-save default config if it doesn't exist
    if !config_path.exists() {
        Config::default().save(&config_path)?;
    }

    match cli.command {
        Commands::Index { path } => {
            let project_path = Path::new(&path).canonicalize().map_err(|e| {
                cortex::error::CortexError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Directory not found: {path} ({e})"),
                ))
            })?;

            let indexer = Indexer::new(&config, &project_path).await?;
            let stats = indexer.index_project(&project_path).await?;

            println!(
                "Indexed {} files ({} symbols, {} unchanged, {} failed)",
                stats.files_indexed, stats.symbols_found, stats.files_unchanged, stats.files_failed,
            );
        }
        Commands::Search { query, path, kind } => {
            let project_path = Path::new(&path).canonicalize().map_err(|e| {
                cortex::error::CortexError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Directory not found: {path} ({e})"),
                ))
            })?;

            let db_path = cortex::config::project_db_path(&project_path);
            let db_str = format!("sqlite:{}", db_path.display());
            let pool = db::init_pool(&db_str).await?;
            let results = search::search_symbols(&pool, &query, kind.as_deref()).await?;

            if results.is_empty() {
                println!("No symbols matching '{query}'");
            } else {
                for row in &results {
                    let sig = row.signature.as_deref().unwrap_or("");
                    println!(
                        "{} {} ({}:{}-{}) {}",
                        row.kind, row.name, row.path, row.start_line, row.end_line, sig
                    );
                }
            }
        }
        Commands::Context { symbol, path, file } => {
            let project_path = Path::new(&path).canonicalize().map_err(|e| {
                cortex::error::CortexError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Directory not found: {path} ({e})"),
                ))
            })?;

            let db_path = cortex::config::project_db_path(&project_path);
            let db_str = format!("sqlite:{}", db_path.display());
            let pool = db::init_pool(&db_str).await?;
            let symbol_row = context::lookup_symbol(&pool, file.as_deref(), &symbol).await?;
            let abs_path = symbol_row.absolute_path();
            let file_content = std::fs::read_to_string(std::path::Path::new(&abs_path))
                .map_err(|_| cortex::error::CortexError::FileNotFound(abs_path))?;
            let ctx = context::extract_code(&symbol_row, &file_content);
            println!("--- {} ({}) ---", ctx.symbol_name, ctx.kind);
            println!(
                "File: {} lines {}-{}",
                ctx.file_path, ctx.start_line, ctx.end_line
            );
            if let Some(sig) = &ctx.signature {
                println!("Signature: {sig}");
            }
            println!();
            println!("{}", ctx.code);
        }
        Commands::Serve => {
            cortex::mcp_server::server::serve(config).await?;
        }
        Commands::Watch { path } => {
            let project_path = Path::new(&path).canonicalize().map_err(|e| {
                cortex::error::CortexError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Directory not found: {path} ({e})"),
                ))
            })?;

            // Initial index
            let indexer = Indexer::new(&config, &project_path).await?;
            indexer.index_project(&project_path).await?;

            // Watch for changes
            cortex::watcher::file_watcher::watch_project(&project_path, &config).await?;
        }
        Commands::Reset { path } => {
            let project_path = Path::new(&path).canonicalize().map_err(|e| {
                cortex::error::CortexError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Directory not found: {path} ({e})"),
                ))
            })?;
            let root = project_path.to_string_lossy().to_string();

            let db_path = cortex::config::project_db_path(&project_path);
            if db_path.exists() {
                std::fs::remove_file(&db_path).map_err(cortex::error::CortexError::Io)?;
                cortex::config::unregister_project(&root);
                println!("Cleared index for {}", root);
            } else {
                println!("No index found for {}", root);
            }
        }
        Commands::List => {
            let projects = cortex::config::load_registered_projects()?;

            if projects.is_empty() {
                println!("No indexed projects.");
                return Ok(());
            }

            println!("Indexed projects ({}):\n", projects.len());
            for p in &projects {
                let project_path = Path::new(p);
                let db_path = cortex::config::project_db_path(project_path);
                if !db_path.exists() {
                    println!("  {} (index missing)", p);
                    continue;
                }

                let db_str = format!("sqlite:{}", db_path.display());
                match db::init_pool(&db_str).await {
                    Ok(pool) => match db::get_project_stats(&pool, p).await {
                        Ok((file_count, symbol_count, last_indexed)) => {
                            let last = last_indexed.as_deref().unwrap_or("never");
                            println!("  {}", p);
                            println!(
                                "    Files: {}  Symbols: {}  Last indexed: {}",
                                file_count, symbol_count, last
                            );
                        }
                        Err(e) => {
                            println!("  {} (error: {})", p, e);
                        }
                    },
                    Err(e) => {
                        println!("  {} (db error: {})", p, e);
                    }
                }

                // Show DB file size
                if let Ok(meta) = std::fs::metadata(&db_path) {
                    println!("    DB size: {}", format_size(meta.len()));
                }
            }
        }
        Commands::Clean { name } => {
            let projects = cortex::config::load_registered_projects()?;

            if name == "all" {
                for p in &projects {
                    let project_path = Path::new(p);
                    let db_path = cortex::config::project_db_path(project_path);
                    if db_path.exists() {
                        let _ = std::fs::remove_file(&db_path);
                    }
                }
                // Clear registry
                let registry_path = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                    .join(".cortex")
                    .join("projects.json");
                let _ = std::fs::remove_file(registry_path);
                println!("Cleaned all indexes ({} projects)", projects.len());
            } else {
                let matches: Vec<_> = projects.iter().filter(|p| p.contains(&name)).collect();

                if matches.is_empty() {
                    println!("No indexed project matching '{}'", name);
                    return Ok(());
                }

                for p in &matches {
                    let project_path = Path::new(p);
                    let db_path = cortex::config::project_db_path(project_path);
                    if db_path.exists() {
                        let _ = std::fs::remove_file(&db_path);
                    }
                    cortex::config::unregister_project(p);
                    println!("Cleaned index for {}", p);
                }

                if matches.len() > 1 {
                    println!("Matched {} projects", matches.len());
                }
            }
        }
        Commands::Update => {
            cortex::update::perform_update()
                .await
                .map_err(cortex::error::CortexError::Update)?;
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
