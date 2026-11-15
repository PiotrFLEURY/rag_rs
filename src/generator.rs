//! Génération de texte par un LLM de chat.

mod ollama;

pub use ollama::OllamaGenerator;

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    // `Assistant` viendra avec les conversations multi-tours.
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
}

pub trait Generator {
    fn id(&self) -> String;

    /// Génère la réponse à une conversation. `on_token` est appelé à chaque
    /// fragment reçu (affichage en continu) ; la réponse complète est renvoyée.
    fn generate(&self, messages: &[Message], on_token: &mut dyn FnMut(&str)) -> Result<String>;
}
