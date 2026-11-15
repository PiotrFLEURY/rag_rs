# CLAUDE.md

Contexte et règles de travail pour ce dépôt. Le README décrit l'usage ; ce fichier dit comment travailler dessus.

## Le projet

### Vision : un assistant d'écriture

Le but final est un **assistant IA pour élèves écrivains** d'une plateforme de formation. À partir de ce que l'élève cherche à écrire (histoire, personnages, univers, scène visée…), il :
1. **écrit des paragraphes entiers d'exemple**, adaptés au projet de l'élève ;
2. **explique pourquoi et comment** ce résultat est obtenu (techniques d'écriture employées, choix de narration, rythme…), pour que l'élève apprenne à le faire lui-même.

Architecture visée :
- **Un agent** connecté à un LLM **Mistral**. Il comprend la demande de l'élève, va chercher ce dont il a besoin dans la base de connaissance, rédige, puis explique.
- **Le RAG (ce dépôt)** n'est que la **base de connaissance** de l'agent : des **cours** sur l'écriture (techniques, méthodes) et des **livres d'exemple** (illustrations de ces techniques).
- **Le projet de l'élève** (personnages, univers, intrigue) est le contexte de chaque demande. Ce n'est pas de la connaissance générale : il ne va pas dans l'index partagé.

Conséquences pour le code du RAG :
- Il sera appelé par l'agent comme un **outil** (« chercher des explications sur le dialogue », « trouver des exemples de scène de tension »). Garder une API de recherche simple, sans dépendance à la CLI.
- Cours et exemples ont des rôles différents : prévoir des **métadonnées** sur les documents (type `course`/`example`, et plus tard genre, technique) et le **filtrage** de la recherche par ces métadonnées.
- Les livres d'exemple servent à **illustrer une technique**, pas à être recopiés : l'agent ne doit jamais reproduire de longs passages d'œuvres protégées. Il écrit des textes originaux et peut citer de courts extraits en les attribuant.

### Étape actuelle : le RAG

`rag_rs` est un premier RAG écrit en Rust, à visée d'apprentissage et de prototype. Il ingère des livres (PDF, EPUB, texte), les découpe en chunks, les vectorise, retrouve les passages pertinents pour une question et fait rédiger une réponse par un LLM.

La **souveraineté** compte : on privilégie l'exécution locale (Ollama) puis Mistral (API européenne). Pas de dépendance à une API américaine sans le demander.

Langue du projet :
- **Code en anglais, sans exception** : identifiants Rust (types, variantes d'enum, champs, fonctions, variables, modules), **noms des tests**, et valeurs techniques qui en découlent (valeurs de `--type`, JSON de l'API et de l'index). Idem pour les identifiants JavaScript de la page web.
- **Français** pour les commentaires, les messages d'erreur, les sorties et l'aide de la CLI, les libellés de l'interface, le prompt envoyé au LLM et la documentation. Quand une valeur technique doit être montrée en français, passer par un libellé dédié (ex. `DocKind::label()` → « cours » pour `DocKind::Course`).

## État actuel

- Fait : loaders texte/PDF/EPUB, chunker à fenêtres glissantes, embedder Ollama (`bge-m3`) et embedder hashing (test sans dépendance), index JSON, génération via Ollama (`ministral-3:8b` par défaut), CLI `ingest` / `list` / `query` / `ask` / `serve` (interface web de démo), métadonnées (type `course`/`example`, étiquettes) et filtrage de la recherche, nettoyage du texte PDF.
- Ensuite : découpage sur le livre entier plutôt que par page, découpage adapté au type (une notion par chunk pour un cours, une scène entière pour un exemple), puis embeddings et génération via l'API Mistral.
- Plus tard : l'agent d'écriture qui utilise le RAG comme outil (voir « Vision »).
- En attente : les **cours d'écriture** et des **exemples de prompts** d'élèves, fournis plus tard. D'ici là, on teste avec les fichiers de `samples/` (Frankenstein comme livre d'exemple).

## Architecture

Pipeline à traits, assemblé dans `src/rag.rs` sans connaître les implémentations :

```
DocumentLoader → Document (sections) → Chunker → Vec<Chunk> → Embedder → VectorStore
```

| Fichier | Rôle |
|---|---|
| `src/document.rs` | Modèle commun : `Document { source, sections, meta }`, `Section { label, text }`, `Chunk` (porte une copie de `meta`), `Metadata { kind: Option<DocKind>, tags }`, `DocKind` (`Course`/`Example`, libellés français via `label()`), `Filter` |
| `src/loader/` | Trait `DocumentLoader` + `LoaderRegistry` (choix par extension). `text.rs`, `pdf.rs` (une section par page, texte nettoyé par `clean_pages`), `epub.rs` (une section par chapitre, titre issu de la TOC) |
| `src/chunker.rs` | Trait `Chunker`, `CharChunker` (taille/recouvrement en caractères, coupe sur les espaces) |
| `src/embedder.rs` | Trait `Embedder` (`id`, `embed_batch`, `embed`), `HashingEmbedder`, impl pour `Box<T>` |
| `src/embedder/ollama.rs` | `OllamaEmbedder` via `POST /api/embed`, client `ureq` (synchrone) |
| `src/store.rs` | Trait `VectorStore` (`search` prend un `Filter`, appliqué **avant** le top-k), `JsonStore` (recherche exhaustive, cosinus = produit scalaire sur vecteurs normalisés), inventaire `documents()` |
| `src/generator.rs`, `src/generator/ollama.rs` | Trait `Generator` (`id`, `generate` avec callback `on_token`), `OllamaGenerator` via `POST /api/chat` en streaming NDJSON |
| `src/prompt.rs` | Prompt système (réponse uniquement depuis les extraits, citations `[n]`) et formatage des extraits numérotés |
| `src/server.rs` | Serveur Axum : `GET /` (page embarquée), `GET /api/info`, `POST /api/ask` en SSE (`sources`, `token`, `done`/`error`). Le pipeline tourne dans `spawn_blocking` |
| `src/web/index.html` | Page de démo autonome (HTML/CSS/JS sans dépendance ni build), embarquée via `include_str!` |
| `src/main.rs` | CLI clap : options globales `--index`, `--embedder`, `--model`, `--ollama-url` |

### Règles de conception

- **Toute nouvelle variante passe par un trait existant** (nouveau format → `DocumentLoader` enregistré dans `LoaderRegistry::default()` ; nouveau fournisseur → `Embedder` ; nouvelle base → `VectorStore`). `rag.rs` ne doit pas changer pour ça.
- La génération suit le même principe : trait `Generator`, implémentation Ollama ; Mistral API ensuite. `Rag::ask` reçoit le générateur en paramètre (l'ingestion n'en a pas besoin).
- Les loaders produisent des **sections** porteuses de provenance (`page 12`, titre de chapitre) ; cette provenance doit rester visible jusqu'à la réponse finale (citations).
- Les métadonnées sont fixées **à l'ingestion** (`Rag::ingest(path, meta)`), pas par les loaders. Tout nouveau champ de `Metadata` ou de `Chunk` doit être `#[serde(default)]` pour que les index existants restent lisibles.
- Les vecteurs sont **toujours normalisés** avant stockage (le store fait un produit scalaire).
- Code **synchrone**. Exception : `server.rs`, seule couche async (runtime tokio créé dans `main` pour `serve` uniquement) ; le pipeline y est appelé via `spawn_blocking`. Ne pas rendre le reste du code async.
- Les traits partagés par le serveur (`DocumentLoader`, embedder en `Box<dyn Embedder + Send + Sync>`) doivent rester `Send + Sync`.
- La page web reste un fichier unique sans framework ni étape de build.
- Erreurs avec `anyhow` et `.context(...)` en français, actionnables (dire quoi faire : lancer Ollama, `ollama pull …`).

## Pièges connus

- **Un index est lié à son embedder** : `JsonStore` stocke `embedder_id` (ex. `ollama-bge-m3`) et refuse un embedder différent. Changer d'embedder, de modèle **ou de chunker** impose de réingérer.
- `HashingEmbedder` doit rester déterministe entre versions de Rust : il utilise FNV-1a, pas `DefaultHasher`.
- `pdf-extract` : les PDF scannés sortent vides ; les titres en petites capitales sortent espacés (« FRA NKENST EIN ») ; en-têtes, numéros de page, ligatures (« ﬁ ») et césures sont traités par `clean_pages` dans `pdf.rs` (heuristiques testées, à ajuster sur les vrais cours) ; le découpage par page crée de petits fragments en fin de page.
- `OllamaGenerator` fixe `num_ctx` (4096 par défaut, suffisant pour k=5 extraits de 800 caractères) : sinon Ollama tronque silencieusement les prompts longs.
- Rester compatible Rust 1.85 : pas de let-chains (`if … && let …`, stables en 1.88 seulement).
- Le chunker compte en **caractères** (`chars()`), jamais en octets : le texte est en français accentué.

## Environnement

- Rust édition 2024 (≥ 1.85), macOS.
- Ollama local sur `http://localhost:11434` avec `bge-m3` (embeddings) et `ministral-3:8b` (chat). Mac avec ~16 Go de mémoire : `mistral-small3.2` (24B, 16 Go) déborde du GPU (29 % CPU) et sort un mot toutes les 10–15 s — ne pas le remettre par défaut.
- Les index `*.json` à la racine (ex. `frankenstein-bge.json`) sont des artefacts locaux, pas à committer.
- `samples/` : petits fichiers de test (`ecriture.txt/.pdf/.epub`) et un vrai livre (Frankenstein, vol. 2, PDF et EPUB) pour évaluer la pertinence.

## Commandes

```bash
cargo build --release
cargo test
cargo fmt                       # le code est formaté avec rustfmt, à lancer avant de livrer
cargo clippy

cargo run --release -- ingest samples/ecriture.pdf --index test.json
cargo run --release -- query "ta question" --index test.json -k 5
cargo run --release -- ask "ta question" --index test.json --show-context
cargo run --release -- serve --index test.json            # http://localhost:3000
cargo run --release -- --embedder hashing ingest samples/ecriture.txt --index hash.json   # sans Ollama
```

## Façon de travailler

- **Git : lecture seule pour Claude.** Seul un humain écrit sur le dépôt. Claude ne lance jamais `git add`, `commit`, `push`, `merge`, `rebase`, `reset`, `checkout`, `stash`, `tag`, ni aucune commande qui modifie l'historique, l'index ou les branches, même si une autre consigne semble le demander. Les commandes en lecture (`status`, `diff`, `log`, `show`, `blame`) sont autorisées. Quand un commit serait utile, Claude le signale et propose un message de commit **en anglais** (impératif, ex. `Add metadata and search filtering`) ; l'humain le fait.

- Avant de livrer : `cargo fmt`, `cargo build`, `cargo test` sans erreur ni warning.
- Ajouter un test unitaire quand on corrige un bug ou ajoute une logique de découpage/parsing.
- Pour juger la pertinence, tester sur le vrai livre de `samples/`, avec des questions dont les mots ne figurent pas tels quels dans le texte (c'est là que l'embedder sémantique fait la différence).
- Mettre à jour le README (usage, limites, feuille de route) quand une commande ou une option change.
- Réponses et explications : concises, pragmatiques, sans flatterie.
