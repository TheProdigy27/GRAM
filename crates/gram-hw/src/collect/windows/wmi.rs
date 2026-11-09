//! CPU et barrettes mémoire via WMI (`Win32_Processor`, `Win32_PhysicalMemory`).
//!
//! Ne pas utiliser `Win32_VideoController.AdapterRAM` : entier 32 bits plafonné à 4 Go (§6.7).

use crate::raw::{RawHardware, RawMemoryModule};
use serde::Deserialize;
use wmi::WMIConnection;

#[derive(Deserialize)]
#[serde(rename = "Win32_Processor", rename_all = "PascalCase")]
struct Processor {
    name: Option<String>,
    number_of_cores: Option<u32>,
    number_of_logical_processors: Option<u32>,
    max_clock_speed: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename = "Win32_PhysicalMemory", rename_all = "PascalCase")]
struct PhysicalMemory {
    capacity: Option<u64>,
    speed: Option<u32>,
    configured_clock_speed: Option<u32>,
    #[serde(rename = "SMBIOSMemoryType")]
    smbios_memory_type: Option<u32>,
    data_width: Option<u32>,
    device_locator: Option<String>,
    bank_label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename = "Win32_PhysicalMemoryArray", rename_all = "PascalCase")]
struct PhysicalMemoryArray {
    memory_devices: Option<u32>,
    /// 3 = mémoire système.
    #[serde(rename = "Use")]
    usage: Option<u32>,
}

/// Résultat des requêtes WMI ; chaque requête peut échouer indépendamment.
pub struct WmiData {
    processors: Result<Vec<Processor>, String>,
    modules: Result<Vec<PhysicalMemory>, String>,
    arrays: Result<Vec<PhysicalMemoryArray>, String>,
}

pub fn query() -> Result<WmiData, String> {
    let con = WMIConnection::new().map_err(|e| e.to_string())?;
    Ok(WmiData {
        processors: con.query().map_err(|e| e.to_string()),
        modules: con.query().map_err(|e| e.to_string()),
        arrays: con.query().map_err(|e| e.to_string()),
    })
}

impl WmiData {
    /// Complète les données de sysinfo avec celles de WMI, plus précises sous Windows.
    pub fn apply(self, raw: &mut RawHardware) {
        match self.processors {
            Ok(procs) if !procs.is_empty() => {
                // Une ligne par socket.
                if let Some(name) = procs.iter().find_map(|p| p.name.as_deref()) {
                    raw.cpu.name = name.trim().to_string();
                }
                let sum = |f: fn(&Processor) -> Option<u32>| -> Option<u32> {
                    procs.iter().map(f).sum::<Option<u32>>().filter(|&n| n > 0)
                };
                if let Some(n) = sum(|p| p.number_of_cores) {
                    raw.cpu.cores_physical = n;
                }
                if let Some(n) = sum(|p| p.number_of_logical_processors) {
                    raw.cpu.cores_logical = n;
                }
                raw.cpu.max_clock_mhz = procs.iter().filter_map(|p| p.max_clock_speed).max();
            }
            Ok(_) => {}
            Err(e) => note(raw, "Win32_Processor", &e),
        }

        match self.modules {
            Ok(mods) => {
                raw.memory.modules = mods
                    .into_iter()
                    .map(|m| RawMemoryModule {
                        capacity: m.capacity.unwrap_or(0),
                        speed: m.speed,
                        configured_speed: m.configured_clock_speed,
                        smbios_type: m.smbios_memory_type,
                        data_width: m.data_width,
                        device_locator: m.device_locator.unwrap_or_default().trim().to_string(),
                        bank_label: m.bank_label.unwrap_or_default().trim().to_string(),
                    })
                    .collect();
            }
            Err(e) => note(raw, "Win32_PhysicalMemory", &e),
        }

        match self.arrays {
            Ok(arrays) => {
                let slots: u32 = arrays
                    .iter()
                    .filter(|a| a.usage.is_none_or(|u| u == 3))
                    .filter_map(|a| a.memory_devices)
                    .sum();
                raw.memory.slots = (slots > 0).then_some(slots);
            }
            Err(e) => note(raw, "Win32_PhysicalMemoryArray", &e),
        }
    }
}

fn note(raw: &mut RawHardware, class: &str, error: &str) {
    tracing::warn!(class, error, "requête WMI en échec");
    raw.notes
        .push(format!("Requête WMI {class} en échec : {error}"));
}
