use crate::error::Result;
use crate::indexer::db::{self, DbPool, ImportRow};

pub async fn get_imports(
    pool: &DbPool,
    project_root: &str,
    file_path: &str,
    direction: &str,
) -> Result<ImportAnalysis> {
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();

    match direction {
        "outgoing" | "both" => {
            outgoing = db::get_outgoing_imports(pool, project_root, file_path).await?;
        }
        _ => {}
    }

    match direction {
        "incoming" | "both" => {
            incoming = db::get_incoming_imports(pool, project_root, file_path).await?;
        }
        _ => {}
    }

    Ok(ImportAnalysis {
        outgoing,
        incoming,
    })
}

pub struct ImportAnalysis {
    pub outgoing: Vec<ImportRow>,
    pub incoming: Vec<ImportRow>,
}
