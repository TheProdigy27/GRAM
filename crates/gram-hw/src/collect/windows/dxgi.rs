//! Adaptateurs graphiques via DXGI (tous constructeurs), enrichis par DXCore.
//!
//! DXCore indique de façon fiable si un GPU est intégré (`IsIntegrated`) ; l'heuristique sur la
//! VRAM dédiée échoue sur les iGPU récents qui réservent 2 Gio ou plus. `dxcore.dll` est chargée
//! dynamiquement : une liaison statique empêcherait l'application de démarrer sur les versions
//! de Windows 10 qui ne l'ont pas (antérieures à 2004).

use crate::raw::RawAdapter;
use std::ffi::c_void;
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::DXCore::{
    DXCoreAdapterProperty, IDXCoreAdapter, IDXCoreAdapterFactory, IsHardware, IsIntegrated,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3, IDXGIFactory1,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{GUID, HRESULT, Interface, s, w};

pub fn adapters() -> windows::core::Result<Vec<RawAdapter>> {
    // SAFETY : appels DXGI documentés, sans pointeur fourni par l'appelant.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let dxcore = DxCore::load();
    if dxcore.is_none() {
        tracing::debug!("DXCore indisponible : heuristique pour les iGPU");
    }

    let mut out = Vec::new();
    for index in 0.. {
        // SAFETY : idem.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(a) => a,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(e),
        };
        // SAFETY : idem.
        let desc = unsafe { adapter.GetDesc1()? };
        let name_len = desc.Description.iter().position(|&c| c == 0).unwrap_or(128);
        let budget = adapter.cast::<IDXGIAdapter3>().ok().and_then(|a3| {
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            // SAFETY : `info` est une structure valide, possédée ici.
            unsafe { a3.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info) }
                .ok()
                .map(|()| info.Budget)
        });
        let (integrated, hardware) = dxcore
            .as_ref()
            .map_or((None, None), |d| d.flags(&desc.AdapterLuid));

        out.push(RawAdapter {
            index,
            name: String::from_utf16_lossy(&desc.Description[..name_len])
                .trim()
                .to_string(),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            sub_sys_id: desc.SubSysId,
            dedicated_video_memory: desc.DedicatedVideoMemory as u64,
            dedicated_system_memory: desc.DedicatedSystemMemory as u64,
            shared_system_memory: desc.SharedSystemMemory as u64,
            budget,
            software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
            integrated,
            hardware,
        });
    }
    Ok(out)
}

struct DxCore {
    factory: IDXCoreAdapterFactory,
}

type CreateFactoryFn = unsafe extern "system" fn(*const GUID, *mut *mut c_void) -> HRESULT;

impl DxCore {
    fn load() -> Option<Self> {
        // SAFETY : chargement limité à System32 ; la signature de `DXCoreCreateAdapterFactory`
        // est celle de dxcore.h (REFIID, void**) → HRESULT. Le module reste chargé (jamais libéré)
        // tant que la fabrique peut être utilisée.
        unsafe {
            let module =
                LoadLibraryExW(w!("dxcore.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
            let proc = GetProcAddress(module, s!("DXCoreCreateAdapterFactory"))?;
            let create =
                std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateFactoryFn>(proc);
            let mut raw = std::ptr::null_mut();
            create(&IDXCoreAdapterFactory::IID, &mut raw).ok().ok()?;
            Some(Self {
                factory: IDXCoreAdapterFactory::from_raw(raw),
            })
        }
    }

    /// (`IsIntegrated`, `IsHardware`) de l'adaptateur identifié par son LUID.
    fn flags(&self, luid: &LUID) -> (Option<bool>, Option<bool>) {
        // SAFETY : `luid` pointe vers une structure valide le temps de l'appel.
        let Ok(adapter) = (unsafe { self.factory.GetAdapterByLuid::<IDXCoreAdapter>(luid) }) else {
            return (None, None);
        };
        (
            bool_property(&adapter, IsIntegrated),
            bool_property(&adapter, IsHardware),
        )
    }
}

fn bool_property(adapter: &IDXCoreAdapter, property: DXCoreAdapterProperty) -> Option<bool> {
    // SAFETY : la taille demandée est vérifiée et ne dépasse jamais celle du tampon.
    unsafe {
        if !adapter.IsPropertySupported(property) {
            return None;
        }
        let size = adapter.GetPropertySize(property).ok()?;
        let mut buf = [0u8; 8];
        if size == 0 || size > buf.len() {
            return None;
        }
        adapter
            .GetProperty(property, size, buf.as_mut_ptr().cast())
            .ok()?;
        Some(buf[..size].iter().any(|&b| b != 0))
    }
}
