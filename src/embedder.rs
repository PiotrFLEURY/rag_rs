//! Transformation d'un texte en vecteur.

mod ollama;

pub use ollama::OllamaEmbedder;

use anyhow::Result;

pub trait Embedder {
    /// Identifiant stocké dans l'index : empêche d'interroger un index avec
    /// un embedder différent de celui qui l'a construit.
    fn id(&self) -> String;

    /// Vectorise plusieurs textes d'un coup (un seul appel réseau par lot).
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.embed_batch(&[text])?.remove(0))
    }
}

/// Permet de choisir l'embedder à l'exécution (`Box<dyn Embedder>`).
impl<T: Embedder + ?Sized> Embedder for Box<T> {
    fn id(&self) -> String {
        (**self).id()
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        (**self).embed_batch(texts)
    }
}

/// Embedder local sans modèle ni réseau (« hashing trick » sur les mots).
///
/// Il ne capte pas le sens (« voiture » ≠ « automobile »), seulement le
/// vocabulaire commun : suffisant pour valider le pipeline. À remplacer par
/// un vrai modèle d'embeddings (Mistral, Ollama…) via le même trait.
pub struct HashingEmbedder {
    pub dim: usize,
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self { dim: 1024 }
    }
}

impl Embedder for HashingEmbedder {
    fn id(&self) -> String {
        format!("hashing-fnv1a-{}", self.dim)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.hash_embed(t)).collect())
    }
}

impl HashingEmbedder {
    fn hash_embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for token in tokenize(text) {
            let h = fnv1a(token.as_bytes());
            let idx = (h % self.dim as u64) as usize;
            // Le bit de signe limite l'effet des collisions.
            let sign = if (h >> 63) == 0 { 1.0 } else { -1.0 };
            v[idx] += sign;
        }
        normalize(&mut v);
        v
    }
}

fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 2) // ignore "le", "de", "a"…
        .map(|t| {
            // Racinisation naïve : retire le pluriel (« personnages » → « personnage »).
            let t = t.to_lowercase();
            match t.strip_suffix(['s', 'x']) {
                Some(stem) if stem.chars().count() > 3 => stem.to_string(),
                _ => t,
            }
        })
}

/// FNV-1a : hash stable entre versions de Rust (contrairement à DefaultHasher),
/// indispensable puisque les vecteurs sont persistés sur disque.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministe_et_normalise() {
        let e = HashingEmbedder::default();
        let a = e.embed("Le personnage principal").unwrap();
        assert_eq!(a, e.embed("Le personnage principal").unwrap());
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
