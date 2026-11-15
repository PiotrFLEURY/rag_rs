use super::DocumentLoader;
use crate::document::{Document, Section};
use anyhow::{Context, Result};
use std::path::Path;

/// PDF contenant du texte (une section par page).
///
/// Les PDF scannés (images sans couche texte) ressortent vides : il faudra un
/// OCR pour ceux-là.
pub struct PdfLoader;

impl DocumentLoader for PdfLoader {
    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn load(&self, path: &Path) -> Result<Document> {
        let pages = pdf_extract::extract_text_by_pages(path)
            .with_context(|| format!("extraction PDF impossible : {}", path.display()))?;

        let sections = pages
            .into_iter()
            .enumerate()
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(i, text)| Section {
                label: Some(format!("page {}", i + 1)),
                text,
            })
            .collect();

        Ok(Document {
            source: path.display().to_string(),
            sections,
            meta: Default::default(),
        })
    }
}
