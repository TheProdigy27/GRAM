//! Repli quand l'en-tête GGUF est illisible : `config.json` du modèle de base (§7.4 du plan).
//! Pour les modèles multimodaux, les champs utiles sont souvent dans `text_config`.

use gram_core::{ArchInfo, ArchSource};
use serde_json::Value;

fn u32_field(obj: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|k| obj.get(*k).and_then(Value::as_u64))
        .and_then(|v| u32::try_from(v).ok())
}

/// Résultat du repli : architecture, et contexte d'entraînement s'il est indiqué.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigArch {
    pub arch: ArchInfo,
    pub ctx_train: Option<u32>,
}

pub fn parse_config(json: &[u8]) -> Option<ConfigArch> {
    let root: Value = serde_json::from_slice(json).ok()?;
    let cfg = root
        .get("text_config")
        .filter(|t| t.get("num_hidden_layers").is_some())
        .unwrap_or(&root);

    let n_layers = u32_field(cfg, &["num_hidden_layers", "n_layer", "num_layers"])?;
    let n_heads = u32_field(cfg, &["num_attention_heads", "n_head"])?;
    if n_layers == 0 || n_heads == 0 {
        return None;
    }
    let n_kv_heads = u32_field(cfg, &["num_key_value_heads", "num_kv_heads"]).unwrap_or(n_heads);
    let hidden = u32_field(cfg, &["hidden_size", "n_embd", "d_model"]);
    let head_dim = u32_field(cfg, &["head_dim"])
        .or_else(|| hidden.map(|h| h / n_heads))
        .filter(|&d| d > 0)?;

    // Hybrides : `layer_types` (« full_attention », « sliding_attention », « linear_attention »,
    // « mamba »…) ou `full_attention_interval`. Seules les couches d'attention ont un cache KV.
    let kv_layers = cfg
        .get("layer_types")
        .and_then(Value::as_array)
        .map(|types| {
            types
                .iter()
                .filter_map(Value::as_str)
                .filter(|t| t.contains("attention") && !t.contains("linear"))
                .count() as u32
        })
        .or_else(|| {
            u32_field(cfg, &["full_attention_interval"])
                .filter(|&i| i > 1)
                .map(|i| n_layers / i)
        })
        .filter(|&k| k < n_layers);

    let architecture = cfg
        .get("model_type")
        .or_else(|| root.get("model_type"))
        .and_then(Value::as_str)
        .unwrap_or("inconnue")
        .to_owned();

    Some(ConfigArch {
        arch: ArchInfo {
            architecture,
            n_layers,
            n_heads,
            n_kv_heads,
            head_dim,
            kv_layers,
            embedding_length: hidden,
            expert_count: u32_field(
                cfg,
                &["num_experts", "num_local_experts", "n_routed_experts"],
            )
            .filter(|&n| n > 0),
            expert_used_count: u32_field(cfg, &["num_experts_per_tok", "num_experts_per_token"])
                .filter(|&n| n > 0),
            sliding_window: u32_field(cfg, &["sliding_window"]).filter(|&n| n > 0),
            source: ArchSource::ConfigJson,
        },
        ctx_train: u32_field(cfg, &["max_position_embeddings"]).filter(|&n| n > 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn qwen25() {
        let c = parse_config(&fixture("config/Qwen--Qwen2.5-7B-Instruct.json")).unwrap();
        assert_eq!(c.arch.architecture, "qwen2");
        assert_eq!(
            (
                c.arch.n_layers,
                c.arch.n_heads,
                c.arch.n_kv_heads,
                c.arch.head_dim
            ),
            (28, 28, 4, 128)
        );
        assert_eq!(c.arch.source, ArchSource::ConfigJson);
        assert_eq!(c.ctx_train, Some(32768));
    }

    #[test]
    fn multimodal_text_config() {
        let c = parse_config(&fixture("config/Qwen--Qwen3-VL-8B-Instruct.json")).unwrap();
        assert_eq!(c.arch.architecture, "qwen3_vl_text");
        assert_eq!(
            (c.arch.n_layers, c.arch.n_kv_heads, c.arch.head_dim),
            (36, 8, 128)
        );
        assert_eq!(c.ctx_train, Some(262144));
    }

    #[test]
    fn hybride_et_moe() {
        let json = br#"{ "model_type": "qwen3_next", "num_hidden_layers": 48,
            "num_attention_heads": 16, "num_key_value_heads": 2, "head_dim": 256,
            "hidden_size": 2048, "num_experts": 512, "num_experts_per_tok": 10,
            "full_attention_interval": 4, "sliding_window": null }"#;
        let a = parse_config(json).unwrap().arch;
        assert_eq!(a.kv_layers, Some(12));
        assert_eq!((a.expert_count, a.expert_used_count), (Some(512), Some(10)));
        assert_eq!(a.sliding_window, None);

        let json = br#"{ "num_hidden_layers": 4, "num_attention_heads": 8, "hidden_size": 512,
            "layer_types": ["mamba", "attention", "mamba", "mamba"] }"#;
        let a = parse_config(json).unwrap().arch;
        assert_eq!((a.n_kv_heads, a.head_dim, a.kv_layers), (8, 64, Some(1)));

        assert!(parse_config(b"{}").is_none());
        assert!(parse_config(b"pas du json").is_none());
    }
}
