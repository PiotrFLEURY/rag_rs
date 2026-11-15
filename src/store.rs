//! Stockage des vecteurs et recherche par similarité.

use crate::document::Chunk;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: f32,
    pub chunk: Chunk,
}

pub trait VectorStore {
    fn add(&mut self, chunk: Chunk, vector: Vec<f32>);
    fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult>;
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

    /// Sources (fichiers) présentes dans l'index, triées et sans doublon.
    pub fn sources(&self) -> Vec<String> {
        let mut sources: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.chunk.source.clone())
            .collect();
        sources.sort();
        sources.dedup();
        sources
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

    fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = self
            .entries
            .iter()
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
