//! Modèle de données commun à tout le pipeline, indépendant du format source.

use serde::{Deserialize, Serialize};

/// Un document chargé, quel que soit son format d'origine.
///
/// Un document est découpé en `sections` : une seule pour un fichier texte,
/// une par page pour un PDF, une par chapitre pour un EPUB… Cela permet de
/// garder la provenance (« page 12 », « chapitre 3 ») jusqu'aux réponses.
#[derive(Debug, Clone)]
pub struct Document {
    pub source: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Section {
    /// Libellé lisible : "page 12", "chapitre 3"… `None` pour un texte brut.
    pub label: Option<String>,
    pub text: String,
}

/// Un morceau de texte prêt à être vectorisé et indexé.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub source: String,
    pub section: Option<String>,
    /// Position du chunk dans le document (0, 1, 2…).
    pub index: usize,
    pub text: String,
}
