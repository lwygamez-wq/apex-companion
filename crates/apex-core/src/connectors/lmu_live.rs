//! Lecture de la mémoire partagée `LMU_Data` (Windows uniquement). Toute la logique (conversion,
//! enregistrement, coach) est dans `lmu` et `ingest::lmu`, testée hors Windows : ce module se
//! contente d'ouvrir la projection et d'en copier une image cohérente.

use super::lmu_shm::{read_frame_ptr, Frame, LMU_DATA_NAME, LMU_DATA_SIZE};
use super::ConnectorError;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery, FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct Mapping {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

// Les handles Win32 et la vue en lecture seule sont utilisables depuis n'importe quel fil.
unsafe impl Send for Mapping {}

impl Mapping {
    pub fn open() -> Result<Self, ConnectorError> {
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide(LMU_DATA_NAME).as_ptr());
            if handle.is_null() {
                return Err(ConnectorError::Import("mémoire partagée LMU absente : active « Enable Plugins » dans Réglages → Gameplay, puis redémarre le jeu".into()));
            }
            let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                CloseHandle(handle);
                return Err(ConnectorError::Import("lecture de la mémoire partagée LMU impossible".into()));
            }
            // Projection plus petite que la disposition connue (autre version du jeu) : on ne lit rien
            // plutôt que de lire hors de la vue.
            let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
            let n = VirtualQuery(view.Value, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
            if n == 0 || info.RegionSize < LMU_DATA_SIZE {
                UnmapViewOfFile(view);
                CloseHandle(handle);
                return Err(ConnectorError::Import(format!("version de LMU non prise en charge (mémoire partagée de {} octets)", info.RegionSize)));
            }
            Ok(Self { handle, view })
        }
    }

    pub fn read(&mut self) -> Option<Frame> {
        // SAFETY : la vue couvre au moins LMU_DATA_SIZE octets (vérifié à l'ouverture).
        unsafe { read_frame_ptr(self.view.Value as *const u8) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}
