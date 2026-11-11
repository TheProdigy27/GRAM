//! Familles de modèles : chaque variante (fine-tune, version « abliterated », distillation…)
//! est rattachée au modèle officiel dont elle dérive, pour que l'interface les regroupe.
//!
//! `gram-hub` réunit déjà les dépôts qui publient **le même** modèle ; une variante forme son
//! propre modèle, et son `base_model` désigne en général la variante elle-même. Le
//! rattachement se fait donc sur les noms affichés, découpés en mots en minuscules (`-`, `_`,
//! espace ; « Qwen3.8-27B » → `qwen3.8` `27b`) :
//!
//! - une variante contient, mot pour mot et d'un seul tenant, le nom d'un autre modèle du
//!   catalogue : « Huihui-Qwen3.8-27B-abliterated » contient « Qwen3.8-27B », mais
//!   « Qwen3-VL-8B-Instruct » ne contient pas « Qwen3-8B » ;
//! - le parent vient d'une autre organisation (celle de son modèle de base, sinon celle de son
//!   dépôt) : « Qwen2.5-14B-Instruct » et « Qwen2.5-14B », tous deux de Qwen, sont deux
//!   modèles officiels, pas une variante l'un de l'autre ;
//! - le mot qui suit le nom du parent n'est pas une taille (« 35b », « a3b », « e4b ») :
//!   « Qwen3.8-35B-A3B-Distill » n'est pas une variante d'un « Qwen3.8-35B » dense ;
//! - le nom du parent a au moins deux mots, dont un qui n'est ni un nombre ni une taille
//!   (« 7B » seul ne fait pas une famille) ;
//! - un parent dont le nom finit par « -it », « -Instruct » ou « -Chat » est aussi trouvé sans
//!   ce suffixe (« Gemma-4-31B-Humanlike » → gemma-4-31B-it), si le nom court n'est pas
//!   lui-même un modèle du catalogue et que le mot suivant n'est pas une autre déclinaison
//!   officielle (« Qwen3-VL-8B-Thinking » n'est pas une variante de « Qwen3-VL-8B-Instruct »).
//!
//! Entre plusieurs parents possibles, le nom le plus long l'emporte (« Qwen3.8-27B » plutôt
//! que « Qwen3.8 »), puis le nom exact plutôt que le nom sans suffixe, puis le plus à droite
//! (« DeepSeek-V4-Pro-Qwen3.5-9B » est un Qwen3.5-9B distillé). Une variante d'une variante
//! remonte au modèle de tête : une famille n'a qu'un niveau.

use crate::model::ModelEntry;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

/// Suffixes retirés du nom d'un parent pour le trouver aussi sans eux.
const FLAVOR_SUFFIXES: [&str; 3] = ["it", "instruct", "chat"];

/// Déclinaisons officielles : après un nom sans suffixe, elles désignent un modèle frère, pas
/// une variante.
const OFFICIAL_FLAVORS: [&str; 6] = ["it", "instruct", "chat", "thinking", "base", "reasoning"];

/// Familles du catalogue : dépôt canonique d'une variante → dépôt canonique du modèle de tête
/// de sa famille. Un modèle officiel, ou une variante sans parent trouvé, n'y figure pas.
pub fn families(models: &[ModelEntry]) -> HashMap<&str, &str> {
    let entries: Vec<Entry> = models.iter().map(Entry::new).collect();
    let index = Index::new(&entries);
    let parents: Vec<Option<usize>> = (0..entries.len())
        .map(|i| index.parent(&entries, i))
        .collect();

    let mut out = HashMap::new();
    for (i, parent) in parents.iter().enumerate() {
        let Some(mut head) = *parent else { continue };
        // Remonte au modèle de tête. Chaque parent a un nom plus court que sa variante (ou
        // égal, par un nom sans suffixe) : la limite ne sert qu'en cas de boucle imprévue.
        let mut steps = 0;
        while let Some(p) = parents[head] {
            head = p;
            steps += 1;
            if steps > entries.len() {
                break;
            }
        }
        if head != i {
            out.insert(models[i].repo_id.as_str(), models[head].repo_id.as_str());
        }
    }
    out
}

struct Entry<'a> {
    repo_id: &'a str,
    words: Vec<String>,
    /// Organisation d'origine, en minuscules.
    org: String,
}

impl<'a> Entry<'a> {
    fn new(m: &'a ModelEntry) -> Self {
        let origin = m.base_model.as_deref().unwrap_or(&m.repo_id);
        Self {
            repo_id: &m.repo_id,
            words: words(&m.display_name),
            org: origin.split_once('/').map_or("", |(o, _)| o).to_lowercase(),
        }
    }

    /// Peut servir de parent : au moins deux mots, dont un qui n'est ni un nombre ni une taille.
    fn can_be_parent(words: &[String]) -> bool {
        words.len() >= 2 && words.iter().any(|w| !is_number(w) && !is_size(w))
    }
}

/// Un parent possible, trouvé par son nom exact ou par son nom sans suffixe.
#[derive(Clone, Copy)]
struct Name {
    entry: usize,
    exact: bool,
}

/// Noms des parents possibles, en mots.
struct Index {
    names: HashMap<Vec<String>, Vec<Name>>,
    /// Plus long nom, en mots.
    longest: usize,
}

impl Index {
    fn new(entries: &[Entry]) -> Self {
        let mut names: HashMap<Vec<String>, Vec<Name>> = HashMap::new();
        let all: HashSet<&[String]> = entries.iter().map(|e| e.words.as_slice()).collect();
        for (i, e) in entries.iter().enumerate() {
            if !Entry::can_be_parent(&e.words) {
                continue;
            }
            names.entry(e.words.clone()).or_default().push(Name {
                entry: i,
                exact: true,
            });
            if let Some((last, short)) = e.words.split_last()
                && FLAVOR_SUFFIXES.contains(&last.as_str())
                && Entry::can_be_parent(short)
                && !all.contains(short)
            {
                names.entry(short.to_vec()).or_default().push(Name {
                    entry: i,
                    exact: false,
                });
            }
        }
        let longest = names.keys().map(Vec::len).max().unwrap_or(0);
        Self { names, longest }
    }

    /// Parent direct de `entries[i]`, s'il y en a un.
    fn parent(&self, entries: &[Entry], i: usize) -> Option<usize> {
        let e = &entries[i];
        let n = e.words.len();
        // (mots, nom exact, fin de la correspondance, dépôt) : le plus grand l'emporte.
        type Preference<'a> = (usize, bool, usize, Reverse<&'a str>);
        let mut best: Option<(Preference, usize)> = None;
        for start in 0..n {
            // Un parent a au moins deux mots, et un nom strictement plus court que la variante.
            let last = n.min(start + self.longest);
            for end in start + 2..=last {
                if end - start == n {
                    continue;
                }
                let Some(found) = self.names.get(&e.words[start..end]) else {
                    continue;
                };
                let next = e.words.get(end).map(String::as_str);
                if next.is_some_and(is_size) {
                    continue;
                }
                for name in found {
                    let p = &entries[name.entry];
                    if name.entry == i || p.org == e.org {
                        continue;
                    }
                    if !name.exact && next.is_some_and(|w| OFFICIAL_FLAVORS.contains(&w)) {
                        continue;
                    }
                    let key = (end - start, name.exact, end, Reverse(p.repo_id));
                    if best.as_ref().is_none_or(|(k, _)| key > *k) {
                        best = Some((key, name.entry));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

/// Mots d'un nom, en minuscules.
fn words(name: &str) -> Vec<String> {
    name.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// « 8 », « 3.5 ».
fn is_number(w: &str) -> bool {
    w.starts_with(|c: char| c.is_ascii_digit())
        && w.ends_with(|c: char| c.is_ascii_digit())
        && w.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Nombre de paramètres : « 27b », « 1.5b », « 1t », « 360m », actifs (« a3b »), effectifs
/// (« e4b »), experts (« 8x7b »). « 1m » est un contexte, pas une taille.
fn is_size(w: &str) -> bool {
    let w = w.strip_prefix(['a', 'e']).unwrap_or(w);
    let w = match w.split_once('x') {
        Some((n, rest)) if is_number(n) => rest,
        _ => w,
    };
    if let Some(n) = w.strip_suffix(['b', 't']) {
        is_number(n)
    } else if let Some(n) = w.strip_suffix('m') {
        is_number(n) && n.len() >= 2
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Pipeline;

    /// Un modèle réduit à ce qui compte ici : dépôt, modèle de base et nom affiché.
    fn model(repo_id: &str, base_model: Option<&str>) -> ModelEntry {
        let name = repo_id.split_once('/').unwrap().1.trim_end_matches("-GGUF");
        ModelEntry {
            repo_id: repo_id.into(),
            sha: String::new(),
            base_model: base_model.map(Into::into),
            display_name: name.into(),
            params_total: 8_000_000_000,
            params_active: None,
            arch: None,
            ctx_train: 32_768,
            pipeline: Pipeline::TextGeneration,
            use_cases: vec![],
            downloads: 0,
            likes: 0,
            created_at: None,
            last_modified: None,
            gated: false,
            variants: vec![],
            mmproj_bytes: None,
            alternatives: vec![],
        }
    }

    fn family_of<'a>(f: &HashMap<&str, &'a str>, repo: &str) -> Option<&'a str> {
        f.get(repo).copied()
    }

    #[test]
    fn mots_et_tailles() {
        assert_eq!(
            words("Qwen3.8-27B_Heretic v2"),
            ["qwen3.8", "27b", "heretic", "v2"]
        );
        for size in ["27b", "1.5b", "a3b", "e4b", "8x7b", "1t", "360m", "a2.5b"] {
            assert!(is_size(size), "{size}");
        }
        for word in ["1m", "qwen3", "v2", "it", "b", "3.5", "abliterated", "mtp"] {
            assert!(!is_size(word), "{word}");
        }
        assert!(is_number("3.5") && is_number("8") && !is_number("v4") && !is_number("3."));
    }

    #[test]
    fn variantes_rattachees_au_modele_officiel() {
        let models = [
            model("bartowski/Qwen3.8-27B-GGUF", Some("Qwen/Qwen3.8-27B")),
            model("DavidAU/Qwen3.8-27B-Ultimate-Heretic-Uncensored-GGUF", None),
            model(
                "huihui-ai/Huihui-Qwen3.8-27B-abliterated-GGUF",
                Some("huihui-ai/Huihui-Qwen3.8-27B-abliterated"),
            ),
            model("Qwen/Qwen3-8B-GGUF", Some("Qwen/Qwen3-8B")),
            model(
                "Qwen/Qwen3-VL-8B-Instruct-GGUF",
                Some("Qwen/Qwen3-VL-8B-Instruct"),
            ),
            model("unsloth/Qwen3.5-9B-GGUF", Some("Qwen/Qwen3.5-9B")),
            model("antirez/deepseek-v4-GGUF", None),
            model("Sakura721/DeepSeek-V4-Pro-Qwen3.5-9B-heretic-GGUF", None),
        ];
        let f = families(&models);
        let official = Some("bartowski/Qwen3.8-27B-GGUF");
        assert_eq!(
            family_of(&f, "DavidAU/Qwen3.8-27B-Ultimate-Heretic-Uncensored-GGUF"),
            official
        );
        assert_eq!(
            family_of(&f, "huihui-ai/Huihui-Qwen3.8-27B-abliterated-GGUF"),
            official
        );
        // Les modèles officiels n'ont pas de famille.
        assert_eq!(family_of(&f, "bartowski/Qwen3.8-27B-GGUF"), None);
        assert_eq!(family_of(&f, "Qwen/Qwen3-8B-GGUF"), None);
        // Mots non contigus : Qwen3-VL-8B n'est pas un Qwen3-8B.
        assert_eq!(family_of(&f, "Qwen/Qwen3-VL-8B-Instruct-GGUF"), None);
        // Deux parents possibles de même longueur : le plus à droite (le modèle distillé).
        assert_eq!(
            family_of(&f, "Sakura721/DeepSeek-V4-Pro-Qwen3.5-9B-heretic-GGUF"),
            Some("unsloth/Qwen3.5-9B-GGUF")
        );
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn la_correspondance_la_plus_longue_l_emporte() {
        let mut models = vec![
            model("Qwen/Qwen3.8-35B-GGUF", Some("Qwen/Qwen3.8-35B")),
            model("empero-ai/Qwen3.8-35B-A3B-Distill-GGUF", None),
        ];
        // Sans le MoE officiel, pas de famille : « a3b » suit le nom du modèle dense.
        assert!(families(&models).is_empty());
        models.push(model(
            "unsloth/Qwen3.8-35B-A3B-GGUF",
            Some("Qwen/Qwen3.8-35B-A3B"),
        ));
        assert_eq!(
            family_of(&families(&models), "empero-ai/Qwen3.8-35B-A3B-Distill-GGUF"),
            Some("unsloth/Qwen3.8-35B-A3B-GGUF")
        );
    }

    #[test]
    fn modeles_officiels_d_une_meme_organisation() {
        let models = [
            model("bartowski/Qwen2.5-14B-GGUF", Some("Qwen/Qwen2.5-14B")),
            model(
                "Qwen/Qwen2.5-14B-Instruct-GGUF",
                Some("Qwen/Qwen2.5-14B-Instruct"),
            ),
            model(
                "lmstudio-community/Qwen2.5-14B-Instruct-1M-GGUF",
                Some("Qwen/Qwen2.5-14B-Instruct-1M"),
            ),
            model("microsoft/phi-4-GGUF", Some("microsoft/phi-4")),
            model(
                "unsloth/Phi-4-mini-instruct-GGUF",
                Some("microsoft/Phi-4-mini-instruct"),
            ),
            model(
                "bartowski/Qwen2.5-14B-Instruct-abliterated-GGUF",
                Some("someone/Qwen2.5-14B-Instruct-abliterated"),
            ),
        ];
        let f = families(&models);
        assert_eq!(f.len(), 1, "{f:?}");
        // Une variante tierce de l'Instruct remonte à lui, pas au modèle de base.
        assert_eq!(
            family_of(&f, "bartowski/Qwen2.5-14B-Instruct-abliterated-GGUF"),
            Some("Qwen/Qwen2.5-14B-Instruct-GGUF")
        );
    }

    #[test]
    fn variante_d_une_variante_et_suffixes() {
        let models = [
            model("Qwen/Qwen3-8B-GGUF", Some("Qwen/Qwen3-8B")),
            model(
                "mlabonne/Qwen3-8B-abliterated-GGUF",
                Some("mlabonne/Qwen3-8B-abliterated"),
            ),
            model("someone/Qwen3-8B-abliterated-v2-GGUF", None),
            model("unsloth/gemma-4-31B-it-GGUF", Some("google/gemma-4-31B-it")),
            model("LessThanThreeAI/Gemma-4-31B-Humanlike-Chat-GGUF", None),
            model(
                "Qwen/Qwen3-VL-8B-Instruct-GGUF",
                Some("Qwen/Qwen3-VL-8B-Instruct"),
            ),
            model(
                "unsloth/Qwen3-VL-8B-Thinking-1M-GGUF",
                Some("unsloth/Qwen3-VL-8B-Thinking-1M"),
            ),
            model("TheBloke/CausalLM-7B-GGUF", Some("CausalLM/7B")),
            model(
                "Qwen/Qwen2.5-7B-Instruct-GGUF",
                Some("Qwen/Qwen2.5-7B-Instruct"),
            ),
        ];
        let mut models = models.to_vec();
        // Nom affiché réduit à « 7B » par le dédoublonnage.
        models[7].display_name = "7B".into();
        let f = families(&models);
        // Famille plate : la v2 remonte au modèle officiel.
        assert_eq!(
            family_of(&f, "someone/Qwen3-8B-abliterated-v2-GGUF"),
            Some("Qwen/Qwen3-8B-GGUF")
        );
        assert_eq!(
            family_of(&f, "mlabonne/Qwen3-8B-abliterated-GGUF"),
            Some("Qwen/Qwen3-8B-GGUF")
        );
        // Nom sans « -it ».
        assert_eq!(
            family_of(&f, "LessThanThreeAI/Gemma-4-31B-Humanlike-Chat-GGUF"),
            Some("unsloth/gemma-4-31B-it-GGUF")
        );
        // « Thinking » est une déclinaison sœur de « Instruct ».
        assert_eq!(family_of(&f, "unsloth/Qwen3-VL-8B-Thinking-1M-GGUF"), None);
        // « 7B » seul ne fait pas une famille.
        assert_eq!(family_of(&f, "Qwen/Qwen2.5-7B-Instruct-GGUF"), None);
        assert_eq!(f.len(), 3, "{f:?}");
    }

    #[test]
    fn independant_de_l_ordre_du_catalogue() {
        let models = vec![
            model("a/Qwen3-8B-GGUF", Some("Qwen/Qwen3-8B")),
            model("b/Qwen3-8B-GGUF", Some("Qwen/Qwen3-8B")),
            model("c/Qwen3-8B-heretic-GGUF", None),
        ];
        let mut reversed = models.clone();
        reversed.reverse();
        let (f, g) = (families(&models), families(&reversed));
        assert_eq!(f, g);
        // À égalité parfaite, le premier dépôt dans l'ordre alphabétique.
        assert_eq!(
            family_of(&f, "c/Qwen3-8B-heretic-GGUF"),
            Some("a/Qwen3-8B-GGUF")
        );
    }
}
