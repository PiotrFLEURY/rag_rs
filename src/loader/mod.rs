//! Chargement des fichiers. Ajouter un format = implémenter `DocumentLoader`
//! et l'enregistrer dans `LoaderRegistry::default()`. Rien d'autre ne change.

mod epub;
mod pdf;
mod text;

pub use epub::EpubLoader;
pub use pdf::PdfLoader;
pub use text::TextLoader;

use crate::document::Document;
use anyhow::{Result, bail};
use std::path::Path;

/// `Send + Sync` : les loaders sont partagés entre les requêtes du serveur web.
pub trait DocumentLoader: Send + Sync {
    /// Extensions gérées, en minuscules et sans point (ex. `["txt", "md"]`).
    fn extensions(&self) -> &[&str];
    fn load(&self, path: &Path) -> Result<Document>;
}

/// Choisit le bon loader selon l'extension du fichier.
pub struct LoaderRegistry {
    loaders: Vec<Box<dyn DocumentLoader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self {
            loaders: Vec::new(),
        }
    }

    pub fn register(mut self, loader: impl DocumentLoader + 'static) -> Self {
        self.loaders.push(Box::new(loader));
        self
    }

    pub fn load(&self, path: &Path) -> Result<Document> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();

        match self
            .loaders
            .iter()
            .find(|l| l.extensions().contains(&ext.as_str()))
        {
            Some(loader) => loader.load(path),
            None => bail!(
                "format non supporté : .{ext} (formats gérés : {})",
                self.supported().join(", ")
            ),
        }
    }

    pub fn supported(&self) -> Vec<String> {
        self.loaders
            .iter()
            .flat_map(|l| l.extensions().iter().map(|e| format!(".{e}")))
            .collect()
    }
}

impl Default for LoaderRegistry {
    fn default() -> Self {
        Self::new()
            .register(TextLoader)
            .register(PdfLoader)
            .register(EpubLoader)
    }
}
