# rag_rs

Premier RAG (Retrieval-Augmented Generation) en Rust : ingestion de fichiers, découpage en chunks, vectorisation, puis recherche des passages les plus pertinents pour une question.

Formats gérés :

| Extension | Loader | Découpage en sections |
|---|---|---|
| `.txt`, `.md` | `TextLoader` | une seule section |
| `.pdf` | `PdfLoader` (crate `pdf-extract`) | une section par page (`page 3`) |
| `.epub` | `EpubLoader` (crates `epub` + `html2text`) | une section par chapitre, titrée via la table des matières |

Le loader est choisi automatiquement d'après l'extension du fichier.

## Prérequis

- Rust ≥ 1.85 (édition 2024)
- [Ollama](https://ollama.com) avec un modèle d'embeddings :

  ```bash
  ollama pull bge-m3
  ```

  ```bash
  ollama pull ministral-3:8b     # modèle de chat pour la commande ask
  ```

  `bge-m3` est multilingue et bon en français. Sans Ollama, utiliser `--embedder hashing` (voir plus bas).

## Utilisation

### Ingérer un fichier

```bash
cargo run -- ingest samples/ecriture.txt --chunk-size 400 --overlap 80
cargo run -- ingest samples/ecriture.pdf --chunk-size 400 --overlap 80
cargo run -- ingest samples/ecriture.epub --chunk-size 400 --overlap 80
```

Options :

| Option | Défaut | Rôle |
|---|---|---|
| `--chunk-size` | `800` | Taille max d'un chunk, en caractères |
| `--overlap` | `150` | Recouvrement entre deux chunks consécutifs |
| `--index` | `index.json` | Fichier d'index à créer ou compléter |

Réingérer un fichier remplace ses anciens chunks (pas de doublons). Plusieurs fichiers peuvent cohabiter dans le même index.

### Métadonnées : type et étiquettes

Chaque document peut recevoir à l'ingestion un **type** et des **étiquettes**, qui servent ensuite à filtrer la recherche :

```bash
cargo run --release -- ingest cours/dialogues.pdf --type course --tag dialogue
cargo run --release -- ingest romans/frankenstein.pdf --type example --tag gothique --tag roman
```

- `--type` : `course` (cours : explique des techniques d'écriture) ou `example` (exemple : texte littéraire qui les illustre). Facultatif : sans lui, le document est « sans type ».
- `--tag` : étiquette libre (genre, technique…), répétable. Les étiquettes sont mises en minuscules.

Pour changer les métadonnées d'un document, il faut le réingérer.

Voir le contenu de l'index :

```bash
cargo run --release -- list --index livre.json
```

Filtrer la recherche (valable pour `query` et `ask`) :

```bash
cargo run --release -- ask "Comment écrire un bon dialogue ?" --type course --index livre.json
cargo run --release -- query "scène de tension" --type example --tag gothique --index livre.json
```

Avec plusieurs `--tag`, un document doit les porter **toutes**. Un document sans type est exclu dès qu'on filtre sur le type. Dans le prompt, chaque extrait est marqué `(cours)` ou `(exemple)` pour que le LLM sache comment s'en servir. Les index créés avant les métadonnées restent lisibles (documents « sans type »).

### Choisir l'embedder

Options globales, valables pour `ingest` et `query` :

| Option | Défaut | Rôle |
|---|---|---|
| `--embedder` | `ollama` | `ollama` (sémantique, local) ou `hashing` (lexical, sans dépendance) |
| `--model` | `bge-m3` | Modèle d'embeddings Ollama |
| `--ollama-url` | `http://localhost:11434` | URL du serveur Ollama |

Un index ne peut être interrogé qu'avec l'embedder et le modèle qui l'ont construit : en changer impose de réingérer (de préférence dans un autre fichier via `--index`).

```bash
cargo run --release -- ingest mon_livre.pdf --index livre.json
cargo run --release -- query "ta question" --index livre.json -k 5
```

Lancer en `--release` : l'ingestion d'un livre entier prend quelques minutes. Les chunks sont envoyés à Ollama par lots de 32.

### Interroger l'index

```bash
cargo run -- query "comment accélérer le rythme d'une scène ?" -k 3
```

- `-k` : nombre de passages renvoyés (défaut `3`)
- `--index` : index à interroger (défaut `index.json`)

Chaque résultat affiche son score de similarité (cosinus), sa source, sa section éventuelle (page, chapitre) et le texte du chunk.

### Poser une question (réponse rédigée)

```bash
cargo run --release -- ask "Pourquoi la créature se sent-elle rejetée ?" --index livre.json
```

Les `k` passages les plus pertinents sont envoyés au LLM avec la question. La réponse s'affiche au fil de l'eau, avec des renvois `[1]`, `[2]`… vers les sources listées en fin de réponse. Le LLM a pour consigne de ne répondre qu'à partir des extraits et de le dire quand ils ne suffisent pas.

| Option | Défaut | Rôle |
|---|---|---|
| `-k` | `5` | Nombre de passages donnés au LLM |
| `--chat-model` | `ministral-3:8b` | Modèle de chat Ollama |
| `--temperature` | `0.2` | Bas = réponses fidèles au texte |
| `--num-ctx` | `4096` | Contexte du LLM en tokens (à augmenter si `-k` est grand) |
| `--show-context` | — | Affiche aussi le texte des extraits utilisés |

`query` reste disponible pour voir les passages bruts, sans génération.

#### Choisir le modèle de chat selon la mémoire

Le modèle doit tenir **entièrement** en mémoire GPU, sinon la génération devient extrêmement lente (plusieurs secondes par mot). Sur Mac, le GPU dispose d'environ 65–75 % de la mémoire unifiée.

| Mémoire du Mac | Modèle |
|---|---|
| 8 Go | `ministral-3:3b` |
| 16 Go | `ministral-3:8b` (défaut) |
| 24–32 Go | `ministral-3:14b`, ou `mistral-small3.2` (24B, ~16 Go) en fermant les autres applications |

Vérifier pendant une génération avec `ollama ps` : la colonne `PROCESSOR` doit indiquer `100% GPU`. Une valeur comme `29%/71% CPU/GPU` signifie que le modèle déborde : prendre plus petit ou réduire `--num-ctx`.

### Interface web de démo

```bash
cargo run --release -- serve --index livre.json
```

Puis ouvrir http://localhost:3000. La page affiche les documents indexés (avec leur type) et les modèles utilisés, permet de limiter la recherche aux cours ou aux exemples, propose des questions d'exemple, écrit la réponse au fil de l'eau et liste les sources (page, score, texte). Un clic sur un renvoi `[n]` ouvre la source correspondante. Thème clair/sombre automatique, utilisable sur mobile.

| Option | Défaut | Rôle |
|---|---|---|
| `--port` | `3000` | Port d'écoute (local uniquement : `127.0.0.1`) |
| `-k` | `5` | Passages consultés par défaut (modifiable dans la page) |
| `--chat-model`, `--temperature`, `--num-ctx` | comme `ask` | Réglages du LLM |

Les questions d'exemple sont dans `src/web/index.html` (constante `EXAMPLES`), à adapter au livre indexé. La page est embarquée dans le binaire (`include_str!`) : recompiler après l'avoir modifiée.

API utilisée par la page :
- `GET /api/info` : index, nombre de passages, documents (avec type et étiquettes), modèles.
- `POST /api/ask` avec `{"question": "...", "k": 5, "type": "course", "tags": ["dialogue"]}` (`type` et `tags` facultatifs) : flux SSE avec les événements `sources` (JSON), `token` (chaîne JSON), puis `done` ou `error`.

### Tests

```bash
cargo test
```

## Architecture

Chaque étape du pipeline est un trait. `rag.rs` les assemble sans connaître les implémentations concrètes.

```
fichier ──► DocumentLoader ──► Document (sections)
                                   │
                                Chunker ──► Vec<Chunk>
                                   │
                                Embedder ──► Vec<f32>
                                   │
                              VectorStore ──► recherche top-k
```

| Fichier | Trait | Implémentation actuelle |
|---|---|---|
| `src/loader/` | `DocumentLoader` | `TextLoader`, `PdfLoader`, `EpubLoader` |
| `src/chunker.rs` | `Chunker` | `CharChunker` : fenêtres glissantes, coupe sur un espace |
| `src/embedder.rs`, `src/embedder/` | `Embedder` | `OllamaEmbedder` (défaut, via `/api/embed`), `HashingEmbedder` (hashing trick, sans modèle ni réseau) |
| `src/store.rs` | `VectorStore` | `JsonStore` : en mémoire, recherche exhaustive, persisté en JSON |
| `src/document.rs` | — | Modèle commun : `Document`, `Section`, `Chunk` |
| `src/rag.rs` | — | Orchestration `ingest` / `retrieve` |
| `src/generator.rs`, `src/generator/` | `Generator` | `OllamaGenerator` (`/api/chat`, streaming) |
| `src/prompt.rs` | — | Prompt système et mise en forme des extraits numérotés |
| `src/server.rs`, `src/web/index.html` | — | Serveur Axum (`serve`) et page de démo |
| `src/main.rs` | — | CLI (clap) |

Un `Document` est découpé en `sections` : une seule pour un texte brut, une par page pour un PDF, une par chapitre pour un EPUB. La provenance est conservée jusque dans les résultats.

## Ajouter un format de fichier

1. Créer `src/loader/docx.rs` qui implémente `DocumentLoader` (voir `pdf.rs` ou `epub.rs` comme modèle) :

   ```rust
   pub struct DocxLoader;

   impl DocumentLoader for DocxLoader {
       fn extensions(&self) -> &[&str] { &["docx"] }
       fn load(&self, path: &Path) -> Result<Document> {
           // extraire le texte → une ou plusieurs Section
       }
   }
   ```

2. Le déclarer dans `src/loader/mod.rs` (`mod docx; pub use docx::DocxLoader;`) et l'enregistrer dans `LoaderRegistry::default()` :

   ```rust
   Self::new()
       .register(TextLoader)
       .register(PdfLoader)
       .register(EpubLoader)
       .register(DocxLoader)
   ```

Le reste du code ne change pas.

## Limites actuelles

- **`HashingEmbedder`** : lexical, il compare les mots en commun, pas le sens (« voiture » ≠ « automobile »). Utile seulement pour tester sans Ollama.
- **Taille de l'index** : JSON avec des vecteurs de 1024 flottants (`bge-m3`), plusieurs dizaines de Mo pour un gros livre.
- **Index lié à l'embedder** : l'index stocke l'identifiant de l'embedder qui l'a construit et refuse d'être interrogé avec un autre. Changer d'embedder impose de réingérer.
- **Recherche exhaustive** : adaptée jusqu'à quelques dizaines de milliers de chunks. Au-delà, implémenter `VectorStore` sur une base vectorielle (Qdrant, LanceDB…).
- **PDF scannés** : sans couche texte, ils ressortent vides (il faudrait un OCR). La mise en page complexe (colonnes, notes de bas de page) peut mélanger l'ordre du texte.
- **EPUB protégés par DRM** : non lisibles.

## Feuille de route

- [x] Embeddings locaux via Ollama (`bge-m3`)
- [ ] Embeddings Mistral (`mistral-embed`)
- [ ] Nettoyage du texte PDF (en-têtes, pieds de page, césures)
- [x] Commande `ask` : passages top-k injectés comme contexte dans un LLM (Ollama, `ministral-3:8b`)
- [x] Interface web de démo (`serve`)
- [x] Métadonnées (type `course`/`example`, étiquettes) et filtrage de la recherche
- [ ] Génération via l'API Mistral
- [x] Loader PDF
- [x] Loader EPUB
