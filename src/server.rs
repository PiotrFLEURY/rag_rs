//! Interface web de démo : une page HTML et une API qui renvoie la réponse en
//! streaming (Server-Sent Events).
//!
//! Seule couche asynchrone du projet : le pipeline RAG reste synchrone et
//! tourne dans `spawn_blocking`.

use crate::chunker::CharChunker;
use crate::embedder::Embedder;
use crate::generator::{Generator, OllamaGenerator};
use crate::prompt;
use crate::rag::Rag;
use crate::store::{JsonStore, SearchResult, VectorStore};
use anyhow::{Context, Result};
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct AppState {
    pub rag: Rag<CharChunker, Box<dyn Embedder + Send + Sync>, JsonStore>,
    pub generator: OllamaGenerator,
    pub default_k: usize,
    pub index_name: String,
}

pub async fn run(state: AppState, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(page))
        .route("/api/info", get(info))
        .route("/api/ask", post(ask))
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("port {port} indisponible (essayer --port)"))?;
    println!("Interface disponible sur http://localhost:{port}  (Ctrl+C pour arrêter)");
    axum::serve(listener, app).await?;
    Ok(())
}

/// La page est embarquée dans le binaire : rien à déployer à côté.
async fn page() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

#[derive(Serialize)]
struct Info {
    index: String,
    chunks: usize,
    books: Vec<String>,
    embedder: String,
    generator: String,
    default_k: usize,
}

async fn info(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(Info {
        index: state.index_name.clone(),
        chunks: state.rag.store.len(),
        books: state
            .rag
            .store
            .sources()
            .iter()
            .map(|s| file_stem(s))
            .collect(),
        embedder: state.rag.embedder.id(),
        generator: state.generator.id(),
        default_k: state.default_k,
    })
}

#[derive(Deserialize)]
struct AskRequest {
    question: String,
    k: Option<usize>,
}

#[derive(Serialize)]
struct SourceDto {
    n: usize,
    /// Titre du livre (nom de fichier sans extension)
    book: String,
    /// « page 12 », titre de chapitre… (absent pour un texte brut)
    section: Option<String>,
    citation: String,
    score: f32,
    text: String,
}

/// Flux SSE : un événement `sources`, puis des `token`, puis `done` (ou `error`).
async fn ask(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AskRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Event>();
    let k = req.k.unwrap_or(state.default_k).clamp(1, 20);

    tokio::task::spawn_blocking(move || {
        let send = |event: Event| {
            let _ = tx.send(event);
        };
        let result = state.rag.ask(
            &state.generator,
            req.question.trim(),
            k,
            &mut |sources: &[SearchResult]| {
                let dto: Vec<SourceDto> = sources
                    .iter()
                    .enumerate()
                    .map(|(i, r)| SourceDto {
                        n: i + 1,
                        book: file_stem(&r.chunk.source),
                        section: r.chunk.section.clone(),
                        citation: prompt::citation(r),
                        score: r.score,
                        text: r.chunk.text.clone(),
                    })
                    .collect();
                send(json_event("sources", &dto));
            },
            // Encodé en JSON : un token peut contenir des sauts de ligne.
            &mut |token| send(json_event("token", &token)),
        );
        match result {
            Ok(_) => send(Event::default().event("done").data("")),
            Err(e) => send(json_event("error", &format!("{e:#}"))),
        }
    });

    Sse::new(UnboundedReceiverStream::new(rx).map(Ok)).keep_alive(KeepAlive::default())
}

fn json_event(name: &str, data: &impl Serialize) -> Event {
    Event::default()
        .event(name)
        .json_data(data)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("\"sérialisation impossible\"")
        })
}

fn file_stem(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string())
}
