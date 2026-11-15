//! Modèle de données commun à tout le pipeline, indépendant du format source.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Un document chargé, quel que soit son format d'origine.
///
/// Un document est découpé en `sections` : une seule pour un fichier texte,
/// une par page pour un PDF, une par chapitre pour un EPUB… Cela permet de
/// garder la provenance (« page 12 », « chapitre 3 ») jusqu'aux réponses.
#[derive(Debug, Clone)]
pub struct Document {
    pub source: String,
    pub sections: Vec<Section>,
    /// Fixées à l'ingestion (les loaders ne les connaissent pas).
    pub meta: Metadata,
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
    /// Métadonnées du document d'origine, recopiées sur chaque chunk pour
    /// filtrer la recherche. `default` : les index antérieurs restent lisibles.
    #[serde(default)]
    pub meta: Metadata,
}

/// Rôle d'un document dans la base de connaissance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    /// Cours sur l'écriture : techniques, méthodes, conseils
    Course,
    /// Texte littéraire qui illustre des techniques (roman, nouvelle…)
    Example,
}

impl DocKind {
    /// Libellé français, pour les textes lus par un humain ou par le LLM.
    pub fn label(&self) -> &'static str {
        match self {
            DocKind::Course => "cours",
            DocKind::Example => "exemple",
        }
    }
}

/// Valeur technique (`course`, `example`) : celle de `--type`, de l'API et de l'index.
impl fmt::Display for DocKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DocKind::Course => "course",
            DocKind::Example => "example",
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// `None` : document ingéré sans `--type` (ou avant l'ajout des métadonnées).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocKind>,
    /// Étiquettes libres (genre, technique…), normalisées en minuscules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Metadata {
    pub fn new(kind: Option<DocKind>, tags: &[String]) -> Self {
        Self {
            kind,
            tags: normalize_tags(tags),
        }
    }
}

/// Critères de recherche. Vide = tout l'index.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub kind: Option<DocKind>,
    /// Le chunk doit porter **toutes** ces étiquettes.
    pub tags: Vec<String>,
}

impl Filter {
    pub fn new(kind: Option<DocKind>, tags: &[String]) -> Self {
        Self {
            kind,
            tags: normalize_tags(tags),
        }
    }

    pub fn matches(&self, meta: &Metadata) -> bool {
        self.kind.is_none_or(|k| meta.kind == Some(k))
            && self.tags.iter().all(|t| meta.tags.contains(t))
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_none() && self.tags.is_empty()
    }
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(kind) = self.kind {
            parts.push(format!("type = {kind}"));
        }
        if !self.tags.is_empty() {
            parts.push(format!("étiquettes = {}", self.tags.join(", ")));
        }
        if parts.is_empty() {
            f.write_str("aucun filtre")
        } else {
            f.write_str(&parts.join(", "))
        }
    }
}

/// Minuscules, sans espaces autour, sans doublon ni étiquette vide.
fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn filters_by_kind_and_tags() {
        let meta = Metadata::new(Some(DocKind::Example), &tags(&["Gothique", " dialogue "]));
        assert_eq!(meta.tags, tags(&["dialogue", "gothique"]));

        assert!(Filter::default().matches(&meta));
        assert!(Filter::new(Some(DocKind::Example), &[]).matches(&meta));
        assert!(!Filter::new(Some(DocKind::Course), &[]).matches(&meta));
        assert!(Filter::new(None, &tags(&["GOTHIQUE"])).matches(&meta));
        assert!(!Filter::new(None, &tags(&["gothique", "polar"])).matches(&meta));
        // Un document sans type ne passe pas un filtre sur le type.
        assert!(!Filter::new(Some(DocKind::Course), &[]).matches(&Metadata::default()));
    }

    #[test]
    fn reads_chunk_without_metadata() {
        let chunk: Chunk =
            serde_json::from_str(r#"{"source":"a.pdf","section":null,"index":0,"text":"x"}"#)
                .unwrap();
        assert_eq!(chunk.meta, Metadata::default());
    }
}
