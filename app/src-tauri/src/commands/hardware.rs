//! Matériel et runtimes installés.

use crate::error::CmdResult;
use crate::state::AppState;
use gram_core::{HardwareProfile, Overrides, RuntimesStatus};
use gram_hw::GpuSpecs;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

/// Un GPU de `data/gpu_specs.json`, proposé dans le mode simulation.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SimulationGpu {
    /// Valeur de `Overrides::simulated_gpu` (ex. « NVIDIA GeForce RTX 4090 24 Go »).
    pub label: String,
    /// VRAM, en octets.
    pub vram: u64,
    pub bandwidth_gbs: f32,
}

/// Données du dialogue « Corriger… » : le matériel tel que détecté (sans corrections ni
/// simulation) et les GPU qu'on peut simuler.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HardwareOptions {
    pub detected: HardwareProfile,
    /// Par constructeur, puis par VRAM et bande passante croissantes.
    pub gpus: Vec<SimulationGpu>,
}

fn simulation_gpus(specs: &GpuSpecs) -> Vec<SimulationGpu> {
    let mut gpus: Vec<(u8, SimulationGpu)> = specs
        .all()
        .map(|s| {
            let vendor = match s.vendor() {
                gram_core::Vendor::Nvidia => 0,
                gram_core::Vendor::Amd => 1,
                gram_core::Vendor::Intel => 2,
                gram_core::Vendor::Other => 3,
            };
            let gpu = SimulationGpu {
                label: s.label(),
                vram: s.vram_bytes(),
                bandwidth_gbs: s.bandwidth_gbs,
            };
            (vendor, gpu)
        })
        .collect();
    gpus.sort_by(|(va, a), (vb, b)| {
        va.cmp(vb)
            .then(a.vram.cmp(&b.vram))
            .then(a.bandwidth_gbs.total_cmp(&b.bandwidth_gbs))
            .then_with(|| a.label.cmp(&b.label))
    });
    gpus.into_iter().map(|(_, g)| g).collect()
}

/// Profil matériel, détecté au premier appel (normalement dès le démarrage) puis gardé en
/// mémoire.
#[tauri::command]
pub async fn get_hardware<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<HardwareProfile> {
    state.hardware(&app).await
}

/// Nouvelle détection (bouton « Ré-analyser »), en gardant les corrections.
#[tauri::command]
pub async fn rescan_hardware<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<HardwareProfile> {
    state.rescan(&app).await
}

/// Corrections manuelles et simulation (§6.6) ; renvoie le profil qui en résulte, utilisé par
/// les classements suivants. `Overrides` vide : retour aux valeurs détectées. Les corrections
/// sont enregistrées (`corrections.json`), pas la simulation.
#[tauri::command]
pub async fn set_hardware_overrides<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    overrides: Overrides,
) -> CmdResult<HardwareProfile> {
    state.set_overrides(&app, overrides).await
}

/// Matériel détecté et GPU simulables, pour le dialogue « Corriger… ».
#[tauri::command]
pub async fn hardware_options<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<HardwareOptions> {
    Ok(HardwareOptions {
        detected: state.detected(&app).await?,
        gpus: simulation_gpus(GpuSpecs::embedded()),
    })
}

/// Ollama, LM Studio et llama.cpp : installés, en cours d'exécution.
#[tauri::command]
pub async fn detect_runtimes(state: State<'_, AppState>) -> CmdResult<RuntimesStatus> {
    state.detect_runtimes().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpus_simulables_tries() {
        let gpus = simulation_gpus(GpuSpecs::embedded());
        assert!(gpus.len() > 50);
        let pos = |label: &str| gpus.iter().position(|g| g.label == label).unwrap();
        // NVIDIA, puis AMD, puis Intel ; par VRAM croissante chez chacun.
        assert!(pos("NVIDIA GeForce RTX 3060 12 Go") < pos("NVIDIA GeForce RTX 4090 24 Go"));
        assert!(pos("NVIDIA GeForce RTX 5090 32 Go") < pos("AMD Radeon RX 7800 XT 16 Go"));
        assert!(pos("AMD Radeon RX 7900 XTX 24 Go") < pos("Intel Arc A770 16 Go"));
        let g = &gpus[pos("NVIDIA GeForce RTX 4090 24 Go")];
        assert_eq!((g.vram, g.bandwidth_gbs), (24 << 30, 1008.0));
    }
}
