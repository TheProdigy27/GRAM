//! Classement (§9 du plan) sur un échantillon figé du catalogue réel
//! (`tests/fixtures/catalog/sample.json`, 35 modèles) et les profils matériels types (§15).
//!
//! Les snapshots (`tests/snapshots/`) ont été relus à la main : ils fixent un classement jugé
//! cohérent. Après un changement volontaire du scoring, relire les différences avec
//! `cargo insta review` (ou `INSTA_UPDATE=always cargo test`, puis `git diff`).

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use gram_core::export::{to_csv, to_markdown};
use gram_core::fmt::{ctx_k, gb_value};
use gram_core::launch::Runtime;
use gram_core::rank::{MIN_GAIN, QUALITY_GAP, QUANT_GAP, SCORE_GAP};
use gram_core::{
    DataSource, DataSourceKind, DetailRequest, FitLevel, HardwareProfile, ModelEntry, OllamaModel,
    PickKind, RankContext, RankItem, RankRequest, RankResponse, RunMode, RuntimeStatus,
    RuntimesStatus, Settings, UseCase, model_detail, rank,
};
use std::fmt::Write as _;

macro_rules! fixture {
    ($path:literal) => {
        serde_json::from_str(include_str!(concat!("../../../tests/fixtures/", $path))).expect($path)
    };
}

fn sample() -> Vec<ModelEntry> {
    fixture!("catalog/sample.json")
}
fn p1() -> HardwareProfile {
    fixture!("profiles/p1-portable-igpu-16go.json")
}
fn p2() -> HardwareProfile {
    fixture!("profiles/p2-rtx3060-12go-32go-ddr4.json")
}
fn p3() -> HardwareProfile {
    fixture!("profiles/p3-rtx4090-24go-64go-ddr5.json")
}
fn p4() -> HardwareProfile {
    fixture!("profiles/p4-rx7800xt-16go-32go-ddr5.json")
}
/// Mac Apple Silicon, mémoire unifiée.
fn p5() -> HardwareProfile {
    fixture!("profiles/p5-m1pro-16go-macos.json")
}

/// Date de référence de la récence : les snapshots ne dépendent pas du jour du test.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap()
}

fn source() -> DataSource {
    DataSource {
        kind: DataSourceKind::Snapshot,
        fetched_at: Utc.with_ymd_and_hms(2026, 11, 9, 19, 57, 31).unwrap(),
    }
}

fn run(models: &[ModelEntry], hw: &HardwareProfile, req: &RankRequest) -> RankResponse {
    let settings = Settings::default();
    let cx = RankContext::new(hw, None, &settings, now());
    rank(models, req, &cx, source())
}

fn request(use_case: UseCase, ctx: Option<u32>) -> RankRequest {
    RankRequest {
        use_case,
        ctx,
        show_too_big: true,
        ..RankRequest::default()
    }
}

/// Une ligne par modèle, lisible dans un diff.
fn table(out: &mut String, title: &str, items: &[RankItem]) {
    let _ = writeln!(out, "{title} ({})", items.len());
    for it in items {
        let (v, e) = (&it.best.variant, &it.best.estimate);
        let name: String = it.display_name.chars().take(36).collect();
        let label: String = v.label.chars().take(14).collect();
        let _ = writeln!(
            out,
            "{:>2} {:<36} {:<14}{} {:>6} Go {:<14} {:>6.1} tok/s ctx {:>5} score {:>5.1} (Q {:>5.1} V {:>5.1} F {:>5.1} C {:>5.1}) {}",
            it.rank,
            name,
            label,
            if e.approximate { "*" } else { " " },
            gb_value(v.size_bytes),
            e.mode.label(),
            e.tok_s,
            ctx_k(e.ctx_max),
            it.score.total,
            it.score.quality,
            it.score.speed,
            it.score.fit,
            it.score.context,
            e.fit.label()
        );
    }
}

fn snapshot_text(r: &RankResponse) -> String {
    let mut out = format!(
        "{} · contexte {} · VRAM {} Go · RAM {} Go · lent sous {} tok/s\n\n",
        r.use_case.label(),
        r.ctx,
        gb_value(r.budget.vram),
        gb_value(r.budget.ram),
        r.slow_tok_s
    );
    table(&mut out, "Exécutables", &r.items);
    out.push('\n');
    table(&mut out, "Exécutable mais lent", &r.slow);
    out.push('\n');
    table(&mut out, "Trop gros", &r.too_big);

    let _ = writeln!(out, "\nConseils ({})", r.picks.len());
    for p in &r.picks {
        let (it, e) = (&p.item, &p.item.best.estimate);
        let name: String = it.display_name.chars().take(36).collect();
        let _ = writeln!(
            out,
            "{:<21} #{:<2} {:<36} {:<14} {:>6.1} tok/s ctx {:>5} score {:>5.1} (Q {:>5.1}) — {}",
            p.kind.label(),
            it.rank,
            name,
            it.best.variant.label,
            e.tok_s,
            ctx_k(e.ctx_max),
            it.score.total,
            it.score.quality,
            p.reason
        );
    }

    out.push_str("\nFamilles\n");
    for it in r.items.iter().chain(&r.slow).chain(&r.too_big) {
        if let Some(family) = &it.family {
            let _ = writeln!(out, "{} → {family}", it.display_name);
        }
    }
    out
}

// ── Snapshots (relus à la main) ─────────────────────────────────────────────────────────────

#[test]
fn snapshot_p1_portable_polyvalent() {
    let r = run(&sample(), &p1(), &request(UseCase::General, None));
    insta::assert_snapshot!(snapshot_text(&r));
}

#[test]
fn snapshot_p2_rtx3060_code_16k() {
    let r = run(&sample(), &p2(), &request(UseCase::Code, Some(16_384)));
    insta::assert_snapshot!(snapshot_text(&r));
}

#[test]
fn snapshot_p3_rtx4090_raisonnement() {
    let r = run(&sample(), &p3(), &request(UseCase::Reasoning, None));
    insta::assert_snapshot!(snapshot_text(&r));
}

#[test]
fn snapshot_p4_rx7800xt_vision() {
    let r = run(&sample(), &p4(), &request(UseCase::Vision, None));
    insta::assert_snapshot!(snapshot_text(&r));
}

// ── Règles du classement ───────────────────────────────────────────────────────────────────

fn sorted_by_score(items: &[RankItem]) -> bool {
    items
        .windows(2)
        .all(|w| w[0].score.total >= w[1].score.total)
}

#[test]
fn invariants_sur_tous_les_profils_et_usages() {
    let models = sample();
    for hw in [p1(), p2(), p3(), p4()] {
        for use_case in UseCase::ALL {
            let r = run(&models, &hw, &request(use_case, None));
            let ctx = Settings::default().use_cases.get(use_case).ctx;
            assert_eq!(r.ctx, ctx);
            assert!(sorted_by_score(&r.items) && sorted_by_score(&r.slow));
            assert_eq!(r.total_items as usize, r.items.len());
            assert_eq!(r.too_big_count as usize, r.too_big.len());
            // Chaque modèle n'apparaît qu'une fois.
            let mut ids: Vec<&str> = r
                .items
                .iter()
                .chain(&r.slow)
                .chain(&r.too_big)
                .map(|i| i.repo_id.as_str())
                .collect();
            let n = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), n, "doublon pour {use_case:?}");
            if use_case != UseCase::Vision {
                // Seul Nemotron-74B-A9B-24GB est écarté : son unique quantification (2,38 bits
                // par poids) est sous le minimum de 2,5.
                assert_eq!(n, models.len() - 1, "tous les autres modèles sont classés");
                assert!(
                    !ids.iter()
                        .any(|id| id.contains("Nemotron-3.5-Bolt-74B-A9B-24GB"))
                );
            }
            for it in &r.items {
                let e = &it.best.estimate;
                assert!(e.mode != RunMode::TooBig && !it.slow);
                assert!(e.tok_s >= r.slow_tok_s, "{} : {}", it.repo_id, e.tok_s);
                assert!((0.0..=100.0).contains(&it.score.total));
                assert!(!it.explanation.is_empty());
            }
            for it in &r.slow {
                assert!(it.slow && it.best.estimate.tok_s < r.slow_tok_s);
                assert!(it.best.estimate.mode != RunMode::TooBig);
            }
            for it in &r.too_big {
                assert_eq!(it.best.estimate.mode, RunMode::TooBig);
                assert_eq!(it.best.estimate.fit, FitLevel::TooTight);
                assert!(it.explanation.starts_with("Trop gros"));
            }
            if use_case == UseCase::Vision {
                assert!(
                    r.items
                        .iter()
                        .chain(&r.slow)
                        .chain(&r.too_big)
                        .all(|i| i.use_cases.contains(&UseCase::Vision))
                );
            }
        }
    }
}

#[test]
fn deterministe_et_independant_de_l_ordre_du_catalogue() {
    let models = sample();
    let req = request(UseCase::General, None);
    let a = run(&models, &p3(), &req);
    let b = run(&models, &p3(), &req);
    assert_eq!(a, b);
    let mut reversed = models.clone();
    reversed.reverse();
    let c = run(&reversed, &p3(), &req);
    assert_eq!(a, c);
}

#[test]
fn la_meilleure_quantification_est_celle_de_la_fiche_detail() {
    let models = sample();
    let hw = p2();
    let settings = Settings::default();
    let cx = RankContext::new(&hw, None, &settings, now());
    let r = rank(&models, &request(UseCase::Code, None), &cx, source());
    for it in r.items.iter().chain(&r.slow) {
        let m = models.iter().find(|m| m.repo_id == it.repo_id).unwrap();
        let req = DetailRequest {
            repo_id: it.repo_id.clone(),
            use_case: UseCase::Code,
            ..DetailRequest::default()
        };
        let d = model_detail(m, &req, &cx);
        let best = &d.variants[d.best.expect("meilleure variante") as usize];
        assert_eq!(best.variant, it.best.variant);
        assert_eq!(best.score, it.score);
        assert_eq!(best.explanation, it.explanation);
        // Aucune variante assez rapide ne fait mieux.
        for v in &d.variants {
            if v.estimate.mode != RunMode::TooBig && (!v.slow || it.slow) {
                assert!(
                    v.score.total <= best.score.total,
                    "{} {}",
                    m.repo_id,
                    v.variant.label
                );
            }
        }
        assert_eq!(d.variants.len() as u32, it.variant_count);
        assert!(
            d.variants
                .iter()
                .all(|v| v.launch.runtime == Runtime::Ollama)
        );
    }
}

#[test]
fn modele_lent_si_aucune_quantification_n_est_assez_rapide() {
    // Llama-3.3-70B sur RTX 3060 : seule une quantification agressive tient, en hybride, lente.
    let models: Vec<ModelEntry> = sample()
        .into_iter()
        .filter(|m| m.display_name == "Llama-3.3-70B-Instruct")
        .collect();
    let r = run(&models, &p2(), &request(UseCase::General, None));
    assert!(r.items.is_empty());
    assert_eq!(r.slow.len(), 1);
    assert!(matches!(
        r.slow[0].best.estimate.mode,
        RunMode::Hybrid { .. }
    ));
    // Sur RTX 4090, assez de couches tiennent en VRAM pour dépasser le seuil : exécutable.
    let r = run(&models, &p3(), &request(UseCase::General, None));
    assert_eq!(r.items.len(), 1, "{:?}", r.slow);
    assert!(r.items[0].best.estimate.tok_s >= r.slow_tok_s);
}

#[test]
fn filtres_taille_limite_et_trop_gros() {
    let models = sample();
    let all = run(&models, &p2(), &request(UseCase::General, None));
    assert!(all.too_big_count > 0 && !all.too_big.is_empty());
    // Les modèles trop gros sont triés du plus proche de tenir au plus éloigné.
    assert!(
        all.too_big
            .windows(2)
            .all(|w| w[0].best.estimate.mem_usage <= w[1].best.estimate.mem_usage)
    );

    let hidden = run(
        &models,
        &p2(),
        &RankRequest {
            show_too_big: false,
            ..request(UseCase::General, None)
        },
    );
    assert!(hidden.too_big.is_empty());
    assert_eq!(hidden.too_big_count, all.too_big_count);
    assert_eq!(hidden.items, all.items);

    let max = 5u64 << 30;
    let small = run(
        &models,
        &p2(),
        &RankRequest {
            max_size: Some(max),
            ..request(UseCase::General, None)
        },
    );
    assert!(
        small
            .items
            .iter()
            .chain(&small.slow)
            .chain(&small.too_big)
            .all(|i| i.best.variant.size_bytes <= max)
    );
    // Les modèles sans aucune quantification sous la taille maximale sont écartés.
    assert!(small.total_items + small.total_slow + small.too_big_count < models.len() as u32);

    let limited = run(
        &models,
        &p2(),
        &RankRequest {
            limit: Some(3),
            ..request(UseCase::General, None)
        },
    );
    assert_eq!(limited.items.len(), 3);
    assert_eq!(limited.items[..], all.items[..3]);
    assert_eq!(limited.total_items, all.total_items);
}

#[test]
fn recherche_par_nom() {
    let models = sample();
    let all = run(&models, &p2(), &request(UseCase::General, None));
    let search = |q: &str| {
        run(
            &models,
            &p2(),
            &RankRequest {
                search: Some(q.into()),
                ..request(UseCase::General, None)
            },
        )
    };
    let names = |r: &RankResponse| -> Vec<String> {
        r.items
            .iter()
            .chain(&r.slow)
            .chain(&r.too_big)
            .map(|i| i.display_name.clone())
            .collect()
    };

    // Chaque mot doit figurer, sans tenir compte de la casse ni des séparateurs.
    let r = search("  QWEN3   8b ");
    assert_eq!(r.search.as_deref(), Some("QWEN3 8b"));
    let found = names(&r);
    assert!(found.contains(&"Qwen3-8B".to_owned()), "{found:?}");
    assert!(
        found.contains(&"Qwen3-VL-8B-Instruct".to_owned()),
        "{found:?}"
    );
    assert!(!found.contains(&"Qwen3-14B".to_owned()), "{found:?}");
    assert_eq!(
        r.total_items + r.total_slow + r.too_big_count,
        found.len() as u32
    );

    // Chaque ligne garde son rang dans le classement complet.
    for it in r.items.iter() {
        let pos = all
            .items
            .iter()
            .position(|a| a.repo_id == it.repo_id)
            .unwrap();
        assert_eq!(it.rank, pos as u32 + 1, "{}", it.display_name);
    }
    assert!(
        all.items
            .iter()
            .enumerate()
            .all(|(i, it)| it.rank == i as u32 + 1)
    );

    // Un mot sans séparateur les ignore dans les noms ; un mot avec séparateur est cherché tel
    // quel.
    let mut llama3 = names(&search("llama3"));
    llama3.sort();
    assert_eq!(
        llama3,
        ["Llama-3.3-70B-Instruct", "llama-3-firefunction-v2"]
    );
    assert_eq!(names(&search("llama-3.3")), ["Llama-3.3-70B-Instruct"]);
    let dotted = names(&search("3.3"));
    assert!(
        !dotted.is_empty() && dotted.iter().all(|n| n.contains("3.3")),
        "{dotted:?}"
    );

    // Les modèles trop gros trouvés sont comptés même s'ils ne sont pas listés.
    let hidden = run(
        &models,
        &p2(),
        &RankRequest {
            search: Some("llama".into()),
            show_too_big: false,
            ..request(UseCase::General, None)
        },
    );
    assert!(hidden.too_big.is_empty());
    assert_eq!(hidden.too_big_count, search("llama").too_big.len() as u32);
    assert!(hidden.too_big_count < all.too_big_count);

    // Recherche vide : tous les modèles.
    let blank = search("   ");
    assert_eq!(blank.search, None);
    assert_eq!(blank.items, all.items);
    assert!(names(&search("aucun-modele-de-ce-nom")).is_empty());
}

#[test]
fn usage_code_favorise_les_modeles_de_code() {
    let models = sample();
    let general = run(&models, &p3(), &request(UseCase::General, Some(16_384)));
    let code = run(&models, &p3(), &request(UseCase::Code, Some(16_384)));
    let find = |r: &RankResponse, name: &str| {
        r.items
            .iter()
            .chain(&r.slow)
            .find(|i| i.display_name == name)
            .unwrap()
            .score
    };
    let coder = find(&code, "Qwen2.5-Coder-14B-Instruct");
    let plain = find(&code, "Qwen2.5-14B-Instruct");
    assert_eq!(coder.use_case_factor, 1.0);
    assert_eq!(plain.use_case_factor, 0.92);
    assert!(coder.total > plain.total);
    // En usage polyvalent, aucune pénalité.
    assert_eq!(find(&general, "Qwen2.5-14B-Instruct").use_case_factor, 1.0);
}

#[test]
fn contexte_et_runtime_par_defaut() {
    let r = run(&sample(), &p2(), &RankRequest::default());
    assert_eq!(
        (r.use_case, r.ctx, r.runtime),
        (UseCase::General, 8192, Runtime::Ollama)
    );
    let r = run(
        &sample(),
        &p2(),
        &RankRequest {
            use_case: UseCase::FastChat,
            runtime: Some(Runtime::LlamaCpp),
            ..RankRequest::default()
        },
    );
    assert_eq!(
        (r.ctx, r.slow_tok_s, r.runtime),
        (4096, 10.0, Runtime::LlamaCpp)
    );
}

// ── Conseils et familles ───────────────────────────────────────────────────────────────────

#[test]
fn conseils_sur_tous_les_profils_et_usages() {
    let models = sample();
    let mut other_quant = 0;
    for hw in [p1(), p2(), p3(), p4(), p5()] {
        for use_case in UseCase::ALL {
            let r = run(&models, &hw, &request(use_case, None));
            let ctx = format!("{} · {use_case:?}", hw.cpu.name);
            let Some(best) = r.items.first() else {
                assert!(r.picks.is_empty(), "{ctx}");
                continue;
            };
            assert!(r.picks.len() <= 3, "{ctx}");
            // Meilleur choix d'abord, puis le plus rapide, puis le plus long contexte.
            assert_eq!(r.picks[0].kind, PickKind::Best, "{ctx}");
            assert_eq!(&r.picks[0].item, best, "{ctx}");
            let order = |k: PickKind| {
                [PickKind::Best, PickKind::Fastest, PickKind::LongestContext]
                    .iter()
                    .position(|&o| o == k)
            };
            assert!(
                r.picks
                    .windows(2)
                    .all(|w| order(w[0].kind) < order(w[1].kind)),
                "{ctx}"
            );
            let mut ids: Vec<&str> = r.picks.iter().map(|p| p.item.repo_id.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), r.picks.len(), "un conseil par modèle ({ctx})");
            // Commande de lancement de la quantification du conseil, avec le runtime demandé.
            for p in &r.picks {
                assert_eq!(p.launch.runtime, r.runtime);
                let run = format!(
                    "ollama run hf.co/{}:{}",
                    p.item.best.variant.repo_id, p.item.best.variant.label
                );
                assert_eq!(p.launch.commands[0].command, run, "{ctx}");
            }

            let (b, bq) = (&best.best.estimate, best.score.quality);
            for p in &r.picks[1..] {
                let (it, e) = (&p.item, &p.item.best.estimate);
                // Le modèle est proche du meilleur score ; sa quantification ne perd pas
                // beaucoup en qualité, ni par rapport au classement, ni par rapport au meilleur.
                let ranked = &r.items[it.rank as usize - 1];
                assert_eq!(ranked.repo_id, it.repo_id, "{ctx}");
                assert!(ranked.score.total >= best.score.total - SCORE_GAP, "{ctx}");
                assert!(
                    it.score.quality >= ranked.score.quality - QUANT_GAP,
                    "{ctx}"
                );
                assert!(it.score.quality >= bq - QUALITY_GAP, "{ctx}");
                assert!(e.mode != RunMode::TooBig && !it.slow && e.tok_s >= r.slow_tok_s);
                assert!(it.explanation.contains(&it.best.variant.label), "{ctx}");
                assert!(!p.reason.is_empty());
                if it.best.variant.label != ranked.best.variant.label {
                    other_quant += 1;
                }
                // Aucun candidat dans sa quantification du classement ne fait mieux.
                let candidates = r.items.iter().filter(|c| {
                    c.score.total >= best.score.total - SCORE_GAP
                        && c.score.quality >= bq - QUALITY_GAP
                        && r.picks.iter().all(|q| q.item.repo_id != c.repo_id)
                });
                match p.kind {
                    PickKind::Fastest => {
                        assert!(e.tok_s >= MIN_GAIN * b.tok_s, "{ctx}");
                        assert!(p.reason.contains("plus rapide que le meilleur choix"));
                        for c in candidates {
                            assert!(c.best.estimate.tok_s <= e.tok_s, "{ctx} : {}", c.repo_id);
                        }
                    }
                    PickKind::LongestContext => {
                        assert!(e.ctx_max as f32 >= MIN_GAIN * b.ctx_max as f32, "{ctx}");
                        assert!(p.reason.starts_with("Contexte jusqu'à"));
                        for c in candidates {
                            assert!(
                                c.best.estimate.ctx_max <= e.ctx_max,
                                "{ctx} : {}",
                                c.repo_id
                            );
                        }
                    }
                    PickKind::Best => panic!("un seul meilleur choix ({ctx})"),
                }
            }
        }
    }
    // Un conseil retient parfois une autre quantification que le classement (plus rapide, ou
    // au contexte plus long, pour un peu de qualité en moins).
    assert!(other_quant > 0);
}

#[test]
fn conseils_independants_de_la_recherche_et_de_limit() {
    let models = sample();
    let all = run(&models, &p2(), &request(UseCase::General, None));
    assert_eq!(all.picks.len(), 3, "{:#?}", all.picks);
    let kinds: Vec<PickKind> = all.picks.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [PickKind::Best, PickKind::Fastest, PickKind::LongestContext]
    );
    for req in [
        RankRequest {
            search: Some("llama".into()),
            ..request(UseCase::General, None)
        },
        RankRequest {
            search: Some("aucun-modele-de-ce-nom".into()),
            ..request(UseCase::General, None)
        },
        RankRequest {
            limit: Some(1),
            show_too_big: false,
            ..request(UseCase::General, None)
        },
    ] {
        assert_eq!(run(&models, &p2(), &req).picks, all.picks, "{req:?}");
    }
    // La taille maximale est un filtre : elle change les conseils.
    let small = run(
        &models,
        &p2(),
        &RankRequest {
            max_size: Some(4 << 30),
            ..request(UseCase::General, None)
        },
    );
    assert!(
        small
            .picks
            .iter()
            .all(|p| p.item.best.variant.size_bytes <= 4 << 30)
    );

    // Aucun exécutable assez rapide : aucun conseil.
    assert!(
        run(&[], &p2(), &request(UseCase::General, None))
            .picks
            .is_empty()
    );
    let llama: Vec<ModelEntry> = models
        .into_iter()
        .filter(|m| m.display_name == "Llama-3.3-70B-Instruct")
        .collect();
    let slow = run(&llama, &p2(), &request(UseCase::General, None));
    assert!(slow.items.is_empty() && slow.picks.is_empty());
}

#[test]
fn conseils_en_memoire_unifiee() {
    let r = run(&sample(), &p5(), &request(UseCase::General, None));
    assert!(!r.picks.is_empty());
    assert!(
        r.picks.iter().all(|p| !p.reason.contains("VRAM")),
        "{:#?}",
        r.picks
    );
    assert!(r.picks.iter().any(|p| p.reason.contains("sur le GPU")));
}

#[test]
fn familles_de_l_echantillon() {
    let models = sample();
    for use_case in UseCase::ALL {
        let r = run(&models, &p3(), &request(use_case, None));
        for it in r.items.iter().chain(&r.slow).chain(&r.too_big) {
            if let Some(family) = &it.family {
                // La famille est un autre modèle du catalogue, sans famille lui-même.
                assert_ne!(family, &it.repo_id);
                assert!(models.iter().any(|m| &m.repo_id == family), "{family}");
            }
        }
    }
    let r = run(&models, &p3(), &request(UseCase::General, None));
    let family = |name: &str| {
        r.items
            .iter()
            .chain(&r.slow)
            .chain(&r.too_big)
            .find(|i| i.display_name == name)
            .unwrap_or_else(|| panic!("{name} absent"))
            .family
            .clone()
    };
    assert_eq!(
        family("Qwen3.8-27B-Ultimate-Heretic-Uncensored-Turbo-Brilliance-50x-AGI-NEO").as_deref(),
        Some("bartowski/Qwen3.8-27B-GGUF")
    );
    assert_eq!(family("Qwen3.8-27B"), None);
    // Qwen3-VL-8B n'est pas une variante de Qwen3-8B ; Qwen3.5-9B n'est pas dans l'échantillon.
    assert_eq!(family("Qwen3-VL-8B-Instruct"), None);
    assert_eq!(
        family("Qwen3.5-9B-The-Defiant-Fable-Uncensored-Heretic-NEO-IMATRIX-MAX-MTP"),
        None
    );
    assert_eq!(family("MiMo-V2.6-Distill-Qwen-9B"), None);
}

// ── Avertissements par runtime (§10.2) ─────────────────────────────────────────────────────

fn installed(ollama: bool, lm_studio: bool, llama_cpp: bool) -> RuntimesStatus {
    let s = |installed| RuntimeStatus {
        installed,
        ..RuntimeStatus::default()
    };
    RuntimesStatus {
        ollama: s(ollama),
        lm_studio: s(lm_studio),
        llama_cpp: s(llama_cpp),
        ollama_models: Vec::new(),
    }
}

fn notes(hw: &HardwareProfile, runtime: Runtime, rt: Option<&RuntimesStatus>) -> Vec<String> {
    let settings = Settings::default();
    let cx = RankContext::new(hw, rt, &settings, now());
    let req = RankRequest {
        runtime: Some(runtime),
        ..RankRequest::default()
    };
    rank(&[], &req, &cx, source()).notes
}

#[test]
fn avertissements_des_runtimes() {
    // RTX 3060 (12 Go) : Ollama prendrait 4k tokens de contexte.
    let n = notes(&p2(), Runtime::Ollama, Some(&installed(true, false, false)));
    assert!(n[0].contains("4 096 tokens"), "{n:?}");
    assert!(!n.iter().any(|x| x.contains("n'est pas installé")));
    // RTX 4090 (24 Go) : 32k.
    let n = notes(&p3(), Runtime::Ollama, None);
    assert!(n[0].contains("32 768 tokens"), "{n:?}");
    // iGPU seul : Ollama l'ignore, LM Studio peut l'utiliser.
    let n = notes(&p1(), Runtime::Ollama, None);
    assert!(n.iter().any(|x| x.contains("LM Studio (Vulkan)")), "{n:?}");
    let n = notes(&p1(), Runtime::LmStudio, None);
    assert!(n[0].contains("GPU intégré avec Vulkan"), "{n:?}");
    // Runtime absent : proposer celui qui est installé.
    let n = notes(&p2(), Runtime::Ollama, Some(&installed(false, true, false)));
    assert!(n[0].contains("LM Studio l'est"), "{n:?}");
    let n = notes(
        &p2(),
        Runtime::LlamaCpp,
        Some(&installed(false, false, false)),
    );
    assert!(n[0].contains("installer Ollama"), "{n:?}");
    assert_eq!(n.len(), 1);
}

// ── Fiche détail et export ─────────────────────────────────────────────────────────────────

#[test]
fn fiche_detail_commandes_et_vision() {
    let models = sample();
    let m = models
        .iter()
        .find(|m| m.display_name == "Ornith-1.5-35B-A3B")
        .unwrap();
    let hw = p3();
    let settings = Settings::default();
    let cx = RankContext::new(&hw, None, &settings, now());
    let req = |use_case, runtime| DetailRequest {
        repo_id: m.repo_id.clone(),
        use_case,
        runtime: Some(runtime),
        ..DetailRequest::default()
    };
    let d = model_detail(m, &req(UseCase::Vision, Runtime::LlamaCpp), &cx);
    assert_eq!(d.hf_url, format!("https://huggingface.co/{}", m.repo_id));
    assert_eq!(d.ctx, 8192);
    let best = &d.variants[d.best.unwrap() as usize];
    assert_eq!(best.estimate.breakdown.vision, m.mmproj_bytes.unwrap());
    let cmd = &best.launch.commands[0].command;
    assert!(
        cmd.starts_with(&format!(
            "llama-server -hf {}:{} -c 8192 -ngl ",
            best.variant.repo_id, best.variant.label
        )),
        "{cmd}"
    );
    // Toutes les quantifications sont listées, de la plus petite à la plus grosse.
    assert!(
        d.variants
            .windows(2)
            .all(|w| w[0].variant.size_bytes <= w[1].variant.size_bytes)
    );
    let general = model_detail(m, &req(UseCase::General, Runtime::Ollama), &cx);
    assert!(
        general
            .variants
            .iter()
            .all(|v| v.estimate.breakdown.vision == 0)
    );
    assert!(
        general.variants[0].launch.commands[0]
            .command
            .starts_with("ollama run hf.co/")
    );
}

fn generated_at() -> DateTime<FixedOffset> {
    FixedOffset::east_opt(3600)
        .unwrap()
        .with_ymd_and_hms(2026, 11, 9, 22, 30, 0)
        .unwrap()
}

#[test]
fn export_markdown() {
    let hw = p2();
    let r = run(&sample(), &hw, &request(UseCase::Code, Some(16_384)));
    let md = to_markdown(&r, &hw, generated_at());
    assert!(md.starts_with("# Classement GRAM\n\n- Date : 09/11/2026 22:30\n"));
    assert!(md.contains("- Machine : GPU NVIDIA GeForce RTX 3060 (12,0 Go, ≈360 Go/s)"));
    assert!(md.contains("- Cas d'usage : Code · contexte 16 384 tokens · runtime Ollama"));
    assert!(md.contains("- Données : catalogue embarqué (09/11/2026 20:57)"));
    assert!(md.contains("| # | Modèle | Quantif. |"));
    assert!(!md.contains("- Recherche"));
    assert!(md.contains("## Exécutable mais lent : moins de 5 tok/s"));
    assert!(md.contains("## Trop gros"));
    let rows = md
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| #"))
        .count();
    assert_eq!(rows, r.items.len() + r.slow.len() + r.too_big.len());
}

#[test]
fn export_csv() {
    let r = run(&sample(), &p2(), &request(UseCase::General, None));
    let csv = to_csv(&r);
    assert!(csv.starts_with("\u{feff}section;rang;modele;"));
    let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1 + r.items.len() + r.slow.len() + r.too_big.len()
    );
    let first = lines[1];
    assert!(first.starts_with("executable;1;"), "{first}");
    // Chaque ligne a autant de champs que l'en-tête (aucun `;` hors guillemets dans les données).
    let fields = |l: &str| l.split(';').count();
    assert!(lines.iter().all(|l| fields(l) == fields(lines[0])), "{csv}");
}

#[test]
fn export_d_une_recherche() {
    let hw = p2();
    let all = run(&sample(), &hw, &request(UseCase::General, None));
    let r = run(
        &sample(),
        &hw,
        &RankRequest {
            search: Some("qwen3 8b".into()),
            ..request(UseCase::General, None)
        },
    );
    let md = to_markdown(&r, &hw, generated_at());
    assert!(md.contains("- Recherche : « qwen3 8b »"), "{md}");
    // Le rang exporté est celui du classement complet.
    let first = &r.items[0];
    let rank = all
        .items
        .iter()
        .position(|a| a.repo_id == first.repo_id)
        .unwrap()
        + 1;
    assert!(
        md.contains(&format!("| {rank} | {} |", first.display_name)),
        "{md}"
    );
    let csv = to_csv(&r);
    assert!(
        csv.contains(&format!("executable;{rank};{};", first.display_name)),
        "{csv}"
    );
}

// ── Modèles déjà installés dans Ollama (phase 7) ──────────────────────────────────────────

#[test]
fn quantifications_deja_installees_dans_ollama() {
    let models = sample();
    let m = models
        .iter()
        .find(|m| m.display_name == "Qwen3-8B")
        .expect("Qwen3-8B dans l'échantillon");
    let q4 = m
        .variants
        .iter()
        .find(|v| v.label == "Q4_K_M")
        .expect("Q4_K_M");
    let alt = &m.alternatives[0].repo_id;
    let rt = RuntimesStatus {
        ollama_models: vec![
            // Depuis le dépôt de la variante, en minuscules.
            OllamaModel::new(
                format!("hf.co/{}:q4_k_m", q4.repo_id.to_lowercase()),
                q4.size_bytes,
                None,
            ),
            // Depuis un dépôt alternatif, une quantification absente des candidates.
            OllamaModel::new(format!("hf.co/{alt}:Q2_K_XXS"), 1, None),
            // Bibliothèque d'Ollama : jamais rapprochée du catalogue.
            OllamaModel::new("qwen3:8b", 1, Some("Q4_K_M")),
        ],
        ..installed(true, false, false)
    };
    let hw = p2();
    let settings = Settings::default();
    let cx = RankContext::new(&hw, Some(&rt), &settings, now());
    let r = rank(&models, &request(UseCase::General, None), &cx, source());
    let item = r
        .items
        .iter()
        .chain(&r.slow)
        .find(|i| i.repo_id == m.repo_id)
        .unwrap();
    assert_eq!(item.installed, ["Q2_K_XXS", "Q4_K_M"]);
    let others = r.items.iter().filter(|i| i.repo_id != m.repo_id);
    assert!(others.clone().all(|i| i.installed.is_empty()));

    let d = model_detail(
        m,
        &DetailRequest {
            repo_id: m.repo_id.clone(),
            ..DetailRequest::default()
        },
        &cx,
    );
    let installed: Vec<(&str, &str)> = d
        .variants
        .iter()
        .filter_map(|v| Some((v.variant.label.as_str(), v.ollama_model.as_deref()?)))
        .collect();
    let expected = format!("hf.co/{}:q4_k_m", q4.repo_id.to_lowercase());
    assert_eq!(installed, [("Q4_K_M", expected.as_str())]);

    // Sans runtimes détectés : rien d'installé.
    let none = run(&models, &hw, &request(UseCase::General, None));
    assert!(none.items.iter().all(|i| i.installed.is_empty()));
}
