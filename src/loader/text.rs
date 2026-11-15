use super::DocumentLoader;
use crate::document::{Document, Section};
use anyhow::{Context, Result};
use std::path::Path;

/// Fichiers texte brut (UTF-8).
pub struct TextLoader;

impl DocumentLoader for TextLoader {
    fn extensions(&self) -> &[&str] {
        &["txt", "md"]
    }

    fn load(&self, path: &Path) -> Result<Document> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("lecture impossible de {}", path.display()))?;
        Ok(Document {
            source: path.display().to_string(),
            sections: vec![Section { label: None, text }],
        })
    }
}
