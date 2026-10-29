//! NVIDIA's driver API (`nvcuda.dll`, `libcuda.so.1`): whether CUDA can run, before the
//! CUDA build is downloaded or loaded. After koharu's `koharu-runtime/src/hardware/cuda.rs`
//! (https://github.com/koharu-rs/koharu, MIT OR Apache-2.0).

use std::ffi::{CStr, c_char, c_int, c_uint};
use std::sync::OnceLock;

/// An NVIDIA GPU as the driver reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaGpu {
    /// The CUDA device ordinal, LibTorch's `cuda:<index>`.
    pub index: usize,
    pub name: String,
    /// `10 × major + minor` (8.6 → 86).
    pub compute_capability: u32,
    /// Total memory in bytes.
    pub memory: u64,
    /// Shares memory with the CPU (Jetson and the like).
    pub integrated: bool,
}

/// The NVIDIA driver and its GPUs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDriver {
    /// `cuDriverGetVersion`: the newest CUDA the driver runs, `1000 × major + 10 × minor`
    /// (13030 = 13.3).
    pub version: i32,
    pub gpus: Vec<CudaGpu>,
}

/// The driver and its GPUs, probed once per process (`cuInit` starts the driver, so not
/// on the UI thread). `Err` says why there is none; always on macOS.
pub fn cuda_driver() -> Result<CudaDriver, String> {
    static DRIVER: OnceLock<Result<CudaDriver, String>> = OnceLock::new();
    DRIVER.get_or_init(probe).clone()
}

type Init = unsafe extern "C" fn(c_uint) -> c_int;
type DriverGetVersion = unsafe extern "C" fn(*mut c_int) -> c_int;
type DeviceGetCount = unsafe extern "C" fn(*mut c_int) -> c_int;
type DeviceGet = unsafe extern "C" fn(*mut c_int, c_int) -> c_int;
type DeviceGetAttribute = unsafe extern "C" fn(*mut c_int, c_int, c_int) -> c_int;
type DeviceGetName = unsafe extern "C" fn(*mut c_char, c_int, c_int) -> c_int;
type DeviceTotalMemory = unsafe extern "C" fn(*mut usize, c_int) -> c_int;

const SUCCESS: c_int = 0;
const ERROR_NO_DEVICE: c_int = 100;
const ATTRIBUTE_INTEGRATED: c_int = 18;
const ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR: c_int = 75;
const ATTRIBUTE_COMPUTE_CAPABILITY_MINOR: c_int = 76;

/// The driver's functions; the library stays loaded for the process.
struct Driver {
    get_device: DeviceGet,
    get_attribute: DeviceGetAttribute,
    get_name: DeviceGetName,
    get_memory: DeviceTotalMemory,
}

fn probe() -> Result<CudaDriver, String> {
    let library = unsafe { open_driver() }?;
    // Safety: the driver API's signatures (cuda.h); `cuDeviceTotalMem_v2` is the size_t one.
    let symbol = |name: &str| format!("NVIDIA 드라이버에 {name}이(가) 없습니다");
    let (init, get_version, get_count, driver) = unsafe {
        (
            *library.get::<Init>(b"cuInit\0").map_err(|_| symbol("cuInit"))?,
            *library.get::<DriverGetVersion>(b"cuDriverGetVersion\0").map_err(|_| symbol("cuDriverGetVersion"))?,
            *library.get::<DeviceGetCount>(b"cuDeviceGetCount\0").map_err(|_| symbol("cuDeviceGetCount"))?,
            Driver {
                get_device: *library.get::<DeviceGet>(b"cuDeviceGet\0").map_err(|_| symbol("cuDeviceGet"))?,
                get_attribute: *library
                    .get::<DeviceGetAttribute>(b"cuDeviceGetAttribute\0")
                    .map_err(|_| symbol("cuDeviceGetAttribute"))?,
                get_name: *library.get::<DeviceGetName>(b"cuDeviceGetName\0").map_err(|_| symbol("cuDeviceGetName"))?,
                get_memory: *library
                    .get::<DeviceTotalMemory>(b"cuDeviceTotalMem_v2\0")
                    .map_err(|_| symbol("cuDeviceTotalMem_v2"))?,
            },
        )
    };
    // The function pointers point into it, and unloading an initialised driver isn't safe.
    std::mem::forget(library);
    let mut version = 0;
    // `cuDriverGetVersion` works before `cuInit`, so an old driver still reports it.
    let status = unsafe { get_version(&mut version) };
    if status != SUCCESS {
        return Err(format!("NVIDIA 드라이버 버전을 읽지 못했습니다 (CUresult {status})"));
    }
    match unsafe { init(0) } {
        SUCCESS => {}
        ERROR_NO_DEVICE => return Ok(CudaDriver { version, gpus: Vec::new() }),
        status => return Err(format!("NVIDIA 드라이버를 초기화하지 못했습니다 (cuInit {status})")),
    }
    let mut count = 0;
    if unsafe { get_count(&mut count) } != SUCCESS {
        count = 0;
    }
    let gpus = (0..count).filter_map(|ordinal| driver.gpu(ordinal)).collect();
    Ok(CudaDriver { version, gpus })
}

impl Driver {
    fn gpu(&self, ordinal: c_int) -> Option<CudaGpu> {
        let mut handle = 0;
        if unsafe { (self.get_device)(&mut handle, ordinal) } != SUCCESS {
            return None;
        }
        let major = self.attribute(handle, ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR)?;
        let minor = self.attribute(handle, ATTRIBUTE_COMPUTE_CAPABILITY_MINOR)?;
        let mut buffer = [0u8; 256];
        let length = c_int::try_from(buffer.len()).expect("short buffer");
        let name = if unsafe { (self.get_name)(buffer.as_mut_ptr().cast(), length, handle) } == SUCCESS {
            CStr::from_bytes_until_nul(&buffer).ok().map(|name| name.to_string_lossy().trim().to_owned())
        } else {
            None
        };
        let mut memory = 0usize;
        if unsafe { (self.get_memory)(&mut memory, handle) } != SUCCESS {
            memory = 0;
        }
        Some(CudaGpu {
            index: usize::try_from(ordinal).ok()?,
            name: name.filter(|name| !name.is_empty()).unwrap_or_else(|| format!("CUDA{ordinal}")),
            compute_capability: major * 10 + minor,
            memory: memory as u64,
            integrated: self.attribute(handle, ATTRIBUTE_INTEGRATED).unwrap_or(0) != 0,
        })
    }

    fn attribute(&self, handle: c_int, attribute: c_int) -> Option<u32> {
        let mut value = 0;
        if unsafe { (self.get_attribute)(&mut value, attribute, handle) } != SUCCESS {
            return None;
        }
        u32::try_from(value).ok()
    }
}

#[cfg(windows)]
unsafe fn open_driver() -> Result<libloading::Library, String> {
    use libloading::os::windows::{LOAD_LIBRARY_SEARCH_SYSTEM32, Library};
    // The driver's own copy in System32, never one next to the app or on PATH.
    unsafe { Library::load_with_flags("nvcuda.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .map(Into::into)
        .map_err(|e| format!("NVIDIA 드라이버(nvcuda.dll)를 찾지 못했습니다: {e}"))
}

#[cfg(target_os = "linux")]
unsafe fn open_driver() -> Result<libloading::Library, String> {
    // The unversioned name may be the toolkit's stub rather than the driver.
    unsafe { libloading::Library::new("libcuda.so.1") }
        .map_err(|e| format!("NVIDIA 드라이버(libcuda.so.1)를 찾지 못했습니다: {e}"))
}

#[cfg(not(any(windows, target_os = "linux")))]
unsafe fn open_driver() -> Result<libloading::Library, String> {
    Err("이 운영체제에는 CUDA가 없습니다".to_owned())
}
