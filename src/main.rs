mod chunker;
mod document;
mod embedder;
mod generator;
mod loader;
mod prompt;
mod rag;
mod server;
mod store;

use anyhow::Result;
use chunker::CharChunker;
use clap::{Parser, Subcommand};
use embedder::{Embedder, HashingEmbedder, OllamaEmbedder};
use generator::{Generator, OllamaGenerator};
use loader::LoaderRegistry;
use rag::Rag;
use std::io::Write;
use std::path::PathBuf;
use store::{JsonStore, VectorStore};

#[derive(Parser)]
#[command(
    about = "RAG minimal : ingestion de livres, recherche de passages et réponses rédigées par un LLM"
)]
struct Cli {
    /// Fichier d'index
    #[arg(long, global = true, default_value = "index.json")]
    index: PathBuf,
    /// Embedder : `ollama` (sémantique, local) ou `hashing` (lexical, sans dépendance)
    #[arg(long, global = true, default_value = "ollama")]
    embedder: String,
    /// Modèle d'embeddings Ollama
    #[arg(long, global = true, default_value = "bge-m3")]
    model: String,
    /// URL du serveur Ollama
    #[arg(long, global = true, default_value = "http://localhost:11434")]
    ollama_url: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Ingère un fichier dans l'index (le remplace s'il y était déjà)
    Ingest {
        file: PathBuf,
        #[arg(long, default_value_t = 800)]
        chunk_size: usize,
        #[arg(long, default_value_t = 150)]
        overlap: usize,
    },
    /// Cherche les passages les plus pertinents pour une question
    Query {
        question: String,
        #[arg(short, default_value_t = 3)]
        k: usize,
    },
    /// Pose une question : le LLM rédige une réponse à partir des passages retrouvés
    Ask {
        question: String,
        /// Nombre de passages donnés au LLM comme contexte
        #[arg(short, default_value_t = 5)]
        k: usize,
        /// Modèle de chat Ollama (choisir selon la mémoire : voir le README)
        #[arg(long, default_value = "ministral-3:8b")]
        chat_model: String,
        /// Température (0 = déterministe ; garder bas pour rester fidèle au texte)
        #[arg(long, default_value_t = 0.2)]
        temperature: f32,
        /// Taille du contexte du LLM, en tokens
        #[arg(long, default_value_t = 4096)]
        num_ctx: u32,
        /// Affiche aussi le texte complet des extraits utilisés
        #[arg(long)]
        show_context: bool,
    },
    /// Lance l'interface web de démo (questions/réponses dans le navigateur)
    Serve {
        #[arg(long, default_value_t = 3000)]
        port: u16,
        /// Nombre de passages donnés au LLM (modifiable dans l'interface)
        #[arg(short, default_value_t = 5)]
        k: usize,
        /// Modèle de chat Ollama
        #[arg(long, default_value = "ministral-3:8b")]
        chat_model: String,
        #[arg(long, default_value_t = 0.2)]
        temperature: f32,
        #[arg(long, default_value_t = 4096)]
        num_ctx: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // `Send + Sync` : l'embedder est partagé entre les requêtes du serveur web.
    let embedder: Box<dyn Embedder + Send + Sync> = match cli.embedder.as_str() {
        "hashing" => Box::new(HashingEmbedder::default()),
        "ollama" => Box::new(OllamaEmbedder {
            url: cli.ollama_url.trim_end_matches('/').to_string(),
            model: cli.model.clone(),
        }),
        other => anyhow::bail!("embedder inconnu : {other} (choix : ollama, hashing)"),
    };

    match cli.command {
        Command::Ingest {
            file,
            chunk_size,
            overlap,
        } => {
            let mut store = JsonStore::open_or_create(&cli.index, &embedder.id())?;
            let removed = store.remove_source(&file.display().to_string());
            let mut rag = Rag {
                loaders: LoaderRegistry::default(),
                chunker: CharChunker {
                    size: chunk_size,
                    overlap,
                },
                embedder,
                store,
            };
            let n = rag.ingest(&file)?;
            rag.store.save(&cli.index)?;
            if removed > 0 {
                println!("{removed} anciens chunks remplacés");
            }
            println!(
                "{n} chunks ingérés depuis {} → {} ({} chunks au total)",
                file.display(),
                cli.index.display(),
                rag.store.len()
            );
        }
        Command::Query { question, k } => {
            let store = JsonStore::open(&cli.index, &embedder.id())?;
            let rag = Rag {
                loaders: LoaderRegistry::default(),
                chunker: CharChunker::default(),
                embedder,
                store,
            };
            let started = std::time::Instant::now();
            let results = rag.retrieve(&question, k)?;
            eprintln!(
                "Question vectorisée par « {} » ({} chunks dans l'index) en {} ms\n",
                rag.embedder.id(),
                rag.store.len(),
                started.elapsed().as_millis()
            );
            for (i, r) in results.iter().enumerate() {
                let section = r
                    .chunk
                    .section
                    .as_deref()
                    .map(|s| format!(", {s}"))
                    .unwrap_or_default();
                println!(
                    "#{} score {:.3} — {}{} [chunk {}]\n{}\n",
                    i + 1,
                    r.score,
                    r.chunk.source,
                    section,
                    r.chunk.index,
                    r.chunk.text
                );
            }
        }
        Command::Ask {
            question,
            k,
            chat_model,
            temperature,
            num_ctx,
            show_context,
        } => {
            let store = JsonStore::open(&cli.index, &embedder.id())?;
            let rag = Rag {
                loaders: LoaderRegistry::default(),
                chunker: CharChunker::default(),
                embedder,
                store,
            };
            let generator = OllamaGenerator {
                url: cli.ollama_url.trim_end_matches('/').to_string(),
                model: chat_model,
                temperature,
                num_ctx,
            };
            eprintln!(
                "Recherche avec « {} », réponse rédigée par « {} »…\n",
                rag.embedder.id(),
                generator.id()
            );

            let mut stdout = std::io::stdout();
            let answer = rag.ask(&generator, &question, k, &mut |_| {}, &mut |token| {
                print!("{token}");
                let _ = stdout.flush();
            })?;

            if answer.text.trim().is_empty() {
                eprintln!("\n(le modèle n'a renvoyé aucun texte)");
            }
            println!("\n\nSources :");
            for (i, r) in answer.sources.iter().enumerate() {
                println!("[{}] {} (score {:.3})", i + 1, prompt::citation(r), r.score);
                if show_context {
                    println!("    {}\n", r.chunk.text);
                }
            }
        }
        Command::Serve {
            port,
            k,
            chat_model,
            temperature,
            num_ctx,
        } => {
            let store = JsonStore::open(&cli.index, &embedder.id())?;
            let state = server::AppState {
                index_name: cli.index.display().to_string(),
                default_k: k,
                generator: OllamaGenerator {
                    url: cli.ollama_url.trim_end_matches('/').to_string(),
                    model: chat_model,
                    temperature,
                    num_ctx,
                },
                rag: Rag {
                    loaders: LoaderRegistry::default(),
                    chunker: CharChunker::default(),
                    embedder,
                    store,
                },
            };
            // Le reste du programme est synchrone : le runtime async n'existe que pour le serveur.
            tokio::runtime::Runtime::new()?.block_on(server::run(state, port))?;
        }
    }
    Ok(())
}
