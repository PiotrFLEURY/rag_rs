use super::DocumentLoader;
use crate::document::{Document, Section};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// PDF contenant du texte (une section par page).
///
/// Le texte extrait est nettoyé (voir [`clean_pages`]) : en-têtes et pieds de
/// page répétés, numéros de page, ligatures typographiques, mots coupés en fin
/// de ligne.
///
/// Les PDF scannés (images sans couche texte) ressortent vides : il faudra un
/// OCR pour ceux-là.
pub struct PdfLoader;

impl DocumentLoader for PdfLoader {
    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn load(&self, path: &Path) -> Result<Document> {
        let pages = pdf_extract::extract_text_by_pages(path)
            .with_context(|| format!("extraction PDF impossible : {}", path.display()))?;

        let sections = clean_pages(&pages)
            .into_iter()
            .enumerate()
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(i, text)| Section {
                label: Some(format!("page {}", i + 1)),
                text,
            })
            .collect();

        Ok(Document {
            source: path.display().to_string(),
            sections,
            meta: Default::default(),
        })
    }
}

/// Nombre de lignes examinées en haut et en bas de chaque page pour y chercher
/// en-têtes, pieds de page et numéros de page.
const EDGE_LINES: usize = 2;

/// Une ligne de bord est un en-tête (ou pied de page) si elle revient sur au
/// moins `MIN_REPEAT_PAGES` pages et au moins `MIN_REPEAT_RATIO` des pages.
const MIN_REPEAT_PAGES: usize = 3;
const MIN_REPEAT_RATIO: f64 = 0.4;

/// Nettoie le texte brut extrait page par page. Renvoie autant de pages qu'en
/// entrée (une page peut devenir vide), pour garder la numérotation.
fn clean_pages(pages: &[String]) -> Vec<String> {
    let mut pages: Vec<Vec<String>> = pages
        .iter()
        .map(|p| p.lines().map(clean_chars).collect())
        .collect();
    remove_page_furniture(&mut pages);
    pages.iter().map(|lines| join_lines(lines)).collect()
}

/// Remplace les ligatures (« ﬁ » → « fi ») et supprime les césures
/// conditionnelles invisibles (U+00AD). Sans ça, « difﬁcile » et « difficile »
/// sont deux mots différents pour l'embedder.
fn clean_chars(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        match c {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            '\u{00AD}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Retire en haut et en bas de chaque page les numéros de page et les lignes
/// qui se répètent d'une page à l'autre (titre du livre, nom de l'auteur…).
fn remove_page_furniture(pages: &mut [Vec<String>]) {
    let repeated = repeated_edge_lines(pages);
    let is_furniture = |line: &str| is_page_number(line) || repeated.contains(&furniture_key(line));

    for lines in pages.iter_mut() {
        // Haut de page.
        let mut removed = 0;
        while removed < EDGE_LINES {
            match lines.iter().position(|l| !l.trim().is_empty()) {
                Some(i) if is_furniture(&lines[i]) => {
                    lines.remove(i);
                    removed += 1;
                }
                _ => break,
            }
        }
        // Bas de page.
        let mut removed = 0;
        while removed < EDGE_LINES {
            match lines.iter().rposition(|l| !l.trim().is_empty()) {
                Some(i) if is_furniture(&lines[i]) => {
                    lines.remove(i);
                    removed += 1;
                }
                _ => break,
            }
        }
    }
}

/// Clés des lignes de bord qui reviennent sur assez de pages pour être des
/// en-têtes ou pieds de page.
fn repeated_edge_lines(pages: &[Vec<String>]) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut non_empty_pages = 0;

    for lines in pages {
        let non_empty: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        if non_empty.is_empty() {
            continue;
        }
        non_empty_pages += 1;
        let n = non_empty.len();
        let edges = non_empty
            .iter()
            .take(EDGE_LINES)
            .chain(non_empty.iter().skip(n.saturating_sub(EDGE_LINES)));
        // Une même ligne ne compte qu'une fois par page.
        let keys: HashSet<String> = edges
            .map(|l| furniture_key(l))
            .filter(|k| !k.is_empty())
            .collect();
        for key in keys {
            *counts.entry(key).or_default() += 1;
        }
    }

    let threshold =
        MIN_REPEAT_PAGES.max((non_empty_pages as f64 * MIN_REPEAT_RATIO).ceil() as usize);
    counts
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(key, _)| key)
        .collect()
}

/// Forme normalisée d'une ligne pour repérer les répétitions : minuscules,
/// blancs réduits, chiffres remplacés par `#` (« Frankenstein — 12 » et
/// « Frankenstein — 13 » sont le même en-tête).
fn furniture_key(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

/// « 12 », « - 12 - », « Page 12 », « p. 12 », « 12 / 300 », « 12 sur 300 ».
fn is_page_number(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    let rest = lower
        .strip_prefix("page")
        .or_else(|| lower.strip_prefix("p."))
        .unwrap_or(&lower);
    let rest: String = rest
        .chars()
        .filter(|c| !matches!(c, '-' | '–' | '—' | '|' | '[' | ']' | '(' | ')'))
        .collect();
    let rest = rest.replace(" sur ", "/");
    let parts: Vec<&str> = rest.split('/').map(str::trim).collect();
    parts.len() <= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 4 && p.chars().all(|c| c.is_ascii_digit()))
}

/// Recolle les lignes d'une page. Un mot coupé par un tiret en fin de ligne
/// est ressoudé : sans tiret pour une césure (« senti-/ments » → « sentiments »),
/// avec tiret pour un mot composé (« Croyez-/moi » → « Croyez-moi »).
fn join_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let line = line.trim_end();
        let next = line.trim_start();
        let first = next.chars().next();
        let ends_with_cut =
            out.ends_with('-') && out.chars().rev().nth(1).is_some_and(|c| c.is_alphabetic());

        if ends_with_cut && first.is_some_and(|c| c.is_alphabetic()) {
            if !keeps_hyphen(left_word(&out), next) {
                out.pop();
            }
            out.push_str(next);
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

/// Mot qui précède le tiret final de `text`.
fn left_word(text: &str) -> &str {
    let body = &text[..text.len() - 1];
    let start = body
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic())
        .last()
        .map_or(body.len(), |(i, _)| i);
    &body[start..]
}

/// Second élément fréquent des mots composés (« dis-moi », « a-t-il »,
/// « celui-ci », « vous-même », « c'est-à-dire »).
const COMPOUND_TAILS: &[&str] = &[
    "moi", "toi", "lui", "elle", "elles", "il", "ils", "on", "nous", "vous", "eux", "leur",
    "leurs", "le", "la", "les", "y", "en", "je", "tu", "ce", "t", "ci", "là", "même", "mêmes", "à",
    "dessus", "dessous", "delà", "être",
];

/// Premier élément fréquent des mots composés (« au-dessus », « peut-être »,
/// « très-confuse » dans les textes anciens).
const COMPOUND_HEADS: &[&str] = &[
    "au", "très", "peut", "porte", "arc", "avant", "arrière", "grand", "grande", "demi", "mi",
    "semi", "non", "contre", "sous", "vis", "belle", "beau", "petit", "petite", "quelques",
];

/// Faut-il garder le tiret entre `left` (fin de ligne) et `next` (ligne suivante) ?
/// Heuristique sans dictionnaire : mot composé connu ou nom propre (« Jean-/Pierre »).
fn keeps_hyphen(left: &str, next: &str) -> bool {
    let right: String = next.chars().take_while(|c| c.is_alphabetic()).collect();
    let starts_upper = right.chars().next().is_some_and(char::is_uppercase);
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    starts_upper
        || COMPOUND_TAILS.contains(&right.as_str())
        || COMPOUND_HEADS.contains(&left.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[test]
    fn expands_ligatures_and_soft_hyphens() {
        assert_eq!(
            clean_chars("difﬁcile, inﬂua, efface\u{00AD}ment"),
            "difficile, influa, effacement"
        );
    }

    #[test]
    fn removes_page_numbers_at_edges_only() {
        let pages = vec![
            page(&["Premier paragraphe.", "", "12"]),
            page(&["- 13 -", "Suite du texte en 1821."]),
        ];
        let cleaned = clean_pages(&pages);
        assert_eq!(cleaned[0], "Premier paragraphe.\n");
        assert_eq!(cleaned[1], "Suite du texte en 1821.");

        // Un nombre au milieu de la page est du contenu.
        let pages = vec![page(&["Début", "1821", "Fin"])];
        assert_eq!(clean_pages(&pages)[0], "Début\n1821\nFin");
    }

    #[test]
    fn recognizes_page_number_formats() {
        for s in [
            "12",
            " 12 ",
            "- 12 -",
            "— 7 —",
            "Page 12",
            "p. 3",
            "12 / 300",
            "12 sur 300",
            "[4]",
        ] {
            assert!(is_page_number(s), "{s:?}");
        }
        for s in ["", "12345", "Chapitre 12", "1821 fut une année", "-"] {
            assert!(!is_page_number(s), "{s:?}");
        }
    }

    #[test]
    fn removes_repeated_headers_and_footers() {
        let bodies = [
            "Il faisait nuit.",
            "La porte grinça.",
            "Personne ne vint.",
            "Le jour se leva.",
            "Tout était calme.",
        ];
        let pages: Vec<String> = bodies
            .iter()
            .enumerate()
            .map(|(n, body)| {
                page(&[
                    "Frankenstein — Mary Shelley",
                    body,
                    &format!("Cours d'écriture · {}", n + 1),
                ])
            })
            .collect();
        let cleaned = clean_pages(&pages);
        assert_eq!(cleaned, bodies);
    }

    #[test]
    fn keeps_lines_repeated_on_few_pages() {
        // Un titre de chapitre en haut de 2 pages sur 10 n'est pas un en-tête.
        let mut pages: Vec<String> = (0..10).map(|n| format!("Texte {n}.")).collect();
        pages[0] = page(&["CHAPITRE", "Texte."]);
        pages[5] = page(&["CHAPITRE", "Texte."]);
        assert!(clean_pages(&pages)[0].starts_with("CHAPITRE"));
    }

    #[test]
    fn joins_hyphenated_words() {
        let join = |a: &str, b: &str| join_lines(&[a.to_string(), b.to_string()]);
        // Césure : le tiret disparaît.
        assert_eq!(join("des senti-", "ments vifs"), "des sentiments vifs");
        // Mots composés : le tiret reste.
        assert_eq!(
            join("Croyez-", "moi, Frankenstein"),
            "Croyez-moi, Frankenstein"
        );
        assert_eq!(join("au-", "dessus des"), "au-dessus des");
        assert_eq!(
            join("une connaissance très-", "confuse"),
            "une connaissance très-confuse"
        );
        assert_eq!(join("que toi-", "même tu"), "que toi-même tu");
        assert_eq!(join("Jean-", "Pierre"), "Jean-Pierre");
        assert_eq!(join("qu'au-", "dessus"), "qu'au-dessus");
        // Un tiret isolé (tiret de dialogue, énumération) n'est pas une coupure.
        assert_eq!(join("Il dit -", "oui"), "Il dit -\noui");
        assert_eq!(join("Fin de ligne.", "Début"), "Fin de ligne.\nDébut");
    }

    #[test]
    fn handles_multibyte_words_before_hyphen() {
        assert_eq!(left_word("l'été-"), "été");
        assert_eq!(left_word("-"), "");
        assert_eq!(
            join_lines(&["prélè-".into(), "vement".into()]),
            "prélèvement"
        );
    }
}
