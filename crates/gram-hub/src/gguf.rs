//! Parseur d'en-tête GGUF limité aux métadonnées (§7.4 du plan).
//!
//! Format (little-endian) : magic `GGUF`, version (2 ou 3), nombre de tenseurs (u64), nombre
//! de paires clé/valeur (u64), puis les paires typées. Les tenseurs ne sont pas lus.
//!
//! On ne télécharge que le début du fichier : le parseur accepte des données tronquées et dit
//! s'il a déjà les clés utiles ([`Parsed::Partial`]) ou s'il faut relire plus loin
//! ([`Parsed::NeedMore`]). Dans les fichiers produits par llama.cpp, les clés d'architecture
//! précèdent les tableaux du tokenizer (plusieurs Mo) : les 64 premiers Kio suffisent presque
//! toujours.

use gram_core::{ArchInfo, ArchSource};

const MAGIC: &[u8; 4] = b"GGUF";
/// Longueur maximale d'une chaîne ou d'un tableau : au-delà, le fichier est corrompu.
const MAX_LEN: u64 = 1 << 28;
/// Tableaux d'entiers conservés (valeurs par couche) ; les autres sont sautés.
const MAX_KEPT_ARRAY: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GgufError {
    #[error("ce n'est pas un fichier GGUF")]
    NotGguf,
    #[error("version GGUF {0} non prise en charge")]
    UnsupportedVersion(u32),
    #[error("en-tête GGUF corrompu ({0})")]
    Malformed(&'static str),
}

/// Valeurs utiles de l'en-tête. Les clés `{arch}.*` sont lues pour l'architecture déclarée par
/// `general.architecture`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GgufMeta {
    pub architecture: Option<String>,
    pub block_count: Option<u32>,
    pub context_length: Option<u64>,
    pub embedding_length: Option<u32>,
    pub feed_forward_length: Option<u32>,
    pub expert_feed_forward_length: Option<u32>,
    /// Maximum si la valeur est donnée par couche.
    pub head_count: Option<u32>,
    pub head_count_kv: Option<u32>,
    /// Couches dont `head_count_kv` est non nul, quand la valeur est donnée par couche.
    pub kv_layers_from_array: Option<u32>,
    pub key_length: Option<u32>,
    pub expert_count: Option<u32>,
    pub expert_used_count: Option<u32>,
    pub leading_dense_block_count: Option<u32>,
    pub interleave_moe_layer_step: Option<u32>,
    pub sliding_window: Option<u32>,
    pub full_attention_interval: Option<u32>,
    /// Couches récurrentes (sans cache KV), d'après `attention.recurrent_layers`.
    pub recurrent_layers: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    /// Toutes les métadonnées ont été lues.
    Complete(GgufMeta),
    /// Données tronquées, mais les clés indispensables sont là.
    Partial(GgufMeta),
    /// Données tronquées avant les clés indispensables : relire plus loin.
    NeedMore,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Uint(u64),
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// Tableau d'entiers ou de booléens (`true` = 1).
    Ints(Vec<i64>),
    Skipped,
}

impl Value {
    fn as_u64(&self) -> Option<u64> {
        match *self {
            Value::Uint(u) => Some(u),
            Value::Int(i) => u64::try_from(i).ok(),
            _ => None,
        }
    }

    fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|v| u32::try_from(v).ok())
    }

    /// Entier, ou maximum d'un tableau d'entiers par couche.
    fn as_u32_or_max(&self) -> Option<u32> {
        match self {
            Value::Ints(v) => v.iter().copied().max().and_then(|m| u32::try_from(m).ok()),
            other => other.as_u32(),
        }
    }
}

enum Error {
    Truncated,
    Gguf(GgufError),
}

impl From<GgufError> for Error {
    fn from(e: GgufError) -> Self {
        Error::Gguf(e)
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: u64) -> Result<&'a [u8], Error> {
        let n = usize::try_from(n).map_err(|_| Error::Truncated)?;
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.take(N as u64)?.try_into().expect("taille exacte"))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn len(&mut self) -> Result<u64, Error> {
        let n = self.u64()?;
        if n > MAX_LEN {
            return Err(GgufError::Malformed("longueur aberrante").into());
        }
        Ok(n)
    }

    fn string(&mut self) -> Result<String, Error> {
        let n = self.len()?;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    /// Taille fixe des types scalaires (`None` pour les chaînes et les tableaux).
    fn scalar_size(ty: u32) -> Result<Option<u64>, Error> {
        Ok(match ty {
            0 | 1 | 7 => Some(1),
            2 | 3 => Some(2),
            4..=6 => Some(4),
            10..=12 => Some(8),
            8 | 9 => None,
            _ => return Err(GgufError::Malformed("type de valeur inconnu").into()),
        })
    }

    fn scalar(&mut self, ty: u32) -> Result<Value, Error> {
        let le = |b: &[u8]| {
            let mut buf = [0u8; 8];
            buf[..b.len()].copy_from_slice(b);
            u64::from_le_bytes(buf)
        };
        let size = Self::scalar_size(ty)?.expect("type scalaire");
        let raw = le(self.take(size)?);
        Ok(match ty {
            0 | 2 | 4 | 10 => Value::Uint(raw),
            1 => Value::Int(i64::from(raw as u8 as i8)),
            3 => Value::Int(i64::from(raw as u16 as i16)),
            5 => Value::Int(i64::from(raw as u32 as i32)),
            11 => Value::Int(raw as i64),
            6 => Value::Float(f64::from(f32::from_bits(raw as u32))),
            12 => Value::Float(f64::from_bits(raw)),
            7 => Value::Bool(raw != 0),
            _ => unreachable!("types scalaires uniquement"),
        })
    }

    fn value(&mut self, ty: u32, depth: u32) -> Result<Value, Error> {
        match ty {
            8 => Ok(Value::Str(self.string()?)),
            9 => {
                if depth > 4 {
                    return Err(GgufError::Malformed("tableaux trop imbriqués").into());
                }
                let elem = self.u32()?;
                let count = self.len()?;
                match Self::scalar_size(elem)? {
                    Some(_) if count <= MAX_KEPT_ARRAY && elem != 6 && elem != 12 => {
                        let mut out = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            out.push(match self.scalar(elem)? {
                                Value::Uint(u) => i64::try_from(u).unwrap_or(i64::MAX),
                                Value::Int(i) => i,
                                Value::Bool(b) => i64::from(b),
                                _ => 0,
                            });
                        }
                        Ok(Value::Ints(out))
                    }
                    Some(size) => {
                        self.take(size.checked_mul(count).ok_or(Error::Truncated)?)?;
                        Ok(Value::Skipped)
                    }
                    None => {
                        for _ in 0..count {
                            self.value(elem, depth + 1)?;
                        }
                        Ok(Value::Skipped)
                    }
                }
            }
            _ => self.scalar(ty),
        }
    }
}

/// Lit les métadonnées d'un début de fichier GGUF.
pub fn parse(data: &[u8]) -> Result<Parsed, GgufError> {
    if data.len() >= 4 && &data[..4] != MAGIC {
        return Err(GgufError::NotGguf);
    }
    let mut r = Reader { data, pos: 0 };
    let mut pairs: Vec<(String, Value)> = Vec::new();
    let complete = match read_pairs(&mut r, &mut pairs) {
        Ok(()) => true,
        Err(Error::Truncated) => false,
        Err(Error::Gguf(e)) => return Err(e),
    };
    let meta = extract(&pairs);
    Ok(if complete {
        Parsed::Complete(meta)
    } else if meta.has_required() {
        Parsed::Partial(meta)
    } else {
        Parsed::NeedMore
    })
}

fn read_pairs(r: &mut Reader<'_>, pairs: &mut Vec<(String, Value)>) -> Result<(), Error> {
    r.take(4)?; // magic
    let version = r.u32()?;
    if !(2..=3).contains(&version) {
        // v1 (obsolète) ou fichier big-endian.
        return Err(GgufError::UnsupportedVersion(version).into());
    }
    let _tensors = r.u64()?;
    let kv_count = r.u64()?;
    for _ in 0..kv_count {
        let key = r.string()?;
        let ty = r.u32()?;
        let value = r.value(ty, 0)?;
        if !key.starts_with("tokenizer.") {
            pairs.push((key, value));
        }
    }
    Ok(())
}

fn extract(pairs: &[(String, Value)]) -> GgufMeta {
    let architecture = pairs.iter().find_map(|(k, v)| match v {
        Value::Str(s) if k == "general.architecture" => Some(s.clone()),
        _ => None,
    });
    let mut m = GgufMeta {
        architecture,
        ..GgufMeta::default()
    };
    let Some(arch) = m.architecture.clone() else {
        return m;
    };
    let prefix = format!("{arch}.");
    for (key, v) in pairs {
        let Some(suffix) = key.strip_prefix(&prefix) else {
            continue;
        };
        match suffix {
            "block_count" => m.block_count = v.as_u32(),
            "context_length" => m.context_length = v.as_u64(),
            "embedding_length" => m.embedding_length = v.as_u32(),
            "feed_forward_length" => m.feed_forward_length = v.as_u32_or_max(),
            "expert_feed_forward_length" => m.expert_feed_forward_length = v.as_u32(),
            "attention.head_count" => m.head_count = v.as_u32_or_max(),
            "attention.head_count_kv" => {
                m.head_count_kv = v.as_u32_or_max();
                if let Value::Ints(per_layer) = v {
                    m.kv_layers_from_array =
                        u32::try_from(per_layer.iter().filter(|&&n| n > 0).count()).ok();
                }
            }
            "attention.key_length" => m.key_length = v.as_u32(),
            "expert_count" => m.expert_count = v.as_u32(),
            "expert_used_count" => m.expert_used_count = v.as_u32(),
            "leading_dense_block_count" => m.leading_dense_block_count = v.as_u32(),
            "interleave_moe_layer_step" => m.interleave_moe_layer_step = v.as_u32(),
            "attention.sliding_window" => m.sliding_window = v.as_u32(),
            "full_attention_interval" => m.full_attention_interval = v.as_u32(),
            "attention.recurrent_layers" => {
                if let Value::Ints(flags) = v {
                    m.recurrent_layers =
                        u32::try_from(flags.iter().filter(|&&f| f != 0).count()).ok();
                }
            }
            _ => {}
        }
    }
    m
}

impl GgufMeta {
    /// Clés sans lesquelles le cache KV ne peut pas être calculé.
    fn has_required(&self) -> bool {
        self.architecture.is_some()
            && self.block_count.is_some()
            && self.head_count.is_some()
            && self.head_count_kv.is_some()
            && (self.key_length.is_some() || self.embedding_length.is_some())
    }

    /// Métadonnées d'architecture pour le calcul du cache KV.
    pub fn arch_info(&self) -> Option<ArchInfo> {
        let n_layers = self.block_count.filter(|&n| n > 0)?;
        let n_heads = self.head_count.filter(|&n| n > 0)?;
        let head_dim = self
            .key_length
            .or_else(|| self.embedding_length.map(|e| e / n_heads))
            .filter(|&d| d > 0)?;
        let kv_layers = self
            .kv_layers_from_array
            .or_else(|| self.recurrent_layers.map(|r| n_layers.saturating_sub(r)))
            .or_else(|| {
                self.full_attention_interval
                    .filter(|&i| i > 1)
                    .map(|i| n_layers / i)
            })
            .filter(|&k| k < n_layers);
        Some(ArchInfo {
            architecture: self.architecture.clone()?,
            n_layers,
            n_heads,
            // Absent : attention multi-têtes classique (autant de têtes KV que de têtes).
            n_kv_heads: self.head_count_kv.unwrap_or(n_heads),
            head_dim,
            kv_layers,
            embedding_length: self.embedding_length,
            expert_count: self.expert_count.filter(|&n| n > 0),
            expert_used_count: self.expert_used_count.filter(|&n| n > 0),
            sliding_window: self.sliding_window.filter(|&n| n > 0),
            source: ArchSource::GgufHeader,
        })
    }

    /// Paramètres actifs d'un MoE : on retire les experts inactifs de chaque couche MoE
    /// (3 matrices `embedding × ffn_expert` par expert). `None` pour un modèle dense ou si
    /// le résultat n'est pas plausible.
    pub fn params_active(&self, params_total: u64) -> Option<u64> {
        let experts = u64::from(self.expert_count?);
        let used = u64::from(self.expert_used_count?);
        if used == 0 || experts <= used {
            return None;
        }
        let ff = u64::from(
            self.expert_feed_forward_length
                .or(self.feed_forward_length)?,
        );
        let emb = u64::from(self.embedding_length?);
        let layers = u64::from(self.block_count?);
        let dense = u64::from(self.leading_dense_block_count.unwrap_or(0));
        let step = u64::from(self.interleave_moe_layer_step.unwrap_or(1).max(1));
        let moe_layers = layers.saturating_sub(dense) / step;
        let inactive = moe_layers * (experts - used) * 3 * emb * ff;
        let active = params_total.checked_sub(inactive)?;
        // Moins de 2 % de paramètres actifs : formule inadaptée à cette architecture.
        (active * 50 >= params_total).then_some(active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    fn header(name: &str) -> Vec<u8> {
        fixture(&format!("gguf/{name}.bin"))
    }

    fn partial(name: &str) -> GgufMeta {
        match parse(&header(name)).unwrap() {
            Parsed::Partial(m) => m,
            other => panic!("{name} : {other:?}"),
        }
    }

    #[test]
    fn qwen3_dense() {
        // Les 4 Kio enregistrés s'arrêtent dans le tokenizer : en-tête partiel mais suffisant.
        let m = partial("Qwen--Qwen3-8B-GGUF--Qwen3-8B-Q4_K_M");
        assert_eq!(m.context_length, Some(40960));
        let a = m.arch_info().unwrap();
        assert_eq!(
            a,
            ArchInfo {
                architecture: "qwen3".into(),
                n_layers: 36,
                n_heads: 32,
                n_kv_heads: 8,
                head_dim: 128,
                kv_layers: None,
                embedding_length: Some(4096),
                expert_count: None,
                expert_used_count: None,
                sliding_window: None,
                source: ArchSource::GgufHeader,
            }
        );
        assert_eq!(m.params_active(8_190_735_360), None);
    }

    #[test]
    fn qwen35_hybride() {
        // Une couche sur 4 seulement a un cache KV (les autres sont en attention linéaire).
        let a = partial("unsloth--Qwen3.5-9B-GGUF--Qwen3.5-9B-UD-IQ2_XXS")
            .arch_info()
            .unwrap();
        assert_eq!(a.architecture, "qwen35");
        assert_eq!((a.n_layers, a.n_kv_heads, a.head_dim), (32, 4, 256));
        assert_eq!(a.kv_layers, Some(8));
    }

    #[test]
    fn moe_qwen36() {
        let m = partial("ggml-org--Qwen3.6-35B-A3B-GGUF--Qwen3.6-35B-A3B-Q4_K_M");
        let a = m.arch_info().unwrap();
        assert_eq!((a.expert_count, a.expert_used_count), (Some(256), Some(8)));
        // `recurrent_layers` : 30 couches récurrentes sur 40.
        assert_eq!(a.kv_layers, Some(10));
        // « A3B » : environ 3 milliards de paramètres actifs.
        let active = m.params_active(34_660_610_688).unwrap();
        assert!((3_000_000_000..4_000_000_000).contains(&active), "{active}");
    }

    #[test]
    fn gemma3_fenetre_glissante() {
        let a = partial("ggml-org--gemma-3-4b-it-GGUF--gemma-3-4b-it-Q4_K_M")
            .arch_info()
            .unwrap();
        assert_eq!((a.n_layers, a.n_heads, a.n_kv_heads), (34, 8, 4));
        assert_eq!(a.head_dim, 256);
        assert_eq!(a.sliding_window, Some(1024));
        assert_eq!(a.kv_layers, None);
    }

    #[test]
    fn granite_tetes_kv_par_couche() {
        let m = partial("ibm-granite--granite-4.0-h-micro-GGUF--granite-4.0-h-micro-Q2_K");
        let a = m.arch_info().unwrap();
        assert_eq!(a.n_kv_heads, 8);
        assert_eq!(a.kv_layers, Some(4));
        // `expert_count = 0` : modèle dense.
        assert_eq!(a.expert_count, None);
        assert_eq!(m.params_active(3_191_396_096), None);
        // Pas de `key_length` : head_dim = 2048 / 32.
        assert_eq!(a.head_dim, 64);
    }

    #[test]
    fn tronque_avant_les_cles() {
        let h = header("Qwen--Qwen3-8B-GGUF--Qwen3-8B-Q4_K_M");
        assert_eq!(parse(&h[..300]).unwrap(), Parsed::NeedMore);
        assert_eq!(parse(&h[..10]).unwrap(), Parsed::NeedMore);
        assert_eq!(parse(&[]).unwrap(), Parsed::NeedMore);
    }

    #[test]
    fn erreurs() {
        assert_eq!(parse(b"<!DOCTYPE html>"), Err(GgufError::NotGguf));
        let mut h = header("Qwen--Qwen3-8B-GGUF--Qwen3-8B-Q4_K_M");
        h[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(parse(&h), Err(GgufError::UnsupportedVersion(1)));
        // Longueur de chaîne aberrante dans la première clé.
        let mut h = header("Qwen--Qwen3-8B-GGUF--Qwen3-8B-Q4_K_M");
        h[24..32].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(parse(&h), Err(GgufError::Malformed(_))));
    }

    /// Écrit un en-tête GGUF minimal (tous les types utilisés par llama.cpp).
    fn synth(pairs: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"GGUF".to_vec();
        out.extend(3u32.to_le_bytes());
        out.extend(0u64.to_le_bytes());
        out.extend((pairs.len() as u64).to_le_bytes());
        for (k, ty, v) in pairs {
            out.extend((k.len() as u64).to_le_bytes());
            out.extend(k.as_bytes());
            out.extend(ty.to_le_bytes());
            out.extend(v);
        }
        out
    }

    fn s(v: &str) -> Vec<u8> {
        let mut out = (v.len() as u64).to_le_bytes().to_vec();
        out.extend(v.as_bytes());
        out
    }

    #[test]
    fn complet_et_moe_synthetique() {
        // Mixtral : MoE déclaré dans l'architecture `llama`, FFN des experts = feed_forward_length.
        let strings = {
            let mut v = 8u32.to_le_bytes().to_vec();
            v.extend(2u64.to_le_bytes());
            v.extend(s("a"));
            v.extend(s("b"));
            v
        };
        let floats = {
            let mut v = 6u32.to_le_bytes().to_vec();
            v.extend(2u64.to_le_bytes());
            v.extend(1.0f32.to_le_bytes());
            v.extend(2.0f32.to_le_bytes());
            v
        };
        let h = synth(&[
            ("general.architecture", 8, s("llama")),
            ("general.tags", 9, strings),
            ("general.scores", 9, floats),
            ("general.flag", 7, vec![1]),
            ("general.temp", 12, 0.5f64.to_le_bytes().to_vec()),
            ("llama.block_count", 4, 32u32.to_le_bytes().to_vec()),
            ("llama.context_length", 10, 32768u64.to_le_bytes().to_vec()),
            ("llama.embedding_length", 4, 4096u32.to_le_bytes().to_vec()),
            (
                "llama.feed_forward_length",
                4,
                14336u32.to_le_bytes().to_vec(),
            ),
            (
                "llama.attention.head_count",
                4,
                32u32.to_le_bytes().to_vec(),
            ),
            (
                "llama.attention.head_count_kv",
                4,
                8u32.to_le_bytes().to_vec(),
            ),
            ("llama.expert_count", 4, 8u32.to_le_bytes().to_vec()),
            ("llama.expert_used_count", 4, 2u32.to_le_bytes().to_vec()),
            ("tokenizer.ggml.model", 8, s("llama")),
        ]);
        let Parsed::Complete(m) = parse(&h).unwrap() else {
            panic!("en-tête complet attendu");
        };
        assert_eq!(m.context_length, Some(32768));
        let a = m.arch_info().unwrap();
        assert_eq!((a.n_kv_heads, a.head_dim), (8, 128));
        // Mixtral 8x7B : 46,7 G paramètres, ~12,9 G actifs.
        let active = m.params_active(46_702_792_704).unwrap();
        assert!(
            (12_000_000_000..14_000_000_000).contains(&active),
            "{active}"
        );
    }

    #[test]
    fn deepseek_couches_denses() {
        // DeepSeek-V3 : 3 couches denses en tête, 256 experts dont 8 actifs (~37 G actifs).
        let m = GgufMeta {
            architecture: Some("deepseek2".into()),
            block_count: Some(61),
            embedding_length: Some(7168),
            expert_feed_forward_length: Some(2048),
            expert_count: Some(256),
            expert_used_count: Some(8),
            leading_dense_block_count: Some(3),
            ..GgufMeta::default()
        };
        let active = m.params_active(671_026_419_200).unwrap();
        assert!(
            (30_000_000_000..45_000_000_000).contains(&active),
            "{active}"
        );
        // Total incohérent (plus d'experts inactifs que de paramètres) : pas de valeur.
        assert_eq!(m.params_active(100_000_000_000), None);
    }
}
