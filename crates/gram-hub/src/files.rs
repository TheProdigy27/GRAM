//! Analyse des fichiers d'un dépôt GGUF (§7.6 du plan) : une [`QuantVariant`] par
//! quantification, avec sa taille réelle, plus la taille du projecteur vision (`mmproj`).

use gram_core::QuantVariant;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Un fichier du dépôt (`siblings[]` du détail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RepoFiles {
    /// De la plus petite à la plus grosse.
    pub variants: Vec<QuantVariant>,
    pub mmproj: Option<Mmproj>,
    /// Quantifications écartées pour des bits par poids hors de [1, 40].
    pub rejected: usize,
}

impl RepoFiles {
    /// Plus de fichiers écartés que gardés : c'est le nombre de paramètres annoncé par
    /// Hugging Face qui est faux (calculé sur un petit fichier auxiliaire, par exemple).
    pub fn params_suspect(&self) -> bool {
        self.rejected > self.variants.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mmproj {
    pub file: String,
    pub size: u64,
}

/// Bits par poids plausibles. En dehors : fichier auxiliaire (modèle de brouillon, têtes MTP…)
/// ou dépôt qui mélange plusieurs modèles.
const BPW_RANGE: std::ops::RangeInclusive<f32> = 1.0..=40.0;

/// Préfixes de fichiers qui ne sont pas le modèle lui-même : modèles de brouillon (`dflash-`,
/// `draft-`), têtes de prédiction multi-tokens (`mtp-`), matrices d'importance, vocabulaires.
const AUX_PREFIXES: &[&str] = &[
    "mtp-",
    "dflash-",
    "draft-",
    "eagle",
    "ggml-vocab",
    "imatrix",
];

/// Mots qui désignent un modèle de brouillon (décodage spéculatif) où qu'ils soient dans le nom
/// (« Ternary-Bonsai-2-9B-DFlash2-Q8_0.gguf »).
const AUX_TOKENS: &[&str] = &["dflash", "dspine", "draft", "eagle3"];

fn is_auxiliary(basename_lower: &str) -> bool {
    AUX_PREFIXES.iter().any(|p| basename_lower.starts_with(p))
        || basename_lower.contains("imatrix")
        || basename_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|t| {
                AUX_TOKENS
                    .iter()
                    .any(|a| t.starts_with(a) && t[a.len()..].bytes().all(|b| b.is_ascii_digit()))
            })
}

fn quant_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:UD-)?(?:IQ[1-4]_(?:XXS|XS|NL|S|M)|Q[1-8]_K(?:_(?:XXS|XS|XL|S|M|L))?|Q[4-8]_[01](?:_[48]_[48])?|TQ[12]_0|MXFP4(?:_MOE)?|BF16|FP16|F16|FP32|F32)",
        )
        .expect("regex de quantification valide")
    })
}

fn split_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(.*?)[-_.]?(\d{5})-of-(\d{5})\.gguf$").expect("regex de découpage")
    })
}

fn part_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(.*)\.gguf\.part(\d+)of(\d+)$").expect("regex des parties"))
}

fn is_sep(b: u8) -> bool {
    matches!(b, b'-' | b'_' | b'.' | b' ' | b'/')
}

/// Libellé de quantification contenu dans un nom de fichier ou de dossier (« Q4_K_M »,
/// « UD-Q4_K_XL », « IQ2_XXS », « BF16 »…). Le dernier libellé délimité l'emporte : les noms
/// de modèle viennent avant la quantification.
pub fn quant_label(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let re = quant_re();
    let mut best = None;
    let mut pos = 0;
    while let Some(m) = re.find_at(name, pos) {
        let before = m.start() == 0 || is_sep(bytes[m.start() - 1]);
        let after = m.end() == bytes.len() || is_sep(bytes[m.end()]);
        if before && after {
            best = Some(m.as_str());
            pos = m.end();
        } else {
            pos = m.start() + 1;
        }
    }
    best.map(|l| {
        let up = l.to_ascii_uppercase();
        match up.as_str() {
            "FP16" => "F16".to_owned(),
            "FP32" => "F32".to_owned(),
            _ => up,
        }
    })
}

/// Retire un libellé de quantification final et son séparateur (« gemma-4-e4b-Q4_K_M » →
/// « gemma-4-e4b »), pour les dépôts qui ne contiennent qu'une quantification.
pub(crate) fn strip_quant_suffix(name: &str) -> &str {
    let bytes = name.as_bytes();
    let mut pos = 0;
    while let Some(m) = quant_re().find_at(name, pos) {
        if m.end() == bytes.len() && m.start() > 0 && is_sep(bytes[m.start() - 1]) {
            return &name[..m.start() - 1];
        }
        pos = m.start() + 1;
    }
    name
}

/// Fichier GGUF : (racine commune des parties, numéro de partie, nombre de parties).
fn gguf_stem(basename: &str) -> Option<(String, u32, u32)> {
    if let Some(c) = split_re().captures(basename) {
        let n = c[2].parse().ok()?;
        let total = c[3].parse().ok()?;
        return Some((c[1].to_owned(), n, total));
    }
    if let Some(c) = part_re().captures(basename) {
        let n = c[2].parse().ok()?;
        let total = c[3].parse().ok()?;
        return Some((c[1].to_owned(), n, total));
    }
    let lower = basename.to_ascii_lowercase();
    lower
        .ends_with(".gguf")
        .then(|| (basename[..basename.len() - 5].to_owned(), 1, 1))
}

/// Préférence pour le projecteur vision : F16 d'abord (§7.6).
fn mmproj_rank(label: Option<&str>) -> u8 {
    match label {
        Some("F16") => 0,
        Some("BF16") => 1,
        Some("Q8_0") => 2,
        Some("F32") => 3,
        _ => 4,
    }
}

struct Group {
    label: String,
    parts: Vec<(u32, String, u64)>,
    total_parts: u32,
}

/// Regroupe les fichiers GGUF du dépôt `repo_id` par quantification. `params_total` sert au
/// calcul des bits par poids (`taille × 8 / paramètres`) ; 0 si inconnu.
pub fn analyze(repo_id: &str, files: &[RepoFile], params_total: u64) -> RepoFiles {
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    let mut mmprojs: Vec<(u8, &RepoFile)> = Vec::new();

    for f in files {
        let (dir, basename) = match f.path.rsplit_once('/') {
            Some((d, b)) => (d, b),
            None => ("", f.path.as_str()),
        };
        let Some((stem, part, total_parts)) = gguf_stem(basename) else {
            continue;
        };
        let lower = basename.to_ascii_lowercase();
        if lower.contains("mmproj") {
            mmprojs.push((mmproj_rank(quant_label(&stem).as_deref()), f));
            continue;
        }
        if is_auxiliary(&lower) {
            continue;
        }
        let dir_name = dir.rsplit('/').next().unwrap_or(dir);
        // Quantification au nom exotique : le nom du fichier sert de libellé, les bits par
        // poids réels suffisent au calcul.
        let label = quant_label(&stem)
            .or_else(|| quant_label(dir_name))
            .unwrap_or_else(|| stem.clone());
        let g = groups
            .entry((dir.to_owned(), stem.to_ascii_lowercase()))
            .or_insert_with(|| Group {
                label,
                parts: Vec::new(),
                total_parts,
            });
        g.parts.push((part, f.path.clone(), f.size));
    }

    let mut variants: Vec<QuantVariant> = Vec::new();
    let mut rejected = 0;
    for ((dir, stem), mut g) in groups {
        g.parts.sort();
        let complete = g.parts.len() == g.total_parts as usize
            && g.parts.iter().zip(1..).all(|((n, _, _), i)| *n == i);
        if !complete {
            tracing::debug!(dir, stem, "fichiers découpés incomplets, ignorés");
            continue;
        }
        let size_bytes: u64 = g.parts.iter().map(|(_, _, s)| s).sum();
        let bpw = if params_total > 0 {
            (size_bytes as f64 * 8.0 / params_total as f64) as f32
        } else {
            0.0
        };
        if params_total > 0 && !BPW_RANGE.contains(&bpw) {
            tracing::debug!(dir, stem, bpw, "bits par poids incohérents, fichier ignoré");
            rejected += 1;
            continue;
        }
        if variants.iter().any(|v| v.label == g.label) {
            tracing::debug!(
                dir,
                stem,
                label = g.label,
                "quantification en double, ignorée"
            );
            continue;
        }
        variants.push(QuantVariant {
            label: g.label,
            repo_id: repo_id.to_owned(),
            files: g.parts.into_iter().map(|(_, p, _)| p).collect(),
            size_bytes,
            bpw,
            exact: true,
        });
    }
    variants.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes).then(a.label.cmp(&b.label)));

    mmprojs.sort_by_key(|(rank, f)| (*rank, f.size));
    let mmproj = mmprojs.first().map(|(_, f)| Mmproj {
        file: f.path.clone(),
        size: f.size,
    });
    RepoFiles {
        variants,
        mmproj,
        rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiModel;
    use crate::test_support::fixture;

    const REPO: &str = "test/repo-GGUF";

    fn load(name: &str) -> (Vec<RepoFile>, u64) {
        let m: ApiModel =
            serde_json::from_slice(&fixture(&format!("details/{name}.json"))).unwrap();
        let files = m
            .siblings
            .iter()
            .filter_map(|s| {
                Some(RepoFile {
                    path: s.rfilename.clone(),
                    size: s.size()?,
                })
            })
            .collect();
        (files, m.params_total().unwrap())
    }

    fn labels(r: &RepoFiles) -> Vec<&str> {
        r.variants.iter().map(|v| v.label.as_str()).collect()
    }

    #[test]
    fn libelles() {
        let cases = [
            ("Qwen_Qwen3-8B-Q4_K_M", Some("Q4_K_M")),
            ("Qwen_Qwen3-8B-IQ3_XXS", Some("IQ3_XXS")),
            ("Qwen_Qwen3-8B-IQ3_XS", Some("IQ3_XS")),
            ("Qwen_Qwen3-8B-Q3_K_XL", Some("Q3_K_XL")),
            ("Qwen_Qwen3-8B-bf16", Some("BF16")),
            ("Qwen3.5-9B-UD-Q4_K_XL", Some("UD-Q4_K_XL")),
            ("Qwen3.5-9B-UD-IQ2_XXS", Some("UD-IQ2_XXS")),
            ("qwen2.5-7b-instruct-q4_k_m", Some("Q4_K_M")),
            ("qwen2.5-7b-instruct-fp16", Some("F16")),
            ("Llama-3.3-70B-Instruct-abliterated.IQ4_XS", Some("IQ4_XS")),
            ("Model.i1-Q4_K_M", Some("Q4_K_M")),
            ("gpt-oss-20b-mxfp4", Some("MXFP4")),
            ("gpt-oss-120b-MXFP4_MOE", Some("MXFP4_MOE")),
            ("Ornith-1.5-122B-UD-TQ1_0", Some("UD-TQ1_0")),
            ("model-Q4_0_4_4", Some("Q4_0_4_4")),
            ("Qwen3-30B-A3B-Q6_K", Some("Q6_K")),
            // Pas de faux positif dans les noms de modèle.
            ("Qwen3-30B-A3B", None),
            ("Phi-4-mini-instruct", None),
            ("model", None),
        ];
        for (name, want) in cases {
            assert_eq!(quant_label(name).as_deref(), want, "{name}");
        }
    }

    #[test]
    fn auxiliaires() {
        assert!(is_auxiliary("mtp-qwen3.6-35b-a3b-q8_0.gguf"));
        assert!(is_auxiliary("dflash-qwen3.6-35b-a3b-bf16.gguf"));
        assert!(is_auxiliary("ternary-bonsai-2-9b-dflash2-q8_0.gguf"));
        assert!(is_auxiliary("model-draft-q4_0.gguf"));
        assert!(!is_auxiliary("qwen3.5-9b-ud-q4_k_xl.gguf"));
        // « MTP » au milieu du nom : modèle avec têtes MTP intégrées, pas un fichier auxiliaire.
        assert!(!is_auxiliary("qwen3.5-9b-max-mtp-q4_k_m.gguf"));
        // Mot qui commence seulement par « draft ».
        assert!(!is_auxiliary("drafting-assistant-q4_k_m.gguf"));
    }

    #[test]
    fn bartowski() {
        let (files, params) = load("bartowski--Qwen_Qwen3-8B-GGUF");
        let r = analyze(REPO, &files, params);
        assert_eq!(r.variants.len(), 24);
        assert!(r.mmproj.is_none());
        let first = &r.variants[0];
        assert_eq!(first.label, "IQ2_M");
        assert_eq!(first.size_bytes, 3_051_914_784);
        assert_eq!(first.files, vec!["Qwen_Qwen3-8B-IQ2_M.gguf"]);
        assert_eq!(first.repo_id, REPO);
        assert!(first.exact);
        assert!((first.bpw - 2.98).abs() < 0.01, "{}", first.bpw);
        let q4 = r.variants.iter().find(|v| v.label == "Q4_K_M").unwrap();
        assert!((q4.bpw - 4.91).abs() < 0.01, "{}", q4.bpw);
        assert_eq!(r.variants.last().unwrap().label, "BF16");
        // Triées par taille.
        assert!(
            r.variants
                .windows(2)
                .all(|w| w[0].size_bytes <= w[1].size_bytes)
        );
    }

    #[test]
    fn unsloth_ud_et_mmproj() {
        let (files, params) = load("unsloth--Qwen3.5-9B-GGUF");
        let r = analyze(REPO, &files, params);
        let l = labels(&r);
        assert_eq!(r.variants.len(), 22, "{l:?}");
        assert!(l.contains(&"UD-Q4_K_XL") && l.contains(&"Q4_K_M") && l.contains(&"BF16"));
        // `imatrix_unsloth.gguf_file` et les mmproj ne sont pas des variantes.
        assert!(
            !l.iter()
                .any(|x| x.contains("imatrix") || x.contains("mmproj"))
        );
        assert_eq!(
            r.mmproj,
            Some(Mmproj {
                file: "mmproj-F16.gguf".into(),
                size: 918_166_080
            })
        );
    }

    #[test]
    fn sous_dossiers_et_fichiers_decoupes() {
        let (files, params) = load("unsloth--Ornith-1.5-122B-GGUF");
        let r = analyze(REPO, &files, params);
        assert_eq!(
            labels(&r),
            vec![
                "UD-IQ1_M",
                "UD-IQ2_XXS",
                "UD-Q2_K_XL",
                "UD-IQ3_XXS",
                "UD-Q3_K_XL",
                "UD-Q4_K_XL",
                "UD-Q5_K_XL",
                "UD-Q6_K_XL",
                "Q8_0",
                "BF16"
            ]
        );
        let q4 = r.variants.iter().find(|v| v.label == "UD-Q4_K_XL").unwrap();
        assert_eq!(
            q4.files,
            vec![
                "UD-Q4_K_XL/Ornith-1.5-122B-UD-Q4_K_XL-00001-of-00003.gguf",
                "UD-Q4_K_XL/Ornith-1.5-122B-UD-Q4_K_XL-00002-of-00003.gguf",
                "UD-Q4_K_XL/Ornith-1.5-122B-UD-Q4_K_XL-00003-of-00003.gguf"
            ]
        );
        assert_eq!(q4.size_bytes, 10_942_929 + 49_971_099_680 + 29_731_729_056);
        assert_eq!(r.mmproj.as_ref().unwrap().file, "mmproj-F16.gguf");
    }

    #[test]
    fn decoupage_en_minuscules() {
        let (files, params) = load("Qwen--Qwen2.5-7B-Instruct-GGUF");
        let r = analyze(REPO, &files, params);
        assert_eq!(
            labels(&r),
            vec![
                "Q2_K", "Q3_K_M", "Q4_0", "Q4_K_M", "Q5_0", "Q5_K_M", "Q6_K", "Q8_0", "F16"
            ]
        );
        let f16 = r.variants.last().unwrap();
        assert_eq!(f16.files.len(), 4);
        assert_eq!(
            f16.size_bytes,
            3_951_521_376 + 3_864_909_312 + 3_864_894_976 + 3_556_527_872
        );
    }

    #[test]
    fn parties_mradermacher() {
        let (files, params) = load("mradermacher--Llama-3.3-70B-Instruct-abliterated-GGUF");
        let r = analyze(REPO, &files, params);
        assert_eq!(r.variants.len(), 11);
        let q8 = r.variants.iter().find(|v| v.label == "Q8_0").unwrap();
        assert_eq!(
            q8.files,
            vec![
                "Llama-3.3-70B-Instruct-abliterated.Q8_0.gguf.part1of2",
                "Llama-3.3-70B-Instruct-abliterated.Q8_0.gguf.part2of2"
            ]
        );
        assert_eq!(q8.size_bytes, 37_580_963_840 + 37_394_091_264);
    }

    #[test]
    fn fichiers_auxiliaires_ecartes() {
        let (files, params) = load("ggml-org--Qwen3.6-35B-A3B-GGUF");
        let r = analyze(REPO, &files, params);
        // `dflash-…` (brouillon) et `mtp-…` (têtes MTP) ne sont pas des quantifications du modèle.
        assert_eq!(labels(&r), vec!["Q4_K_M", "Q8_0", "BF16"]);
        assert_eq!(r.variants[0].size_bytes, 20_542_732_576);
        // Pas de F16 : BF16 est préféré à Q8_0.
        assert_eq!(r.mmproj.unwrap().file, "mmproj-Qwen3.6-35B-A3B-BF16.gguf");
    }

    #[test]
    fn mmproj_ggml_org() {
        let (files, params) = load("ggml-org--gemma-3-4b-it-GGUF");
        let r = analyze(REPO, &files, params);
        assert_eq!(labels(&r), vec!["Q4_K_M", "Q8_0", "F16"]);
        assert_eq!(r.mmproj.unwrap().size, 851_251_104);
    }

    #[test]
    fn cas_limites() {
        let f = |path: &str, size: u64| RepoFile {
            path: path.into(),
            size,
        };
        let files = vec![
            // Partie manquante : ignorée.
            f("m-Q4_K_M-00001-of-00002.gguf", 4_000_000_000),
            // Nom exotique : le nom sert de libellé.
            f("model-custom.gguf", 5_000_000_000),
            // Trop petit pour être le modèle (0,4 bit par poids).
            f("m-Q8_0-extra.gguf", 400_000_000),
            f("README.md", 10),
            f("imatrix.gguf", 5_000_000),
            // Brouillon pour le décodage spéculatif, même sans préfixe.
            f("Bonsai-9B-DFlash2-Q8_0.gguf", 6_000_000_000),
        ];
        let r = analyze(REPO, &files, 8_000_000_000);
        assert_eq!(labels(&r), vec!["model-custom"]);
        assert!((r.variants[0].bpw - 5.0).abs() < 1e-3);
        assert_eq!(r.rejected, 1);
        assert!(!r.params_suspect());
        // Cas réel (HauhauCS/Qwen3.8-27B-…-MTP-GGUF) : Hugging Face annonce 1,86 G paramètres,
        // lus sur le petit fichier MTP ; les vraies quantifications dépassent 40 bits par poids.
        let files = vec![
            f("Q-27B-FastMTP-32K.gguf", 903_453_952),
            f("Q-27B-IQ2_M.gguf", 10_300_000_000),
            f("Q-27B-Q4_K_P.gguf", 17_900_000_000),
            f("Q-27B-Q8_K_P.gguf", 31_400_000_000),
        ];
        let r = analyze(REPO, &files, 1_863_907_840);
        assert_eq!(r.variants.len(), 1);
        assert!(r.params_suspect());
        // Paramètres inconnus : pas de contrôle des bits par poids.
        let r = analyze(REPO, &[f("x-Q4_K_M.gguf", 10)], 0);
        assert_eq!(r.variants[0].bpw, 0.0);
    }
}
