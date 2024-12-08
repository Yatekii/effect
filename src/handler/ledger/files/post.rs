use anyhow::{anyhow, Context};
use axum::{
    debug_handler,
    extract::{Multipart, Path, State},
    Json,
};
use itertools::Itertools;

use crate::{error::AppError, state::PortfolioAdapter};

use super::get::{LedgerFile, LedgerFiles};

#[debug_handler]
pub async fn handler(
    State(adapter): State<PortfolioAdapter>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<LedgerFiles>, AppError> {
    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap().to_string();
        let content = field.bytes().await.unwrap().into_iter().collect::<Vec<_>>();

        adapter
            .add_file(&id, &name, content)
            .with_context(|| anyhow!("{name} could not be added"))?;
    }

    let files = adapter.list_files()?;
    let Some(paths) = files.get(&id) else {
        return Err(anyhow!("{id} was not found"))?;
    };
    let files = paths
        .iter()
        .map(|path| {
            let entries = adapter.load_file(&id, path);
            LedgerFile {
                filename: path.display().to_string(),
                number_of_entries: entries.as_ref().ok().map(|e| e.len()),
                error: entries.err().map(|e| e.chain().join("\n")),
            }
        })
        .collect();

    Ok(Json(LedgerFiles { id, files }))
}