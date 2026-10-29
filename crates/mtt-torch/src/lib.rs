//! LibTorch, loaded at run time from a downloaded runtime: LibTorch's own libraries plus
//! `koharu-torch`, the C shim koharu builds against them (tch-rs's C API). Only the
//! operations LaMa and the text segmentation model need are bound.
//!
//! `load` (or `load_with`, for a build whose libraries go elsewhere, like CUDA's) opens
//! the libraries once per process and they stay loaded. `cuda_driver` asks NVIDIA's driver
//! about the GPUs without LibTorch. All the unsafe code is in `ffi`.

mod ffi;

pub use ffi::{
    CudaDriver, CudaGpu, Device, LoadError, Tensor, cuda_device_count, cuda_driver, has_cuda, is_loaded, load,
    load_with, mps_available,
};

/// The CPU (on macOS the MPS) runtime's libraries for this platform, in the order they're
/// opened: each one's dependencies are already loaded when it opens, and the shim comes
/// last.
pub const LIBRARIES: &[&str] = if cfg!(target_os = "windows") {
    &["libiomp5md.dll", "c10.dll", "torch_global_deps.dll", "torch_cpu.dll", "torch.dll", "koharu-torch.dll"]
} else if cfg!(target_os = "macos") {
    // LibTorch first, so the shim's `@rpath` dependencies resolve to the loaded images.
    &["libtorch.dylib", "libtorch_global_deps.dylib", "libtorch_cpu.dylib", "libc10.dylib", "libkoharu-torch.dylib"]
} else {
    &["libgomp.so.1", "libc10.so", "libtorch_global_deps.so", "libtorch_cpu.so", "libtorch.so", "libkoharu-torch.so"]
};
