//! Sistema de caché y procesamiento de carátulas de álbumes.
//!
//! Escala imágenes a 400×400 (o 200×200 en modo low-resource),
//! las comprime en formato AVIF al 90% de calidad y las almacena
//! en `cache/covers/` con un LRU cache en RAM para Iced.

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::avif::AvifEncoder;
use image::{ExtendedColorType, ImageEncoder};
use lru::LruCache;
use parking_lot::Mutex;
use rayon::ThreadPoolBuilder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Instancia estática del Thread Pool limitado para tareas de fondo de carátulas.
pub static COVER_POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

/// Canal limitado para controlar la presión de memoria durante el escaneado
static COVER_GATEWAY: OnceLock<crossbeam::channel::Sender<(Vec<u8>, String)>> = OnceLock::new();

/// Almacena rutas de archivos que fallaron o no existen (evita reintentos)
pub static NEGATIVE_CACHE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Caché LRU genérica de carátulas, indexada por ruta.
///
/// `H` es el tipo de handle de imagen de la interfaz que la usa (por ejemplo,
/// `iced::widget::image::Handle` en `gui::covers_iced`). La caché no conoce
/// ninguna interfaz: solo limita cuántos handles se retienen para prevenir OOM.
pub struct CoverCache<H: Clone> {
    /// Entradas ordenadas por recencia (la más reciente al frente).
    entries: LruCache<String, H>,
}

impl<H: Clone> CoverCache<H> {
    /// Creates the cache with the given capacity; a capacity of 0 is treated as 1.
    pub fn new(cap: usize) -> Self {
        Self {
            entries: LruCache::new(non_zero_capacity(cap)),
        }
    }

    /// Devuelve el handle y promueve la entrada a la posición más reciente.
    pub fn get(&mut self, key: &str) -> Option<H> {
        self.entries.get(key).cloned()
    }

    /// Consulta la entrada sin alterar el orden de recencia.
    pub fn peek(&self, key: &str) -> Option<H> {
        self.entries.peek(key).cloned()
    }

    /// Inserta o actualiza la entrada, promoviéndola a la posición más reciente.
    pub fn put(&mut self, key: String, value: H) {
        self.entries.put(key, value);
    }

    /// Promueve una clave existente sin insertarla si no está presente.
    pub fn promote(&mut self, key: &str) -> bool {
        self.entries.promote(key)
    }

    /// Ajusta la capacidad; al reducir desaloja primero las menos recientes.
    /// A capacity of 0 is treated as 1.
    pub fn resize(&mut self, cap: usize) {
        let cap = non_zero_capacity(cap);
        if self.entries.cap() != cap {
            self.entries.resize(cap);
        }
    }

    /// Número de entradas retenidas.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the cache holds no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Indica si la clave está en la caché, sin alterar el orden de recencia.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains(key)
    }

    /// Desaloja las `count` entradas menos recientes. En modo low-resource
    /// duplica el número de entradas a purgar.
    pub fn purge_oldest(&mut self, count: usize, low_resource: bool) {
        let effective_count = if low_resource { count * 2 } else { count };
        let to_remove = effective_count.min(self.entries.len());

        for _ in 0..to_remove {
            self.entries.pop_lru();
        }
    }

    /// Vacía por completo la caché.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Orden de más reciente a menos reciente.
    pub fn order_mru_to_lru(&self) -> Vec<String> {
        self.entries.iter().map(|(key, _)| key.clone()).collect()
    }
}

/// Converts a requested capacity to the `NonZeroUsize` the `lru` crate needs.
fn non_zero_capacity(cap: usize) -> NonZeroUsize {
    NonZeroUsize::new(cap).unwrap_or(NonZeroUsize::MIN)
}

/// Límite dinámico de carátulas en caché según modo:
/// - Normal: 64 (estándar, suficiente para pantallas grandes)
/// - Low-resource: 16 (evicción agresiva para ahorrar RAM)
pub fn get_max_covers() -> usize {
    if crate::utils::is_low_resource() {
        16
    } else {
        64
    }
}

/// Obtiene o inicializa el ThreadPool global para procesamiento de carátulas.
///
/// En modo low-resource usa 1 hilo; en modo normal usa 1/3 de los núcleos.
pub fn get_cover_pool() -> &'static rayon::ThreadPool {
    COVER_POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // Low-resource: máx 1 hilo. Normal: 1/3 de los procesadores (para evitar sobrecalentamiento)
        let pool_size = if crate::utils::is_low_resource() {
            1
        } else {
            (cores / 3).max(1)
        };

        ThreadPoolBuilder::new()
            .num_threads(pool_size)
            .stack_size(8 * 1024 * 1024) // 8MB por hilo — estabilidad con AVIF/ravif
            .thread_name(|idx| format!("cover-cache-worker-{}", idx))
            .build()
            .unwrap()
    })
}

/// Encola una tarea de procesamiento de carátula de forma segura.
/// Si hay demasiadas tareas pendientes (16), el hilo del escáner esperará,
/// evitando clonar miles de búferes de imagen en la RAM simultáneamente.
pub fn enqueue_cover_job(data: Vec<u8>, hash: String) {
    let tx = COVER_GATEWAY.get_or_init(|| {
        let (tx, rx) = crossbeam::channel::bounded::<(Vec<u8>, String)>(16);

        let pool = get_cover_pool();

        // Iniciamos hilos persistentes en el pool que consumen del canal
        for _ in 0..pool.current_num_threads() {
            let rx_worker = rx.clone();
            pool.spawn(move || {
                while let Ok((pic, name)) = rx_worker.recv() {
                    let _ = std::panic::catch_unwind(move || {
                        if let Err(e) = process_and_save_cover(&pic, &name) {
                            tracing::warn!(
                                "No se pudo procesar/guardar la carátula '{}': {}",
                                name,
                                e
                            );
                        }
                    });
                }
            });
        }

        tx
    });

    let _ = tx.send((data, hash));
}

/// Vacía el conjunto de rutas que fallaron al cargar.
fn clear_negative_cache(set: &mut HashSet<String>) {
    set.clear();
}

/// Limpia la caché negativa (archivos que fallaron). Útil tras re-escaneo.
pub fn clear_all_cover_cache() {
    if let Some(neg_cache_mtx) = NEGATIVE_CACHE.get() {
        clear_negative_cache(&mut neg_cache_mtx.lock());
    }
}

/// Genera un ID único para una imagen específica de canción (u8 data)
pub fn generate_pic_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Devuelve la extensión de CPU a forzar, o `None` para conservar el default
/// de la librería (que ya selecciona la mejor extensión disponible en runtime).
///
/// Solo se fuerza AVX2 cuando el procesador la soporta; si no, no se toca nada.
fn forced_cpu_extension() -> Option<fast_image_resize::CpuExtensions> {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            return Some(fast_image_resize::CpuExtensions::Avx2);
        }
    }
    None
}

/// Extrae de un buffer raw de imagen, recorta/escala a 400x400 y guarda como .avif 90% de calidad.
pub fn process_and_save_cover(data: &[u8], safe_album_name: &str) -> std::io::Result<PathBuf> {
    let cache_dir = PathBuf::from("cache/covers");
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }

    let dst_path = cache_dir.join(format!("{}.avif", safe_album_name));
    if dst_path.exists() {
        return Ok(dst_path);
    }

    // 1. Cargar imagen en memoria y convertirla genéricamente a RGBA 8 bit
    let img = match image::load_from_memory(data) {
        Ok(i) => i.to_rgba8(),
        Err(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to decode image",
            ));
        }
    };

    let width = img.width();
    let height = img.height();

    if width == 0 || height == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Dimensiones inválidas: 0",
        ));
    }

    let target_size = if crate::utils::is_low_resource() {
        200
    } else {
        400
    };

    let src_image = Image::from_vec_u8(
        width,
        height,
        img.into_raw(),
        fast_image_resize::PixelType::U8x4,
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let dst_width = target_size;
    let dst_height = target_size;
    let mut dst_image = Image::new(dst_width, dst_height, src_image.pixel_type());

    let mut resizer = Resizer::new();

    if let Some(ext) = forced_cpu_extension() {
        // SAFETY: `forced_cpu_extension` solo devuelve `Avx2` tras comprobar en
        // runtime con `is_x86_feature_detected!("avx2")` que el procesador la
        // soporta, de modo que la extensión está garantizada en este punto.
        unsafe {
            resizer.set_cpu_extensions(ext);
        }
    }

    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));

    resizer
        .resize(&src_image, &mut dst_image, &options)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    // Guardar como AVIF con calidad 90 y velocidad rápida (8) para ahorrar RAM/CPU
    let file = std::fs::File::create(&dst_path)?;
    let encoder = AvifEncoder::new_with_speed_quality(file, 8, 90);

    encoder
        .write_image(
            dst_image.buffer(),
            dst_width,
            dst_height,
            ExtendedColorType::Rgba8.into(),
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    Ok(dst_path)
}

/// A cover decoded to tightly packed 8-bit RGBA pixels, independent of any GUI toolkit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedCover {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA8 pixels, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Decodes an image file (AVIF, PNG, JPEG or WebP) to RGBA8.
///
/// The format is detected from the file content. When the longer edge exceeds
/// `max_edge`, the image is downscaled (Lanczos3, aspect ratio preserved) so the
/// longer edge equals `max_edge`; smaller images are never upscaled. Returns
/// `None` on any failure (missing or unreadable file, unknown format, decode
/// error, zero-sized image or `max_edge == 0`); the reason is logged at debug
/// level.
pub fn decode_cover_rgba(path: &std::path::Path, max_edge: u32) -> Option<DecodedCover> {
    match try_decode_cover_rgba(path, max_edge) {
        Ok(cover) => Some(cover),
        Err(reason) => {
            tracing::debug!("No se pudo decodificar la carátula {:?}: {}", path, reason);
            None
        }
    }
}

/// Fallible body of [`decode_cover_rgba`]; the error is a human-readable reason.
fn try_decode_cover_rgba(path: &std::path::Path, max_edge: u32) -> Result<DecodedCover, String> {
    if max_edge == 0 {
        return Err("max_edge is 0".to_string());
    }
    let image = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();

    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err("zero-sized image".to_string());
    }

    let (dst_width, dst_height) = fit_within(width, height, max_edge);
    if (dst_width, dst_height) == (width, height) {
        return Ok(DecodedCover {
            width,
            height,
            rgba: image.into_raw(),
        });
    }

    let src_image = Image::from_vec_u8(
        width,
        height,
        image.into_raw(),
        fast_image_resize::PixelType::U8x4,
    )
    .map_err(|e| e.to_string())?;
    let mut dst_image = Image::new(dst_width, dst_height, src_image.pixel_type());

    let mut resizer = Resizer::new();
    if let Some(ext) = forced_cpu_extension() {
        // SAFETY: `forced_cpu_extension` solo devuelve `Avx2` tras comprobar en
        // runtime con `is_x86_feature_detected!("avx2")` que el procesador la
        // soporta, de modo que la extensión está garantizada en este punto.
        unsafe {
            resizer.set_cpu_extensions(ext);
        }
    }
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    resizer
        .resize(&src_image, &mut dst_image, &options)
        .map_err(|e| e.to_string())?;

    Ok(DecodedCover {
        width: dst_width,
        height: dst_height,
        rgba: dst_image.into_vec(),
    })
}

/// Size that fits `width` x `height` inside a `max_edge` square, preserving the
/// aspect ratio and never upscaling. Each edge is at least 1 pixel.
fn fit_within(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let longer = width.max(height);
    if longer <= max_edge {
        return (width, height);
    }
    let scale = |edge: u32| -> u32 {
        let scaled =
            (u64::from(edge) * u64::from(max_edge) + u64::from(longer) / 2) / u64::from(longer);
        u32::try_from(scaled).unwrap_or(max_edge).clamp(1, max_edge)
    };
    (scale(width), scale(height))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    /// Handle de prueba, independiente de cualquier interfaz: basta con que sea
    /// `Clone` y comparable para que el arnés compare rastros.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Handle(usize);

    /// Handle determinista y comparable derivado del índice.
    fn test_handle(i: usize) -> Handle {
        Handle(i)
    }

    /// Operaciones mínimas que el arnés de paridad necesita de cada implementación.
    trait ParityCache {
        fn get(&mut self, key: &str) -> Option<Handle>;
        fn peek(&self, key: &str) -> Option<Handle>;
        fn insert(&mut self, key: String, value: Handle);
        fn clear(&mut self);
        fn purge_oldest(&mut self, count: usize, low_resource: bool);
        fn resize(&mut self, cap: usize);
        fn len(&self) -> usize;
        fn contains(&self, key: &str) -> bool;
        /// Orden de más reciente a menos reciente.
        fn order_mru_to_lru(&self) -> Vec<String>;
    }

    /// Adaptador de paridad sobre la caché de producción (respaldada por `lru`).
    struct ProductionCache {
        cache: CoverCache<Handle>,
    }

    impl ProductionCache {
        fn new(cap: usize) -> Self {
            Self {
                cache: CoverCache::new(cap),
            }
        }
    }

    impl ParityCache for ProductionCache {
        fn get(&mut self, key: &str) -> Option<Handle> {
            self.cache.get(key)
        }

        fn peek(&self, key: &str) -> Option<Handle> {
            self.cache.peek(key)
        }

        fn insert(&mut self, key: String, value: Handle) {
            self.cache.put(key, value);
        }

        fn clear(&mut self) {
            self.cache.clear();
        }

        fn purge_oldest(&mut self, count: usize, low_resource: bool) {
            self.cache.purge_oldest(count, low_resource);
        }

        fn resize(&mut self, cap: usize) {
            self.cache.resize(cap);
        }

        fn len(&self) -> usize {
            self.cache.len()
        }

        fn contains(&self, key: &str) -> bool {
            self.cache.contains(key)
        }

        fn order_mru_to_lru(&self) -> Vec<String> {
            self.cache.order_mru_to_lru()
        }
    }

    /// Congela la semántica de la caché manual previa a la adopción de `lru`
    /// (mapa de claves a `Handle` + cola de orden de acceso). Es una referencia
    /// independiente de la implementación de producción: una divergencia real
    /// en capacidad, recencia, desalojo, purga o resize hace fallar el test.
    struct ReferenceCache {
        map: HashMap<String, Handle>,
        order: VecDeque<String>,
        cap: usize,
    }

    impl ReferenceCache {
        fn new(cap: usize) -> Self {
            Self {
                map: HashMap::with_capacity(cap),
                order: VecDeque::with_capacity(cap),
                cap,
            }
        }
    }

    impl ParityCache for ReferenceCache {
        fn get(&mut self, key: &str) -> Option<Handle> {
            let handle = self.map.get(key).cloned()?;
            // El acierto promueve: la clave se mueve al final de la cola.
            if let Some(idx) = self.order.iter().position(|x| x == key) {
                self.order.remove(idx);
                self.order.push_back(key.to_string());
            }
            Some(handle)
        }

        fn peek(&self, key: &str) -> Option<Handle> {
            self.map.get(key).cloned()
        }

        fn insert(&mut self, key: String, value: Handle) {
            if self.map.insert(key.clone(), value).is_some() {
                // Reinsertar una clave existente también la promueve.
                if let Some(idx) = self.order.iter().position(|x| x == &key) {
                    self.order.remove(idx);
                }
            }
            self.order.push_back(key);
            while self.order.len() > self.cap {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }

        fn clear(&mut self) {
            self.map.clear();
            self.order.clear();
        }

        fn purge_oldest(&mut self, count: usize, low_resource: bool) {
            let effective_count = if low_resource { count * 2 } else { count };
            let to_remove = effective_count.min(self.order.len());
            for _ in 0..to_remove {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }

        fn resize(&mut self, cap: usize) {
            self.cap = cap;
            while self.order.len() > self.cap {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }

        fn len(&self) -> usize {
            self.order.len()
        }

        fn contains(&self, key: &str) -> bool {
            self.map.contains_key(key)
        }

        fn order_mru_to_lru(&self) -> Vec<String> {
            self.order.iter().rev().cloned().collect()
        }
    }

    /// Ejecuta la misma secuencia contra la caché de producción y la referencia
    /// manual independiente, y exige que el rastro observable y el orden final
    /// coincidan. La referencia no usa `lru`, así que una divergencia de la
    /// implementación real contra la semántica documentada hace fallar el test.
    fn assert_parity(cap: usize, scenario: impl Fn(&mut dyn ParityCache) -> Vec<String>) {
        let mut production = ProductionCache::new(cap);
        let mut reference = ReferenceCache::new(cap);

        let production_trace = scenario(&mut production);
        let reference_trace = scenario(&mut reference);

        assert_eq!(
            production_trace, reference_trace,
            "rastro divergente en cap={cap}"
        );
        assert_eq!(
            production.order_mru_to_lru(),
            reference.order_mru_to_lru(),
            "orden divergente en cap={cap}"
        );
    }

    /// Contrato explícito de recencia, acierto y desalojo, sin comparar contra
    /// ninguna otra implementación: fija el orden MRU→LRU esperado.
    #[test]
    fn covercache_golden_lru_order() {
        let mut cache: CoverCache<Handle> = CoverCache::new(3);
        cache.put("a".into(), test_handle(0));
        cache.put("b".into(), test_handle(1));
        cache.put("c".into(), test_handle(2));
        assert_eq!(cache.order_mru_to_lru().join(","), "c,b,a");

        let _ = cache.get("a");
        assert_eq!(cache.order_mru_to_lru().join(","), "a,c,b");

        // `peek` consulta sin alterar el orden de recencia.
        assert!(cache.peek("b").is_some());
        assert_eq!(cache.order_mru_to_lru().join(","), "a,c,b");

        // Insertar desaloja la menos reciente ("b"), no la recién leída.
        cache.put("d".into(), test_handle(3));
        assert_eq!(cache.order_mru_to_lru().join(","), "d,a,c");
        assert!(!cache.contains("b"));
    }

    /// Inserta `cap` claves numeradas y devuelve sus nombres en orden de inserción.
    fn fill(cache: &mut dyn ParityCache, cap: usize) -> Vec<String> {
        let keys: Vec<String> = (0..cap).map(|i| format!("k{i}")).collect();
        for (i, key) in keys.iter().enumerate() {
            cache.insert(key.clone(), test_handle(i));
        }
        keys
    }

    #[test]
    fn covercache_capacity_bound_parity() {
        for cap in [4usize, 16, 64] {
            assert_parity(cap, |cache| {
                for i in 0..cap + 5 {
                    cache.insert(format!("k{i}"), test_handle(i));
                }
                vec![format!("len={}", cache.len())]
            });
        }
    }

    #[test]
    fn covercache_recency_eviction_order_parity() {
        for cap in [3usize, 4, 16, 64] {
            assert_parity(cap, |cache| {
                let keys = fill(cache, cap);
                let first = keys[0].clone();
                let second = keys[1].clone();
                // Leer la más antigua la promueve; insertar una nueva desaloja la
                // siguiente más antigua, no la recién leída.
                let _ = cache.get(&first);
                cache.insert("nueva".into(), test_handle(999));
                vec![
                    format!("len={}", cache.len()),
                    format!("first_present={}", cache.contains(&first)),
                    format!("second_present={}", cache.contains(&second)),
                    format!("order={:?}", cache.order_mru_to_lru()),
                ]
            });
        }
    }

    #[test]
    fn covercache_get_promotes_but_peek_does_not() {
        for cap in [4usize, 16, 64] {
            assert_parity(cap, |cache| {
                let keys = fill(cache, cap.min(3));
                let before = cache.order_mru_to_lru();

                let _ = cache.peek(&keys[0]);
                let after_peek = cache.order_mru_to_lru();

                let _ = cache.get(&keys[0]);
                let after_get = cache.order_mru_to_lru();

                vec![
                    format!("peek_preserves_order={}", before == after_peek),
                    format!(
                        "get_promotes_to_mru={}",
                        after_get.first() == Some(&keys[0])
                    ),
                ]
            });
        }
    }

    /// Estrés de concurrencia bajo un único `Mutex`: ningún hilo puede dejar la
    /// caché por encima de la capacidad ni provocar un pánico.
    fn concurrency_stress<C: ParityCache + Send>(mutex: &Mutex<C>, cap: usize) {
        std::thread::scope(|scope| {
            for thread in 0..8usize {
                scope.spawn(move || {
                    for i in 0..200usize {
                        let key = format!("/music/album/{thread}/{i}");
                        let mut guard = mutex.lock();
                        guard.insert(key.clone(), test_handle(i));
                        let _ = guard.get(&key);
                        let _ = guard.get("/music/album/ausente");
                        assert!(guard.len() <= cap, "la caché excedió su capacidad");
                    }
                });
            }
        });
        assert!(mutex.lock().len() <= cap);
    }

    #[test]
    fn covercache_concurrency_parity() {
        let cap = 8;
        concurrency_stress(&Mutex::new(ProductionCache::new(cap)), cap);
        concurrency_stress(&Mutex::new(ReferenceCache::new(cap)), cap);
    }

    #[test]
    fn covercache_clear_parity() {
        for cap in [4usize, 16, 64] {
            assert_parity(cap, |cache| {
                fill(cache, cap);
                cache.clear();
                vec![
                    format!("len={}", cache.len()),
                    format!("order={:?}", cache.order_mru_to_lru()),
                ]
            });
        }
    }

    #[test]
    fn covercache_negative_cache_clear_is_isolated() {
        let mut negative: HashSet<String> = HashSet::new();
        for i in 0..3 {
            negative.insert(format!("missing-{i}"));
        }

        // Ambas implementaciones parten con entradas que no deben verse afectadas.
        let mut production = ProductionCache::new(4);
        production.insert("keep".into(), test_handle(1));
        let mut reference = ReferenceCache::new(4);
        reference.insert("keep".into(), test_handle(1));

        clear_negative_cache(&mut negative);

        assert!(
            negative.is_empty(),
            "el conjunto negativo debe quedar vacío"
        );
        // Limpiar el conjunto negativo es independiente de la caché LRU en ambos casos.
        assert_eq!(production.len(), 1);
        assert_eq!(reference.len(), 1);
    }

    #[test]
    fn covercache_purge_parity() {
        for cap in [4usize, 16] {
            for (count, low_resource) in [(1usize, false), (2usize, true)] {
                assert_parity(cap, |cache| {
                    fill(cache, cap);
                    let newest = format!("k{}", cap - 1);
                    cache.purge_oldest(count, low_resource);

                    let removed = if low_resource { count * 2 } else { count }.min(cap);
                    // La purga quita las menos recientes; la más nueva sobrevive
                    // mientras no se haya purgado la caché entera.
                    assert_eq!(cache.len(), cap - removed);
                    if removed < cap {
                        assert!(cache.contains(&newest));
                        assert!(!cache.contains("k0"));
                    } else {
                        assert!(cache.order_mru_to_lru().is_empty());
                    }

                    vec![
                        format!("len={}", cache.len()),
                        format!("order={:?}", cache.order_mru_to_lru()),
                    ]
                });
            }
        }
    }

    #[test]
    fn covercache_resize_parity() {
        for start in [16usize, 64] {
            for target in [4usize, 8] {
                assert_parity(start, |cache| {
                    fill(cache, start);
                    let newest = format!("k{}", start - 1);

                    // Reducir la capacidad desaloja primero las menos recientes.
                    cache.resize(target);
                    assert_eq!(
                        cache.len(),
                        target,
                        "reducir debe respetar la nueva capacidad"
                    );
                    assert!(cache.contains(&newest), "la más reciente debe sobrevivir");
                    assert!(!cache.contains("k0"), "la menos reciente debe desalojarse");
                    let shrink_order = cache.order_mru_to_lru();

                    // Ampliar la capacidad no desaloja ninguna entrada.
                    cache.resize(start + 16);
                    assert_eq!(cache.len(), target, "ampliar no debe desalojar");

                    vec![
                        format!("shrink_len={target}"),
                        format!("order={shrink_order:?}"),
                        format!("grow_len={}", cache.len()),
                    ]
                });
            }
        }
    }

    #[test]
    fn avx2_selection_matches_detection() {
        let forced = forced_cpu_extension();
        // En x86_64: forzamos Avx2 solo si el host lo soporta.
        #[cfg(target_arch = "x86_64")]
        assert_eq!(forced.is_some(), std::is_x86_feature_detected!("avx2"));
        // Fuera de x86_64 nunca forzamos nada.
        #[cfg(not(target_arch = "x86_64"))]
        assert!(forced.is_none());
    }

    #[test]
    fn process_and_save_cover_smoke() {
        // Imagen RGBA en memoria → PNG en bytes, para recorrer el camino real
        // de decodificación, redimensionado y codificación AVIF.
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("no se pudo codificar el PNG de prueba");

        let path = process_and_save_cover(&bytes, "audoxidy_smoke_test_cover")
            .expect("process_and_save_cover falló en el smoke test");

        assert!(path.exists(), "la carátula AVIF no se creó en disco");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("avif"));

        // Limpia solo el archivo generado; el directorio cache/covers/ es un
        // artefacto de runtime ignorado por git y se deja en su sitio.
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_zero_capacity_is_treated_as_one_entry() {
        let mut cache: CoverCache<Handle> = CoverCache::new(0);
        assert!(cache.is_empty());
        cache.put("a".into(), test_handle(0));
        cache.put("b".into(), test_handle(1));
        assert_eq!(cache.len(), 1);
        assert!(cache.contains("b"));

        cache.resize(0);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn promote_moves_an_existing_key_without_inserting_a_missing_one() {
        let mut cache: CoverCache<Handle> = CoverCache::new(3);
        cache.put("a".into(), test_handle(0));
        cache.put("b".into(), test_handle(1));
        assert!(cache.promote("a"));
        assert_eq!(cache.order_mru_to_lru(), vec!["a", "b"]);
        assert!(!cache.promote("missing"));
        assert_eq!(cache.len(), 2);
    }

    /// Path to a fresh file in the system temp dir, unique per process and call.
    fn unique_temp_path(name: &str, extension: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "audoxidy_cover_test_{}_{}_{}.{}",
            std::process::id(),
            name,
            n,
            extension
        ))
    }

    /// Writes `img` as PNG to a unique temp file and returns its path.
    fn write_png(img: &image::RgbaImage, name: &str) -> std::path::PathBuf {
        let path = unique_temp_path(name, "png");
        img.save_with_format(&path, image::ImageFormat::Png)
            .expect("no se pudo escribir el PNG de prueba");
        path
    }

    /// Asserts that two RGBA pixels differ by at most `tolerance` per channel.
    fn assert_pixel_near(actual: &[u8], expected: [u8; 4], tolerance: u8) {
        for (a, e) in actual.iter().zip(expected.iter()) {
            assert!(
                a.abs_diff(*e) <= tolerance,
                "pixel {actual:?} differs from {expected:?} by more than {tolerance}"
            );
        }
    }

    #[test]
    fn decode_cover_rgba_reads_a_png_without_resizing_when_it_already_fits() {
        let mut img = image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]));
        img.put_pixel(3, 1, image::Rgba([200, 100, 50, 128]));
        let path = write_png(&img, "fits");

        let decoded = decode_cover_rgba(&path, 4).expect("the PNG should decode");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (4, 2));
        assert_eq!(decoded.rgba.len(), 4 * 2 * 4);
        assert_eq!(&decoded.rgba[0..4], &[10, 20, 30, 255]);
        // Row 1, column 3 of a 4-pixel-wide image.
        let last = (4 + 3) * 4;
        assert_eq!(&decoded.rgba[last..last + 4], &[200, 100, 50, 128]);
    }

    #[test]
    fn decode_cover_rgba_downscales_so_the_longer_edge_matches_max_edge() {
        let img = image::RgbaImage::from_pixel(100, 50, image::Rgba([255, 0, 0, 255]));
        let path = write_png(&img, "landscape");
        let decoded = decode_cover_rgba(&path, 20).expect("the PNG should decode");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (20, 10));
        assert_eq!(decoded.rgba.len(), 20 * 10 * 4);
        assert_pixel_near(&decoded.rgba[0..4], [255, 0, 0, 255], 1);

        let img = image::RgbaImage::from_pixel(30, 90, image::Rgba([0, 0, 255, 255]));
        let path = write_png(&img, "portrait");
        let decoded = decode_cover_rgba(&path, 45).expect("the PNG should decode");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (15, 45));
        assert_pixel_near(&decoded.rgba[0..4], [0, 0, 255, 255], 1);
    }

    #[test]
    fn decode_cover_rgba_never_upscales_a_small_image() {
        let img = image::RgbaImage::from_pixel(10, 5, image::Rgba([1, 2, 3, 255]));
        let path = write_png(&img, "small");
        let decoded = decode_cover_rgba(&path, 400).expect("the PNG should decode");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (10, 5));
        assert_eq!(&decoded.rgba[0..4], &[1, 2, 3, 255]);
    }

    #[test]
    fn decode_cover_rgba_reads_an_avif_encoded_by_the_image_crate() {
        let img = image::RgbaImage::from_pixel(16, 8, image::Rgba([0, 200, 0, 255]));
        let path = unique_temp_path("avif", "avif");
        let file = std::fs::File::create(&path).expect("no se pudo crear el AVIF de prueba");
        AvifEncoder::new_with_speed_quality(file, 10, 100)
            .write_image(img.as_raw(), 16, 8, ExtendedColorType::Rgba8)
            .expect("no se pudo codificar el AVIF de prueba");

        let decoded = decode_cover_rgba(&path, 400).expect("the AVIF should decode");
        let full = decode_cover_rgba(&path, 8).expect("the AVIF should decode");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (16, 8));
        // AVIF is lossy: the solid colour survives within a small tolerance.
        assert_pixel_near(&decoded.rgba[0..4], [0, 200, 0, 255], 12);
        assert_eq!((full.width, full.height), (8, 4));
    }

    #[test]
    fn decode_cover_rgba_returns_none_for_a_missing_file() {
        let path = unique_temp_path("missing", "png");
        assert!(decode_cover_rgba(&path, 400).is_none());
    }

    #[test]
    fn decode_cover_rgba_returns_none_for_bytes_that_are_not_an_image() {
        let path = unique_temp_path("garbage", "png");
        std::fs::write(&path, b"definitely not an image").expect("write test file");
        let decoded = decode_cover_rgba(&path, 400);
        let _ = std::fs::remove_file(&path);
        assert!(decoded.is_none());
    }

    #[test]
    fn decode_cover_rgba_returns_none_for_a_zero_max_edge() {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
        let path = write_png(&img, "zero_edge");
        let decoded = decode_cover_rgba(&path, 0);
        let _ = std::fs::remove_file(&path);
        assert!(decoded.is_none());
    }

    #[test]
    fn fit_within_preserves_the_aspect_ratio_and_keeps_edges_at_least_one_pixel() {
        assert_eq!(fit_within(400, 400, 200), (200, 200));
        assert_eq!(fit_within(1000, 10, 100), (100, 1));
        assert_eq!(fit_within(10_000, 1, 100), (100, 1));
        assert_eq!(fit_within(50, 50, 50), (50, 50));
    }
}
