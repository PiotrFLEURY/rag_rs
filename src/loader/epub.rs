use super::DocumentLoader;
use crate::document::{Document, Section};
use anyhow::{Context, Result};
use epub::doc::{EpubDoc, NavPoint};
use std::path::{Path, PathBuf};

/// Livres EPUB : une section par document de la spine (en général un chapitre),
/// libellée avec le titre de la table des matières quand il existe.
pub struct EpubLoader;

impl DocumentLoader for EpubLoader {
    fn extensions(&self) -> &[&str] {
        &["epub"]
    }

    fn load(&self, path: &Path) -> Result<Document> {
        let mut doc = EpubDoc::new(path)
            .with_context(|| format!("ouverture EPUB impossible : {}", path.display()))?;
        let toc = flatten_toc(&doc.toc);

        let mut sections = Vec::new();
        for n in 0..doc.get_num_chapters() {
            doc.set_current_chapter(n);
            let Some((html, _mime)) = doc.get_current_str() else {
                continue;
            };

            let text = html2text::config::plain()
                .string_from_read(html.as_bytes(), usize::MAX / 2)
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue; // couverture, page de garde vide…
            }

            let label = doc
                .get_current_path()
                .and_then(|p| toc_label(&toc, &p))
                .unwrap_or_else(|| format!("chapitre {}", n + 1));
            sections.push(Section {
                label: Some(label),
                text,
            });
        }

        Ok(Document {
            source: path.display().to_string(),
            sections,
            meta: Default::default(),
        })
    }
}

fn flatten_toc(points: &[NavPoint]) -> Vec<(PathBuf, String)> {
    points
        .iter()
        .flat_map(|p| {
            std::iter::once((p.content.clone(), p.label.trim().to_string()))
                .chain(flatten_toc(&p.children))
        })
        .collect()
}

/// Retrouve le titre d'un fichier de la spine dans la table des matières.
/// Les chemins de la TOC peuvent contenir une ancre (`chap1.xhtml#sec2`).
fn toc_label(toc: &[(PathBuf, String)], path: &Path) -> Option<String> {
    toc.iter()
        .find(|(content, _)| {
            let content = content.to_string_lossy();
            let content = content.split('#').next().unwrap_or_default();
            Path::new(content) == path || path.ends_with(content)
        })
        .map(|(_, label)| label.clone())
        .filter(|l| !l.is_empty())
}
