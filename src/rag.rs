//! Assemble les briques. Ne dépend que des traits : changer de loader,
//! de chunker, d'embedder ou de store ne touche pas ce fichier.

use crate::chunker::Chunker;
use crate::embedder::Embedder;
use crate::generator::Generator;
use crate::loader::LoaderRegistry;
use crate::prompt;
use crate::store::{SearchResult, VectorStore};
use anyhow::{Result, bail};
use std::path::Path;

/// Nombre de chunks envoyés à l'embedder en un seul appel.
const BATCH_SIZE: usize = 32;

/// Réponse rédigée par le LLM, avec les passages qui lui ont servi de contexte.
pub struct Answer {
    pub text: String,
    pub sources: Vec<SearchResult>,
}

pub struct Rag<C, E, S> {
    pub loaders: LoaderRegistry,
    pub chunker: C,
    pub embedder: E,
    pub store: S,
}

impl<C: Chunker, E: Embedder, S: VectorStore> Rag<C, E, S> {
    /// Charge, découpe, vectorise et indexe un fichier. Renvoie le nb de chunks.
    pub fn ingest(&mut self, path: &Path) -> Result<usize> {
        let doc = self.loaders.load(path)?;
        let chunks = self.chunker.chunk(&doc);
        let n = chunks.len();

        for (i, batch) in chunks.chunks(BATCH_SIZE).enumerate() {
            let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();
            let vectors = self.embedder.embed_batch(&texts)?;
            if vectors.len() != batch.len() {
                bail!(
                    "l'embedder a renvoyé {} vecteurs pour {} textes",
                    vectors.len(),
                    batch.len()
                );
            }
            for (chunk, vector) in batch.iter().cloned().zip(vectors) {
                self.store.add(chunk, vector);
            }
            eprint!(
                "\r{}/{} chunks vectorisés",
                (i * BATCH_SIZE + batch.len()),
                n
            );
        }
        if n > 0 {
            eprintln!();
        }
        Ok(n)
    }

    pub fn retrieve(&self, question: &str, k: usize) -> Result<Vec<SearchResult>> {
        let q = self.embedder.embed(question)?;
        Ok(self.store.search(&q, k))
    }

    /// Retrouve les `k` passages les plus proches puis fait rédiger la réponse.
    /// `on_sources` reçoit les passages dès qu'ils sont trouvés, `on_token` chaque fragment de réponse.
    pub fn ask<G: Generator>(
        &self,
        generator: &G,
        question: &str,
        k: usize,
        on_sources: &mut dyn FnMut(&[SearchResult]),
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Answer> {
        let sources = self.retrieve(question, k)?;
        if sources.is_empty() {
            bail!("l'index est vide : ingère d'abord un fichier avec `ingest`");
        }
        // Les sources sont connues avant la génération : on peut les afficher tout de suite.
        on_sources(&sources);
        let messages = prompt::build_messages(question, &sources);
        let text = generator.generate(&messages, on_token)?;
        Ok(Answer { text, sources })
    }
}
