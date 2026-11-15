//! Génération via un serveur Ollama local (`/api/chat`, réponse en streaming).

use super::{Generator, Message};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};

pub struct OllamaGenerator {
    /// Ex. `http://localhost:11434`
    pub url: String,
    /// Ex. `ministral-3:8b`
    pub model: String,
    /// Bas = réponses factuelles et stables, ce qu'on veut pour un RAG.
    pub temperature: f32,
    /// Taille du contexte en tokens. Le défaut d'Ollama (souvent 2048–4096)
    /// tronquerait silencieusement le prompt quand il y a beaucoup d'extraits.
    pub num_ctx: u32,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    options: Options,
}

#[derive(Serialize)]
struct Options {
    temperature: f32,
    num_ctx: u32,
}

/// Une ligne du flux NDJSON renvoyé par Ollama.
#[derive(Deserialize)]
struct ChatChunk {
    message: Option<ChunkMessage>,
    #[serde(default)]
    done: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    content: String,
}

impl Generator for OllamaGenerator {
    fn id(&self) -> String {
        format!("ollama-{}", self.model)
    }

    fn generate(&self, messages: &[Message], on_token: &mut dyn FnMut(&str)) -> Result<String> {
        let response = ureq::post(format!("{}/api/chat", self.url))
            .send_json(ChatRequest {
                model: &self.model,
                messages,
                stream: true,
                options: Options { temperature: self.temperature, num_ctx: self.num_ctx },
            })
            .with_context(|| {
                format!(
                    "appel à Ollama impossible ({}). Ollama est-il lancé et le modèle « {} » installé (`ollama pull {}`) ?",
                    self.url, self.model, self.model
                )
            })?;

        let reader = BufReader::new(response.into_body().into_reader());
        let mut answer = String::new();

        for line in reader.lines() {
            let line = line.context("flux Ollama interrompu")?;
            if line.trim().is_empty() {
                continue;
            }
            let chunk: ChatChunk = serde_json::from_str(&line)
                .with_context(|| format!("réponse Ollama invalide : {line}"))?;

            if let Some(error) = chunk.error {
                bail!("erreur Ollama : {error}");
            }
            if let Some(message) = chunk.message {
                on_token(&message.content);
                answer.push_str(&message.content);
            }
            if chunk.done {
                break;
            }
        }
        Ok(answer)
    }
}
