//! Découpage du texte en chunks.

use crate::document::{Chunk, Document};

pub trait Chunker {
    fn chunk(&self, doc: &Document) -> Vec<Chunk>;
}

/// Fenêtres glissantes de `size` caractères avec `overlap` caractères de
/// recouvrement. Coupe de préférence sur un espace pour ne pas casser un mot.
pub struct CharChunker {
    pub size: usize,
    pub overlap: usize,
}

impl Default for CharChunker {
    fn default() -> Self {
        Self {
            size: 800,
            overlap: 150,
        }
    }
}

impl Chunker for CharChunker {
    fn chunk(&self, doc: &Document) -> Vec<Chunk> {
        assert!(self.overlap < self.size, "overlap doit être < size");
        let mut chunks = Vec::new();

        for section in &doc.sections {
            // Normalise les blancs (sauts de ligne, tabulations multiples…).
            let chars: Vec<char> = section
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .collect();

            let mut start = 0;
            while start < chars.len() {
                let mut end = (start + self.size).min(chars.len());
                // Recule jusqu'au dernier espace si on est au milieu d'un mot.
                let cut = chars[start..end]
                    .iter()
                    .rposition(|c| *c == ' ')
                    .filter(|&pos| end < chars.len() && pos > self.size / 2);
                if let Some(pos) = cut {
                    end = start + pos;
                }

                let text: String = chars[start..end]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    chunks.push(Chunk {
                        source: doc.source.clone(),
                        section: section.label.clone(),
                        index: chunks.len(),
                        text,
                        meta: doc.meta.clone(),
                    });
                }

                if end == chars.len() {
                    break;
                }
                let mut next = end.saturating_sub(self.overlap).max(start + 1);
                // Le recouvrement ne doit pas faire démarrer le chunk suivant
                // au milieu d'un mot : on avance jusqu'au début du mot suivant.
                let skip = chars[next..end]
                    .iter()
                    .position(|c| *c == ' ')
                    .filter(|_| chars[next - 1] != ' ');
                if let Some(pos) = skip {
                    next += pos + 1;
                }
                start = next;
            }
        }
        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{DocKind, Metadata, Section};

    #[test]
    fn splits_with_overlap() {
        let doc = Document {
            source: "t".into(),
            sections: vec![Section {
                label: None,
                text: "mot ".repeat(100),
            }],
            meta: Metadata::new(Some(DocKind::Course), &["dialogue".into()]),
        };
        let chunks = CharChunker {
            size: 50,
            overlap: 10,
        }
        .chunk(&doc);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.text.chars().count() <= 50));
        assert!(chunks.iter().all(|c| !c.text.contains("mo t")));
        // Aucun chunk ne commence ni ne finit au milieu d'un mot.
        assert!(
            chunks
                .iter()
                .all(|c| c.text.starts_with("mot") && c.text.ends_with("mot"))
        );
    }

    #[test]
    fn handles_multibyte_characters() {
        let doc = Document {
            source: "t".into(),
            sections: vec![Section {
                label: None,
                text: "éàç ".repeat(50),
            }],
            meta: Metadata::default(),
        };
        assert!(
            !CharChunker {
                size: 20,
                overlap: 5
            }
            .chunk(&doc)
            .is_empty()
        );
    }
}
