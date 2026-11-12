//! Découpage d'un flux NDJSON (une valeur JSON par ligne) arrivé en morceaux quelconques : une
//! ligne, ou un caractère UTF-8, peut être coupé entre deux morceaux.

use crate::error::{OllamaError, Result};

/// Une ligne d'Ollama fait au plus quelques centaines d'octets : au-delà, le flux n'est pas du
/// NDJSON.
const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Default)]
pub(crate) struct Lines {
    pending: Vec<u8>,
}

impl Lines {
    /// Ajoute un morceau et renvoie les lignes qu'il termine, sans fin de ligne ; les lignes
    /// vides sont écartées.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut lines = Vec::new();
        let mut rest = chunk;
        while let Some(i) = rest.iter().position(|&b| b == b'\n') {
            self.pending.extend_from_slice(&rest[..i]);
            lines.extend(take_line(&mut self.pending));
            rest = &rest[i + 1..];
        }
        self.pending.extend_from_slice(rest);
        if self.pending.len() > MAX_LINE {
            return Err(OllamaError::Invalid(format!(
                "ligne de plus de {MAX_LINE} octets dans un flux NDJSON"
            )));
        }
        Ok(lines)
    }

    /// Dernière ligne, quand le flux se termine sans fin de ligne.
    pub(crate) fn finish(mut self) -> Option<Vec<u8>> {
        take_line(&mut self.pending)
    }
}

fn take_line(pending: &mut Vec<u8>) -> Option<Vec<u8>> {
    let mut line = std::mem::take(pending);
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    (!line.iter().all(u8::is_ascii_whitespace)).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &[u8], size: usize) -> Vec<String> {
        let mut lines = Lines::default();
        let mut out = Vec::new();
        for chunk in text.chunks(size) {
            out.extend(lines.push(chunk).unwrap());
        }
        out.extend(lines.finish());
        out.into_iter()
            .map(|l| String::from_utf8(l).unwrap())
            .collect()
    }

    #[test]
    fn lignes_coupees_en_morceaux() {
        let text = "{\"status\":\"téléchargement\"}\n\n{\"a\":1}\r\n  \n{\"b\":2}".as_bytes();
        let expected = ["{\"status\":\"téléchargement\"}", "{\"a\":1}", "{\"b\":2}"];
        // Toutes les tailles de morceau, y compris au milieu d'un caractère accentué.
        for size in 1..=text.len() {
            assert_eq!(split(text, size), expected, "morceaux de {size} octets");
        }
        assert!(split(b"", 1).is_empty());
        assert_eq!(split(b"{}\n", 3), ["{}"]);
    }

    #[test]
    fn ligne_trop_longue() {
        let mut lines = Lines::default();
        assert!(lines.push(&vec![b'x'; MAX_LINE]).unwrap().is_empty());
        assert!(matches!(lines.push(b"x"), Err(OllamaError::Invalid(_))));
    }
}
