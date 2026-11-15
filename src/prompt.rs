//! Construction du prompt envoyé au LLM à partir des passages retrouvés.

use crate::generator::Message;
use crate::store::SearchResult;
use std::path::Path;

const SYSTEM_PROMPT: &str = "\
Tu es un assistant qui répond à des questions sur des livres.
Règles :
- Réponds uniquement à partir des extraits fournis, jamais de tes connaissances générales.
- Si les extraits ne permettent pas de répondre, dis-le clairement au lieu d'inventer.
- Cite tes sources avec leur numéro entre crochets, par exemple [2], après chaque affirmation.
- Réponds en français, de façon claire et concise.";

/// Système + message utilisateur contenant les extraits numérotés et la question.
pub fn build_messages(question: &str, results: &[SearchResult]) -> Vec<Message> {
    let mut context = String::new();
    for (i, r) in results.iter().enumerate() {
        context.push_str(&format!(
            "[{}] {}\n{}\n\n",
            i + 1,
            citation(r),
            r.chunk.text
        ));
    }

    vec![
        Message::system(SYSTEM_PROMPT),
        Message::user(format!("Extraits :\n\n{context}Question : {question}")),
    ]
}

/// « Frankenstein.pdf, page 12 » : nom de fichier sans le chemin, plus la section.
pub fn citation(r: &SearchResult) -> String {
    let name = Path::new(&r.chunk.source)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| r.chunk.source.clone());
    match &r.chunk.section {
        Some(section) => format!("{name}, {section}"),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Chunk;
    use crate::generator::Role;

    fn result(section: Option<&str>, text: &str) -> SearchResult {
        SearchResult {
            score: 0.5,
            chunk: Chunk {
                source: "samples/livre.pdf".into(),
                section: section.map(Into::into),
                index: 0,
                text: text.into(),
            },
        }
    }

    #[test]
    fn numerote_les_extraits_et_place_la_question_a_la_fin() {
        let messages = build_messages(
            "Que fait Félix ?",
            &[
                result(Some("page 3"), "Félix travaille."),
                result(None, "Autre passage."),
            ],
        );
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        let user = &messages[1].content;
        assert!(user.contains("[1] livre.pdf, page 3\nFélix travaille."));
        assert!(user.contains("[2] livre.pdf\nAutre passage."));
        assert!(user.ends_with("Question : Que fait Félix ?"));
    }
}
