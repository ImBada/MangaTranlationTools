//! The LibTorch models (LaMa, the text segmentation model) stay loaded between uses and
//! are unloaded after `IDLE_LIMIT` without one; LibTorch itself stays loaded. They run one
//! inference at a time between them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mtt_torch::Device;

/// A model that has not been used for this long is unloaded (each holds a few hundred MB).
const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

pub(crate) struct ModelCache<T> {
    /// For the log.
    name: &'static str,
    /// The device a loaded model runs on.
    device: fn(&T) -> Device,
    slot: Mutex<Option<Loaded<T>>>,
}

struct Loaded<T> {
    path: PathBuf,
    model: Arc<T>,
    last_used: Instant,
}

impl<T: Send + Sync + 'static> ModelCache<T> {
    pub(crate) const fn new(name: &'static str, device: fn(&T) -> Device) -> Self {
        Self { name, device, slot: Mutex::new(None) }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Loaded<T>>> {
        self.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The model from `path`, loaded with `load` (once LibTorch is, from `runtime`) unless
    /// it already is.
    pub(crate) fn get(
        &'static self,
        path: &Path,
        runtime: &Path,
        load: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        let mut guard = self.lock();
        if let Some(loaded) = guard.as_mut().filter(|loaded| loaded.path == path) {
            loaded.last_used = Instant::now();
            return Ok(Arc::clone(&loaded.model));
        }
        let started = Instant::now();
        mtt_torch::load(runtime).map_err(|e| format!("LibTorch를 불러오지 못했습니다: {e}"))?;
        let model = Arc::new(load()?);
        log::info!("{} loaded on {:?} in {:.2}s", self.name, (self.device)(&model), started.elapsed().as_secs_f32());
        let was_loaded = guard.is_some();
        *guard = Some(Loaded { path: path.to_owned(), model: Arc::clone(&model), last_used: Instant::now() });
        if !was_loaded {
            self.start_idle_watch();
        }
        Ok(model)
    }

    /// Where the model runs (or will, before it has loaded).
    pub(crate) fn device_label(&self) -> &'static str {
        match self.lock().as_ref().map(|loaded| (self.device)(&loaded.model)) {
            Some(Device::Mps) => "GPU (MPS)",
            Some(Device::Cpu) => "CPU",
            None if cfg!(target_os = "macos") => "GPU (MPS)",
            None => "CPU",
        }
    }

    /// Unloads the model once it has been idle for `IDLE_LIMIT`.
    fn start_idle_watch(&'static self) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(30));
                let mut guard = self.lock();
                match guard.as_ref() {
                    Some(loaded) if loaded.last_used.elapsed() >= IDLE_LIMIT => {
                        log::info!("{} idle, unloading the model", self.name);
                        *guard = None;
                        return;
                    }
                    Some(_) => {}
                    None => return,
                }
            }
        });
    }
}

/// Held for an inference: the GPU (or every CPU core) is busy anyway, and LibTorch's MPS
/// stream is shared.
pub(crate) fn exclusive() -> MutexGuard<'static, ()> {
    static RUNNING: Mutex<()> = Mutex::new(());
    RUNNING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
