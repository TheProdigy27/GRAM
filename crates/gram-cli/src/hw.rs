//! `gram-cli hw` : matériel détecté, en texte (à comparer au Gestionnaire des tâches, ou à
//! « Informations système » sur un Mac) ou en JSON.

use anyhow::Result;
use gram_core::fmt::{gb, gb_round, num};
use gram_core::{
    CpuArch, GpuInfo, HardwareProfile, Overrides, RuntimeStatus, RuntimesStatus, Source,
};
use serde::Serialize;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HwReport<'a> {
    hardware: &'a HardwareProfile,
    runtimes: &'a RuntimesStatus,
}

pub fn run(json: bool, raw: bool) -> Result<()> {
    let start = Instant::now();
    let data = gram_hw::collect();
    if raw {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }
    let hw = gram_hw::build_profile(&data, &Overrides::default());
    let elapsed = start.elapsed();
    let runtimes = gram_hw::detect_runtimes();
    if json {
        let report = HwReport {
            hardware: &hw,
            runtimes: &runtimes,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&hw, &runtimes, Some(elapsed)));
    }
    Ok(())
}

const LABEL_WIDTH: usize = 10;

fn line(out: &mut String, label: &str, text: &str) {
    let _ = writeln!(out, "{label:<LABEL_WIDTH$}{text}");
}

fn source(s: Source) -> &'static str {
    match s {
        Source::Detected => "détecté",
        Source::Nvml => "NVML",
        Source::Table => "table",
        Source::Estimated => "estimé",
        Source::Default => "par défaut",
        Source::Manual => "manuel",
    }
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

pub fn render(hw: &HardwareProfile, rt: &RuntimesStatus, elapsed: Option<Duration>) -> String {
    let mut out = String::new();
    let indent = "";

    // CPU
    let c = &hw.cpu;
    line(&mut out, "CPU", &c.name);
    let mut parts = vec![format!(
        "{} / {}",
        plural(c.cores_physical, "cœur", "cœurs"),
        plural(c.cores_logical, "thread", "threads")
    )];
    if let Some(mhz) = c.max_clock_mhz {
        parts.push(format!("{} GHz", num(f64::from(mhz) / 1000.0, 1)));
    }
    let flags: Vec<&str> = [
        (c.avx2, "AVX2"),
        (c.avx512, "AVX-512"),
        (c.fma, "FMA"),
        (c.f16c, "F16C"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if c.arch == CpuArch::Arm {
        parts.push("ARM64".into());
    } else if !flags.is_empty() {
        parts.push(flags.join(" "));
    }
    line(&mut out, indent, &parts.join(" · "));

    // RAM
    let r = &hw.ram;
    let size = r.installed.unwrap_or(r.total);
    let kind = match r.speed_mts {
        Some(s) => format!("{}-{s}", r.kind.label()),
        None => format!("{}, vitesse inconnue", r.kind.label()),
    };
    let unified = hw.gpu.as_ref().is_some_and(|g| g.unified_memory);
    let modules = match r.slots {
        _ if unified && r.modules == 0 => "mémoire unifiée".to_string(),
        Some(slots) => format!(
            "{} / {slots} emplacements",
            plural(r.modules, "barrette", "barrettes")
        ),
        None => plural(r.modules, "barrette", "barrettes"),
    };
    line(&mut out, "RAM", &format!("{} {kind} · {modules}", gb(size)));
    let mut parts = vec![format!(
        "{} ({})",
        plural(r.channels, "canal", "canaux"),
        source(r.channels_source)
    )];
    if let Some(bw) = r.bandwidth_gbs {
        parts.push(format!("≈ {} Go/s", num(f64::from(bw), 1)));
    }
    parts.push(format!("{} vus par le système", gb(r.total)));
    parts.push(format!("{} disponibles", gb(r.available)));
    line(&mut out, indent, &parts.join(" · "));

    // GPU
    match &hw.gpu {
        Some(g) => gpu_lines(&mut out, "GPU", g, "retenu"),
        None => line(&mut out, "GPU", "aucun"),
    }
    for g in &hw.other_gpus {
        gpu_lines(&mut out, indent, g, "non utilisé");
    }

    // Backends
    let b = &hw.backends;
    let mut parts = Vec::new();
    if b.cuda {
        parts.push(match &b.cuda_driver_version {
            Some(v) => format!("CUDA {v} (pilote)"),
            None => "CUDA".to_string(),
        });
    }
    if b.vulkan {
        parts.push("Vulkan".to_string());
    }
    if b.metal {
        parts.push("Metal".to_string());
    }
    line(
        &mut out,
        "Backends",
        &if parts.is_empty() {
            "aucun".to_string()
        } else {
            parts.join(" · ")
        },
    );

    // Runtimes
    let runtimes = [
        ("Ollama", &rt.ollama),
        ("LM Studio", &rt.lm_studio),
        ("llama.cpp", &rt.llama_cpp),
    ];
    for (i, (name, s)) in runtimes.into_iter().enumerate() {
        line(
            &mut out,
            if i == 0 { "Runtimes" } else { indent },
            &format!("{name} : {}", runtime(s)),
        );
    }

    // Disques
    for (i, d) in hw.disks.iter().enumerate() {
        line(
            &mut out,
            if i == 0 { "Disques" } else { indent },
            &format!(
                "{} {} dont {} libres",
                d.mount,
                gb_round(d.total),
                gb_round(d.available)
            ),
        );
    }

    if !hw.warnings.is_empty() {
        out.push_str("\nAvertissements\n");
        for w in &hw.warnings {
            let _ = writeln!(out, "  • {w}");
        }
    }
    if let Some(e) = elapsed {
        let _ = writeln!(out, "\nDétection en {} ms.", e.as_millis());
    }
    out
}

fn gpu_lines(out: &mut String, label: &str, g: &GpuInfo, role: &str) {
    let kind = if g.unified_memory {
        "mémoire unifiée, "
    } else if g.integrated {
        "intégré, "
    } else {
        ""
    };
    line(
        out,
        label,
        &format!("{} ({kind}{role}, n° {})", g.name, g.index),
    );
    let mut parts = vec![match g.vram_free {
        _ if g.unified_memory => format!(
            "{} utilisables par le GPU (limite de Metal)",
            gb(g.vram_total)
        ),
        Some(free) => format!("{} de VRAM ({} libres)", gb(g.vram_total), gb(free)),
        None => format!("{} de VRAM", gb(g.vram_total)),
    }];
    if let Some(bw) = g.bandwidth_gbs {
        parts.push(format!(
            "≈ {} Go/s ({})",
            num(f64::from(bw), 0),
            source(g.bandwidth_source)
        ));
    }
    if let Some((major, minor)) = g.cuda_cc {
        parts.push(format!("compute capability {major}.{minor}"));
    }
    line(out, "", &format!("  {}", parts.join(" · ")));
}

fn runtime(s: &RuntimeStatus) -> String {
    if !s.installed {
        return "non installé".into();
    }
    let mut text = if s.running {
        "en cours d'exécution"
    } else {
        "installé"
    }
    .to_string();
    if let Some(v) = &s.version {
        let _ = write!(text, ", version {v}");
    }
    if let Some(p) = &s.path {
        let _ = write!(text, " ({p})");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_machine() -> HardwareProfile {
        let raw: gram_hw::RawHardware = serde_json::from_str(include_str!(
            "../../../tests/fixtures/hardware/raw-9800x3d-rtx5080-parsec.json"
        ))
        .unwrap();
        gram_hw::build_profile(&raw, &Overrides::default())
    }

    #[test]
    fn rendu_texte() {
        let rt = RuntimesStatus {
            ollama: RuntimeStatus {
                installed: true,
                running: true,
                path: None,
                version: Some("0.13.1".into()),
            },
            ..Default::default()
        };
        let text = render(&dev_machine(), &rt, None);
        let expected = [
            "CPU       AMD Ryzen 7 9800X3D 8-Core Processor",
            "8 cœurs / 16 threads · 4,7 GHz · AVX2 AVX-512 FMA F16C",
            "RAM       64,0 Go DDR5-6000 · 2 barrettes / 4 emplacements",
            "2 canaux (estimé) · ≈ 96,0 Go/s",
            "GPU       NVIDIA GeForce RTX 5080 (retenu, n° 0)",
            "15,9 Go de VRAM",
            "≈ 960 Go/s (table) · compute capability 12.0",
            "AMD Radeon(TM) Graphics (intégré, non utilisé, n° 1)",
            "Backends  CUDA 13.4 (pilote) · Vulkan",
            "Runtimes  Ollama : en cours d'exécution, version 0.13.1",
            "LM Studio : non installé",
            r"Disques   C:\ 1 862 Go",
        ];
        for e in expected {
            assert!(text.contains(e), "« {e} » absent de :\n{text}");
        }
        assert!(!text.contains("Avertissements"));
    }

    #[test]
    fn rendu_mac_apple_silicon() {
        let raw: gram_hw::RawHardware = serde_json::from_str(include_str!(
            "../../../tests/fixtures/hardware/raw-m1pro-16go-macos.json"
        ))
        .unwrap();
        let hw = gram_hw::build_profile(&raw, &Overrides::default());
        let text = render(&hw, &RuntimesStatus::default(), None);
        let expected = [
            "CPU       Apple M1 Pro",
            "10 cœurs / 10 threads · 3,2 GHz · ARM64",
            "RAM       16,0 Go LPDDR5-6400 · mémoire unifiée",
            "4 canaux (table) · ≈ 204,8 Go/s · 16,0 Go vus par le système",
            "GPU       Apple M1 Pro (mémoire unifiée, retenu, n° 0)",
            "11,8 Go utilisables par le GPU (limite de Metal) · ≈ 205 Go/s (table)",
            "Backends  Metal",
            "Disques   / 460 Go",
        ];
        for e in expected {
            assert!(text.contains(e), "« {e} » absent de :\n{text}");
        }
        assert!(!text.contains("Avertissements"), "{text}");
    }

    #[test]
    fn rendu_avec_avertissements_sans_gpu() {
        let mut hw = dev_machine();
        hw.gpu = None;
        hw.other_gpus.clear();
        hw.warnings.push("Aucun GPU détecté".into());
        let text = render(
            &hw,
            &RuntimesStatus::default(),
            Some(Duration::from_millis(42)),
        );
        assert!(text.contains("GPU       aucun"));
        assert!(text.contains("Avertissements\n  • Aucun GPU détecté"));
        assert!(text.ends_with("Détection en 42 ms.\n"));
    }
}
