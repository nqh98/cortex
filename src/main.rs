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
        /// Filter by kind (function, struct, impl, trait, enum, constant, module, class, method)
        #[arg(short, long)]
        kind: Option<String>,
    },
    /// Get code context for a symbol
    Context {
        /// Symbol name
        symbol: String,
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
    /// Clear the index (all projects, or a specific project)
    Reset {
        /// Path to a specific project to reset (omit to clear all)
        path: Option<String>,
    },
    /// List all indexed projects with stats
    List,
    /// Clean index by project name or all projects
    Clean {
        /// Project name (substring match) or "all" to remove all indexes
        name: String,
    },
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

            let indexer = Indexer::new(&config).await?;
            let stats = indexer.index_project(&project_path).await?;

            println!(
                "Indexed {} files ({} symbols, {} unchanged, {} failed)",
                stats.files_indexed, stats.symbols_found, stats.files_unchanged, stats.files_failed,
            );
        }
        Commands::Search { query, kind } => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;
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
        Commands::Context { symbol, file } => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;
            let ctx = context::get_code_context(&pool, file.as_deref(), &symbol).await?;
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
            let indexer = Indexer::new(&config).await?;
            indexer.index_project(&project_path).await?;

            // Watch for changes
            cortex::watcher::file_watcher::watch_project(&project_path, &config).await?;
        }
        Commands::Reset { path } => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;

            if let Some(p) = path {
                let project_path = Path::new(&p).canonicalize().map_err(|e| {
                    cortex::error::CortexError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Directory not found: {p} ({e})"),
                    ))
                })?;
                let root = project_path.to_string_lossy().to_string();
                let count = db::delete_project(&pool, &root).await?;
                println!("Cleared index for {} ({} files removed)", root, count);
            } else {
                let count = db::delete_all(&pool).await?;
                println!("Cleared entire index ({} files removed)", count);
            }
        }
        Commands::List => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;
            let projects = db::list_all_projects(&pool).await?;

            if projects.is_empty() {
                println!("No indexed projects.");
                return Ok(());
            }

            // Get DB file size
            let db_size = std::fs::metadata(&config.database.path)
                .map(|m| m.len())
                .unwrap_or(0);

            println!("Indexed projects ({}):\n", projects.len());
            for p in &projects {
                let last = p.last_indexed.as_deref().unwrap_or("never");
                println!("  {}", p.project_root);
                println!(
                    "    Files: {}  Symbols: {}  Last indexed: {}",
                    p.file_count, p.symbol_count, last
                );
            }

            println!("\nDatabase size: {}", format_size(db_size));
        }
        Commands::Clean { name } => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;

            if name == "all" {
                let projects = db::list_all_projects(&pool).await?;
                let count = db::delete_all(&pool).await?;
                println!(
                    "Cleaned all indexes ({} projects, {} files removed)",
                    projects.len(),
                    count
                );
            } else {
                let projects = db::list_all_projects(&pool).await?;
                let matches: Vec<_> = projects
                    .iter()
                    .filter(|p| p.project_root.contains(&name))
                    .collect();

                if matches.is_empty() {
                    println!("No indexed project matching '{}'", name);
                    return Ok(());
                }

                for p in &matches {
                    let count = db::delete_project(&pool, &p.project_root).await?;
                    println!(
                        "Cleaned index for {} ({} files removed)",
                        p.project_root, count
                    );
                }

                if matches.len() > 1 {
                    println!("Matched {} projects", matches.len());
                }
            }
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
