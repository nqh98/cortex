use clap::Parser;
use cortex::config::Config;
use cortex::indexer::Indexer;
use cortex::indexer::db;
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
        /// File path
        file: String,
        /// Symbol name
        symbol: String,
    },
    /// Start the MCP server
    Serve,
    /// Watch a project directory for changes
    Watch {
        /// Path to the project directory
        path: String,
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
    let config = Config::load(Path::new("config.toml"))?;

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
                stats.files_indexed,
                stats.symbols_found,
                stats.files_unchanged,
                stats.files_failed,
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
        Commands::Context { file, symbol } => {
            let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;
            let ctx = context::get_code_context(&pool, &file, &symbol).await?;
            println!("--- {} ({}) ---", ctx.symbol_name, ctx.kind);
            println!("File: {} lines {}-{}", ctx.file_path, ctx.start_line, ctx.end_line);
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
    }

    Ok(())
}
