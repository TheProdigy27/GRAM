//! `system_profiler` (macOS) : type de la mémoire unifiée, barrettes d'un Mac Intel, cœurs et
//! VRAM des GPU. La lecture de la réponse est pure, testée sur tous les systèmes.

use crate::raw::{RawHardware, RawMemoryModule};
use crate::specs::normalize_name;
use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Chemin absolu : une application lancée depuis le Finder n'a qu'un `PATH` minimal.
const SYSTEM_PROFILER: &str = "/usr/sbin/system_profiler";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Ce que `system_profiler` apprend en plus de sysinfo et Metal.
#[derive(Debug, Default, PartialEq)]
pub struct Profile {
    /// Type de la mémoire unifiée (Apple Silicon), ex. « LPDDR5 ».
    pub memory_type: Option<String>,
    /// Barrettes (Mac Intel).
    pub modules: Vec<RawMemoryModule>,
    /// Emplacements, vides compris (Mac Intel).
    pub slots: Option<u32>,
    pub gpus: Vec<ProfiledGpu>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ProfiledGpu {
    /// Ex. « Apple M1 Pro », « AMD Radeon Pro 5500M ».
    pub model: String,
    pub cores: Option<u32>,
    /// Octets.
    pub vram: Option<u64>,
}

/// Lance `system_profiler` et lit sa réponse. Bloquant (≈ 0,3 s).
pub fn query() -> Result<Profile, String> {
    parse(&run()?)
}

fn run() -> Result<Vec<u8>, String> {
    let mut child = Command::new(SYSTEM_PROFILER)
        .args(["-json", "SPMemoryDataType", "SPDisplaysDataType"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("lancement impossible : {e}"))?;
    let mut stdout = child.stdout.take().ok_or("sortie non redirigée")?;
    // Lue à part : un tampon de tube plein bloquerait `system_profiler`.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return reader
                    .join()
                    .map_err(|_| "lecture interrompue".to_string())?
                    .map_err(|e| e.to_string());
            }
            Ok(Some(status)) => return Err(format!("échec ({status})")),
            Ok(None) if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("pas de réponse en {} s", TIMEOUT.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Lit la réponse JSON de `system_profiler -json SPMemoryDataType SPDisplaysDataType`.
pub fn parse(json: &[u8]) -> Result<Profile, String> {
    let root: Value =
        serde_json::from_slice(json).map_err(|e| format!("réponse illisible : {e}"))?;
    let entries = |key: &str| {
        root.get(key)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
    };
    let mut profile = Profile::default();

    for entry in entries("SPMemoryDataType") {
        // Apple Silicon : une seule entrée, sans barrette.
        if let Some(kind) = text(entry, "dimm_type") {
            profile.memory_type = Some(kind);
        }
        // Mac Intel : une entrée « Memory Slots » qui liste les emplacements.
        if let Some(items) = entry.get("_items").and_then(Value::as_array) {
            *profile.slots.get_or_insert(0) += items.len() as u32;
            profile.modules.extend(items.iter().filter_map(module));
        }
    }

    for entry in entries("SPDisplaysDataType") {
        let Some(model) = text(entry, "sppci_model").or_else(|| text(entry, "_name")) else {
            continue;
        };
        profile.gpus.push(ProfiledGpu {
            model,
            cores: text(entry, "sppci_cores").and_then(|c| c.parse().ok()),
            vram: [
                "spdisplays_vram",
                "_spdisplays_vram",
                "spdisplays_vram_shared",
            ]
            .iter()
            .find_map(|k| text(entry, k))
            .and_then(|v| size_bytes(&v)),
        });
    }
    Ok(profile)
}

impl Profile {
    /// Complète les données de sysinfo et de Metal.
    pub fn apply(self, raw: &mut RawHardware) {
        raw.memory.type_name = self.memory_type;
        if !self.modules.is_empty() {
            raw.memory.modules = self.modules;
            raw.memory.slots = self.slots;
        }
        // Cœurs et VRAM, rapprochés des GPU Metal par leur nom (« Intel(R) UHD Graphics 630 »
        // pour Metal, « Intel UHD Graphics 630 » ici). Une entrée ne sert qu'une fois.
        let mut used = vec![false; self.gpus.len()];
        for device in &mut raw.metal {
            let key = normalize_name(&device.name).key;
            let found = (0..self.gpus.len())
                .find(|&j| !used[j] && normalize_name(&self.gpus[j].model).key == key);
            if let Some(j) = found {
                used[j] = true;
                device.gpu_cores = self.gpus[j].cores;
                device.vram = self.gpus[j].vram;
            }
        }
    }
}

/// Une barrette d'un Mac Intel : « BANK 0/ChannelA-DIMM0 », « 8 GB », « 2667 MHz », « DDR4 ».
/// Un emplacement vide (« empty ») n'en est pas une.
fn module(item: &Value) -> Option<RawMemoryModule> {
    let capacity = size_bytes(&text(item, "dimm_size")?)?;
    let name = text(item, "_name").unwrap_or_default();
    let (bank, locator) = name.split_once('/').unwrap_or(("", &name));
    Some(RawMemoryModule {
        capacity,
        // Apple écrit « MHz », mais c'est le débit (DDR4-2667 : « 2667 MHz »).
        speed: text(item, "dimm_speed").and_then(|s| number(&s).map(|(n, _)| n as u32)),
        configured_speed: None,
        smbios_type: None,
        type_name: text(item, "dimm_type").filter(|t| !t.eq_ignore_ascii_case("empty")),
        data_width: None,
        device_locator: locator.trim().to_string(),
        bank_label: bank.trim().to_string(),
    })
}

fn text(v: &Value, key: &str) -> Option<String> {
    let s = v.get(key)?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// « 2667 MHz » → (2667, « MHz »).
fn number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(s.len());
    let n: f64 = s[..end].replace(',', ".").parse().ok()?;
    Some((n, s[end..].trim()))
}

/// « 16 GB », « 1536 MB » → octets (Apple compte en puissances de 2).
fn size_bytes(s: &str) -> Option<u64> {
    let (n, unit) = number(s)?;
    let scale = match unit.to_ascii_uppercase().as_str() {
        "TB" => 1u64 << 40,
        "GB" => 1 << 30,
        "MB" => 1 << 20,
        "KB" => 1 << 10,
        _ => return None,
    };
    (n > 0.0).then_some((n * scale as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::RawMetalDevice;

    const GIB: u64 = 1 << 30;

    /// MacBook Pro M1 Pro 16 Go (réponse réelle, numéros de série retirés).
    const APPLE_SILICON: &str = r#"{
      "SPDisplaysDataType" : [ {
          "_name" : "Apple M1 Pro",
          "spdisplays_mtlgpufamilysupport" : "spdisplays_metal4",
          "spdisplays_ndrvs" : [ { "_name" : "Color LCD", "spdisplays_main" : "spdisplays_yes" } ],
          "spdisplays_vendor" : "sppci_vendor_Apple",
          "sppci_bus" : "spdisplays_builtin",
          "sppci_cores" : "16",
          "sppci_device_type" : "spdisplays_gpu",
          "sppci_model" : "Apple M1 Pro"
      } ],
      "SPMemoryDataType" : [ {
          "dimm_manufacturer" : "Samsung",
          "dimm_type" : "LPDDR5",
          "SPMemoryDataType" : "16 GB"
      } ]
    }"#;

    /// MacBook Pro 16 pouces 2019 (Intel) : iGPU Intel + Radeon Pro 5500M, 2 × 8 Go DDR4.
    const INTEL_MAC: &str = r#"{
      "SPDisplaysDataType" : [
        { "_name" : "kHW_IntelUHDGraphics630Item", "sppci_model" : "Intel UHD Graphics 630",
          "spdisplays_vram_shared" : "1536 MB", "spdisplays_vendor" : "sppci_vendor_intel" },
        { "_name" : "kHW_AMDRadeonPro5500MItem", "sppci_model" : "AMD Radeon Pro 5500M",
          "spdisplays_vram" : "4 GB", "spdisplays_vendor" : "sppci_vendor_amd" }
      ],
      "SPMemoryDataType" : [ {
          "_name" : "Memory Slots",
          "is_memory_upgradeable" : "No",
          "_items" : [
            { "_name" : "BANK 0/ChannelA-DIMM0", "dimm_size" : "8 GB", "dimm_speed" : "2667 MHz",
              "dimm_status" : "ok", "dimm_type" : "DDR4" },
            { "_name" : "BANK 2/ChannelB-DIMM0", "dimm_size" : "8 GB", "dimm_speed" : "2667 MHz",
              "dimm_status" : "ok", "dimm_type" : "DDR4" }
          ]
      } ]
    }"#;

    #[test]
    fn apple_silicon() {
        let p = parse(APPLE_SILICON.as_bytes()).unwrap();
        assert_eq!(p.memory_type.as_deref(), Some("LPDDR5"));
        assert!(p.modules.is_empty() && p.slots.is_none());
        assert_eq!(
            p.gpus,
            [ProfiledGpu {
                model: "Apple M1 Pro".into(),
                cores: Some(16),
                vram: None
            }]
        );
    }

    #[test]
    fn mac_intel_barrettes_et_deux_gpu() {
        let p = parse(INTEL_MAC.as_bytes()).unwrap();
        assert_eq!(p.slots, Some(2));
        assert_eq!(p.modules.len(), 2);
        let m = &p.modules[1];
        assert_eq!((m.capacity, m.speed), (8 * GIB, Some(2667)));
        assert_eq!(m.type_name.as_deref(), Some("DDR4"));
        assert_eq!(
            (m.bank_label.as_str(), m.device_locator.as_str()),
            ("BANK 2", "ChannelB-DIMM0")
        );
        assert_eq!(p.gpus[0].vram, Some(1536 << 20));
        assert_eq!(p.gpus[1].vram, Some(4 * GIB));
    }

    #[test]
    fn rapprochement_avec_metal() {
        let mut raw = RawHardware {
            metal: vec![
                RawMetalDevice {
                    index: 0,
                    name: "AMD Radeon Pro 5500M".into(),
                    ..Default::default()
                },
                RawMetalDevice {
                    index: 1,
                    name: "Intel(R) UHD Graphics 630".into(),
                    low_power: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        parse(INTEL_MAC.as_bytes()).unwrap().apply(&mut raw);
        assert_eq!(raw.metal[0].vram, Some(4 * GIB));
        assert_eq!(raw.metal[1].vram, Some(1536 << 20));
        assert_eq!(raw.memory.modules.len(), 2);
        assert_eq!(raw.memory.slots, Some(2));
        assert_eq!(raw.memory.type_name, None);
    }

    #[test]
    fn emplacements_vides_et_reponses_incompletes() {
        let json = r#"{ "SPMemoryDataType" : [ { "_items" : [
            { "_name" : "DIMM1", "dimm_size" : "16 GB", "dimm_speed" : "2666 MHz", "dimm_type" : "DDR4" },
            { "_name" : "DIMM2", "dimm_size" : "empty", "dimm_type" : "empty" } ] } ] }"#;
        let p = parse(json.as_bytes()).unwrap();
        assert_eq!((p.slots, p.modules.len()), (Some(2), 1));
        assert_eq!(p.modules[0].device_locator, "DIMM1");
        assert_eq!(parse(b"{}").unwrap(), Profile::default());
        assert!(parse(b"pas du json").is_err());
    }

    #[test]
    fn tailles() {
        assert_eq!(size_bytes("16 GB"), Some(16 * GIB));
        assert_eq!(size_bytes("1536 MB"), Some(1536 << 20));
        assert_eq!(size_bytes("1,5 GB"), Some(3 * GIB / 2));
        assert_eq!(size_bytes("empty"), None);
        assert_eq!(size_bytes("0 MB"), None);
    }
}
