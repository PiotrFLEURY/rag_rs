//! Embeddings calculés localement par un serveur Ollama (`/api/embed`).

use super::{Embedder, normalize};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub struct OllamaEmbedder {
    /// Ex. `http://localhost:11434`
    pub url: String,
    /// Ex. `bge-m3` (à récupérer avec `ollama pull bge-m3`)
    pub model: String,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl Embedder for OllamaEmbedder {
    fn id(&self) -> String {
        format!("ollama-{}", self.model)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let resp: EmbedResponse = ureq::post(format!("{}/api/embed", self.url))
            .send_json(EmbedRequest {
                model: &self.model,
                input: texts,
            })
            .with_context(|| {
                format!(
                    "appel à Ollama impossible ({}). Ollama est-il lancé et le modèle « {} » installé (`ollama pull {}`) ?",
                    self.url, self.model, self.model
                )
            })?
            .body_mut()
            .read_json()
            .context("réponse Ollama invalide")?;

        if resp.embeddings.len() != texts.len() {
            bail!(
                "Ollama a renvoyé {} vecteurs pour {} textes",
                resp.embeddings.len(),
                texts.len()
            );
        }

        // Le store calcule le cosinus par produit scalaire : on normalise.
        Ok(resp
            .embeddings
            .into_iter()
            .map(|mut v| {
                normalize(&mut v);
                v
            })
            .collect())
    }
}
