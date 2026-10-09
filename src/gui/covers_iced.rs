//! Carga de carátulas para la interfaz Iced.
//!
//! Mantiene la caché LRU global de `iced::widget::image::Handle` (respaldada por
//! la caché genérica [`CoverCache`] de `utils::covers`) y las funciones que la
//! usan. La caché de disco, el pool de codificación AVIF y la caché negativa
//! siguen en `utils::covers`, que no depende de ninguna interfaz.

use crate::utils::covers::{CoverCache, NEGATIVE_CACHE, get_max_covers};
use iced::widget::image::Handle;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Caché LRU global de Handles de carátulas, para prevenir OOM en Iced.
pub static LRU_COVER_CACHE: OnceLock<Mutex<CoverCache<Handle>>> = OnceLock::new();

/// Obtiene o inicializa la caché LRU global de carátulas.
pub fn get_lru_cache() -> &'static Mutex<CoverCache<Handle>> {
    LRU_COVER_CACHE.get_or_init(|| Mutex::new(CoverCache::new(get_max_covers())))
}

/// Carga una carátula desde disco usando Handle::from_path() de Iced.
/// Iced gestiona internamente la decodificación y la memoria.
/// Arquitectura: Disco AVIF → Iced (sin caché RAM intermedia).
pub fn load_cover_handle(path: &str) -> Option<Handle> {
    let neg_cache_mtx = NEGATIVE_CACHE.get_or_init(|| Mutex::new(HashSet::new()));

    // 1. Verificar si el archivo ya falló anteriormente
    if neg_cache_mtx.lock().contains(path) {
        return None;
    }

    // 2. Revisar la Caché LRU en memoria
    let cache_mtx = get_lru_cache();
    let mut cache = cache_mtx.lock();

    // Recalcular límite dinámico en cada carga (el modo pudo cambiar); al reducir,
    // `resize` desaloja primero las entradas menos recientes.
    cache.resize(get_max_covers());

    if let Some(handle) = cache.get(path) {
        return Some(handle);
    }

    // 3. Crear nuevo Handle y guardar en caché; `put` desaloja si excede la capacidad.
    let handle = Handle::from_path(path);
    cache.put(path.to_string(), handle.clone());

    Some(handle)
}

/// Purga las carátulas más viejas de la caché.
/// Usado por el Garbage Collector cuando la app está inactiva.
/// En low-resource, purga el doble de elementos por ciclo.
pub fn purge_old_covers(count: usize) {
    let cache_mtx = get_lru_cache();
    let mut cache = cache_mtx.lock();
    cache.purge_oldest(count, crate::utils::is_low_resource());
}

/// Precarga carátulas para una lista de rutas de álbumes visibles.
/// Útil para evitar frames en blanco al hacer scroll por la biblioteca:
/// las carátulas se cargan en la LRU cache antes de que el usuario las vea.
pub fn preload_visible_covers(paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let cache_mtx = get_lru_cache();
    let mut cache = cache_mtx.lock();
    for path in paths {
        if cache.contains(path) {
            // Ya en caché: promover a MRU sin reinsertar.
            cache.promote(path);
        } else {
            // No está: cargar desde disco y guardar en LRU.
            let handle = Handle::from_path(path);
            cache.put(path.clone(), handle);
        }
    }
}

/// Solo se usa cuando no hay carátula en caché de disco (datos embebidos del archivo de audio).
/// Carga una imagen desde bytes crudos para usar en Iced (fallback para datos embebidos).
pub fn load_raw_image_for_iced(data: &[u8]) -> Option<Handle> {
    if data.is_empty() {
        return None;
    }
    Some(Handle::from_bytes(data.to_vec()))
}

/// Limpia la caché LRU completa de carátulas (llamado al cambiar de canción).
pub fn clear_raw_cache() {
    // Vaciar LRU completo al limpiar
    let cache_mtx = get_lru_cache();
    let mut cache = cache_mtx.lock();
    cache.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `from_path` deriva el `Id` de la ruta: dos handles de la misma ruta son iguales.
    fn handle_for(i: usize) -> Handle {
        Handle::from_path(format!("cache/covers/{i}.avif"))
    }

    #[test]
    fn the_generic_cache_keeps_iced_handles_in_lru_order() {
        let mut cache: CoverCache<Handle> = CoverCache::new(2);
        cache.put("a".into(), handle_for(0));
        cache.put("b".into(), handle_for(1));
        assert_eq!(cache.get("a"), Some(handle_for(0)));
        cache.put("c".into(), handle_for(2));

        assert_eq!(cache.order_mru_to_lru(), vec!["c", "a"]);
        assert_eq!(cache.peek("c"), Some(handle_for(2)));
        assert!(!cache.contains("b"));
    }

    #[test]
    fn load_raw_image_for_iced_rejects_empty_data() {
        assert!(load_raw_image_for_iced(&[]).is_none());
        assert!(load_raw_image_for_iced(&[1, 2, 3]).is_some());
    }
}
