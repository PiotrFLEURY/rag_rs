//! Stockage des vecteurs et recherche par similarité.

use crate::document::{Chunk, Filter, Metadata};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: f32,
    pub chunk: Chunk,
}

/// Un document de l'index, pour l'inventaire (`list`, interface web).
#[derive(Debug, Clone, Serialize)]
pub struct IndexedDocument {
    pub source: String,
    pub meta: Metadata,
    pub chunks: usize,
}

pub trait VectorStore {
    fn add(&mut self, chunk: Chunk, vector: Vec<f32>);
    /// Les `k` chunks les plus proches parmi ceux qui satisfont `filter`.
    fn search(&self, query: &[f32], k: usize, filter: &Filter) -> Vec<SearchResult>;
    fn len(&self) -> usize;
}

#[derive(Serialize, Deserialize)]
struct Entry {
    chunk: Chunk,
    vector: Vec<f32>,
}

/// Index en mémoire, recherche exhaustive (cosinus), persisté en JSON.
/// Largement suffisant jusqu'à quelques dizaines de milliers de chunks ;
/// au-delà, une implémentation Qdrant/LanceDB du même trait prendra le relais.
#[derive(Serialize, Deserialize)]
pub struct JsonStore {
    embedder_id: String,
    entries: Vec<Entry>,
}

impl JsonStore {
    pub fn new(embedder_id: impl Into<String>) -> Self {
        Self {
            embedder_id: embedder_id.into(),
            entries: Vec::new(),
        }
    }

    /// Ouvre l'index s'il existe, sinon en crée un vide.
    pub fn open_or_create(path: &Path, embedder_id: &str) -> Result<Self> {
        if path.exists() {
            Self::open(path, embedder_id)
        } else {
            Ok(Self::new(embedder_id))
        }
    }

    pub fn open(path: &Path, embedder_id: &str) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("index introuvable : {}", path.display()))?;
        let store: Self = serde_json::from_str(&raw).context("index JSON invalide")?;
        if store.embedder_id != embedder_id {
            bail!(
                "index construit avec l'embedder « {} », incompatible avec « {} »",
                store.embedder_id,
                embedder_id
            );
        }
        Ok(store)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string(self)?)
            .with_context(|| format!("écriture impossible de {}", path.display()))
    }

    /// Documents présents dans l'index, triés par source.
    pub fn documents(&self) -> Vec<IndexedDocument> {
        let mut docs: Vec<IndexedDocument> = Vec::new();
        for e in &self.entries {
            match docs.iter_mut().find(|d| d.source == e.chunk.source) {
                Some(d) => d.chunks += 1,
                None => docs.push(IndexedDocument {
                    source: e.chunk.source.clone(),
                    meta: e.chunk.meta.clone(),
                    chunks: 1,
                }),
            }
        }
        docs.sort_by(|a, b| a.source.cmp(&b.source));
        docs
    }

    /// Retire les chunks d'une source (pour réingérer un fichier sans doublons).
    pub fn remove_source(&mut self, source: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.chunk.source != source);
        before - self.entries.len()
    }
}

impl VectorStore for JsonStore {
    fn add(&mut self, chunk: Chunk, vector: Vec<f32>) {
        self.entries.push(Entry { chunk, vector });
    }

    fn search(&self, query: &[f32], k: usize, filter: &Filter) -> Vec<SearchResult> {
        // Filtre appliqué avant le classement : on obtient bien k résultats
        // parmi les documents demandés, pas k résultats dont certains filtrés.
        let mut results: Vec<SearchResult> = self
            .entries
            .iter()
            .filter(|e| filter.matches(&e.chunk.meta))
            .map(|e| SearchResult {
                score: dot(query, &e.vector),
                chunk: e.chunk.clone(),
            })
            .collect();
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(k);
        results
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Vecteurs normalisés : produit scalaire = similarité cosinus.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocKind;

    fn chunk(source: &str, kind: DocKind) -> Chunk {
        Chunk {
            source: source.into(),
            section: None,
            index: 0,
            text: source.into(),
            meta: Metadata::new(Some(kind), &[]),
        }
    }

    #[test]
    fn filtered_search_and_inventory() {
        let mut store = JsonStore::new("test");
        store.add(chunk("roman.pdf", DocKind::Example), vec![1.0, 0.0]);
        store.add(chunk("cours.pdf", DocKind::Course), vec![0.8, 0.6]);
        store.add(chunk("cours.pdf", DocKind::Course), vec![0.0, 1.0]);

        // Sans filtre, le roman est le plus proche ; filtré sur les cours, il disparaît.
        let all = store.search(&[1.0, 0.0], 1, &Filter::default());
        assert_eq!(all[0].chunk.source, "roman.pdf");
        let courses = store.search(&[1.0, 0.0], 5, &Filter::new(Some(DocKind::Course), &[]));
        assert_eq!(courses.len(), 2);
        assert!(courses.iter().all(|r| r.chunk.source == "cours.pdf"));

        let docs = store.documents();
        assert_eq!(docs.len(), 2);
        assert_eq!((docs[0].source.as_str(), docs[0].chunks), ("cours.pdf", 2));
    }
}
