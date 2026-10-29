//! LibTorch, loaded at run time from a downloaded runtime: LibTorch's own libraries plus
//! `koharu-torch`, the C shim koharu builds against them (tch-rs's C API). Only the
//! operations LaMa needs are bound.
//!
//! `load` opens the libraries once per process and they stay loaded. All the unsafe code
//! is in `ffi`.

mod ffi;

pub use ffi::{Device, Tensor, is_loaded, load, mps_available};

/// The runtime's libraries for this platform, in the order they're opened: each one's
/// dependencies are already loaded when it opens, and the shim comes last.
pub const LIBRARIES: &[&str] = if cfg!(target_os = "windows") {
    &["libiomp5md.dll", "c10.dll", "torch_global_deps.dll", "torch_cpu.dll", "torch.dll", "koharu-torch.dll"]
} else if cfg!(target_os = "macos") {
    // LibTorch first, so the shim's `@rpath` dependencies resolve to the loaded images.
    &["libtorch.dylib", "libtorch_global_deps.dylib", "libtorch_cpu.dylib", "libc10.dylib", "libkoharu-torch.dylib"]
} else {
    &["libgomp.so.1", "libc10.so", "libtorch_global_deps.so", "libtorch_cpu.so", "libtorch.so", "libkoharu-torch.so"]
};
