//! The shim's C API (`torch_api.h`, `torch_api_generated.h` of koharu-rs/torch) and safe
//! wrappers over it. Every operation returns a new tensor through an `out__` array and
//! reports C++ exceptions through `get_and_reset_last_err`.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

/// A `torch::Tensor *`.
type Raw = *mut c_void;

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        struct Api {
            $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Api {
            /// Safety: `library` must be the shim, whose symbols have these signatures.
            unsafe fn load(library: &libloading::Library) -> Result<Self, String> {
                Ok(Self {
                    $($name: unsafe {
                        *library
                            .get::<unsafe extern "C" fn($($arg),*) $(-> $ret)?>(concat!(stringify!($name), "\0").as_bytes())
                            .map_err(|e| format!("{}: {e}", stringify!($name)))?
                    },)*
                })
            }
        }
    };
}

api! {
    get_and_reset_last_err: fn() -> *mut c_char;
    at_context_has_mps: fn() -> bool;
    at_tensor_of_data: fn(*const c_void, *const i64, usize, usize, c_int) -> Raw;
    at_copy_data: fn(Raw, *mut c_void, usize, usize);
    at_shallow_clone: fn(Raw) -> Raw;
    at_dim: fn(Raw) -> usize;
    at_shape: fn(Raw, *mut i64);
    at_free: fn(Raw);
    atg_to: fn(*mut Raw, Raw, c_int);
    atg_conv2d: fn(*mut Raw, Raw, Raw, Raw, *const i64, c_int, *const i64, c_int, *const i64, c_int, i64);
    atg_conv_transpose2d:
        fn(*mut Raw, Raw, Raw, Raw, *const i64, c_int, *const i64, c_int, *const i64, c_int, i64, *const i64, c_int);
    atg_reflection_pad2d: fn(*mut Raw, Raw, *const i64, c_int);
    atg_relu: fn(*mut Raw, Raw);
    atg_sigmoid: fn(*mut Raw, Raw);
    atg_silu: fn(*mut Raw, Raw);
    atg_add: fn(*mut Raw, Raw, Raw);
    atg_mul: fn(*mut Raw, Raw, Raw);
    atg_mean_dim: fn(*mut Raw, Raw, *const i64, c_int, c_int, c_int);
    atg_group_norm: fn(*mut Raw, Raw, i64, Raw, Raw, f64, c_int);
    atg_upsample_nearest2d: fn(*mut Raw, Raw, *const i64, c_int, f64, i8, f64, i8);
    atg_cat: fn(*mut Raw, *const Raw, c_int, i64);
    atg_narrow: fn(*mut Raw, Raw, i64, i64, i64);
    atg_reshape: fn(*mut Raw, Raw, *const i64, c_int);
    atg_permute: fn(*mut Raw, Raw, *const i64, c_int);
    atg_contiguous: fn(*mut Raw, Raw);
    atg_view_as_real: fn(*mut Raw, Raw);
    atg_view_as_complex: fn(*mut Raw, Raw);
    atg_fft_rfftn: fn(*mut Raw, Raw, *const i64, c_int, *const i64, c_int, *const c_char, c_int);
    atg_fft_irfftn: fn(*mut Raw, Raw, *const i64, c_int, *const i64, c_int, *const c_char, c_int);
}

/// `c10::ScalarType::Float`.
const FLOAT: c_int = 6;

struct Runtime {
    api: Api,
    /// Kept open for the process (the API points into them).
    _libraries: Vec<libloading::Library>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Opens the runtime in `directory` (`crate::LIBRARIES`). Later calls return at once,
/// whatever directory they name.
pub fn load(directory: &Path) -> Result<(), String> {
    static LOADING: Mutex<()> = Mutex::new(());
    let _guard = LOADING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if RUNTIME.get().is_some() {
        return Ok(());
    }
    let mut libraries = Vec::new();
    for name in crate::LIBRARIES {
        let path = directory.join(name);
        let library = unsafe { open(&path) }.map_err(|e| format!("{}: {e}", path.display()))?;
        libraries.push(library);
    }
    let shim = libraries.last().expect("the shim is listed");
    let api = unsafe { Api::load(shim) }?;
    let _ = RUNTIME.set(Runtime { api, _libraries: libraries });
    Ok(())
}

#[cfg(windows)]
unsafe fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    use libloading::os::windows::{LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library};
    // Dependencies from the library's own folder, not the app's or PATH.
    unsafe { Library::load_with_flags(path, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .map(Into::into)
}

#[cfg(not(windows))]
unsafe fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    // `RTLD_LOCAL`: LibTorch's symbols stay out of the global namespace.
    unsafe { libloading::Library::new(path) }
}

pub fn is_loaded() -> bool {
    RUNTIME.get().is_some()
}

fn api() -> Result<&'static Api, String> {
    RUNTIME.get().map(|runtime| &runtime.api).ok_or_else(|| "LibTorch가 로드되지 않았습니다".to_owned())
}

/// The pending C++ exception's message, if the last call threw.
fn check(api: &Api) -> Result<(), String> {
    let error = unsafe { (api.get_and_reset_last_err)() };
    if error.is_null() {
        return Ok(());
    }
    let message = unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned();
    unsafe { libc::free(error.cast()) };
    // Drop the C++ backtrace that follows the message.
    Err(message.split("\nException raised from").next().unwrap_or_default().trim().to_owned())
}

/// Whether this LibTorch build has MPS and the Mac can run it.
pub fn mps_available() -> bool {
    api().is_ok_and(|api| {
        let available = unsafe { (api.at_context_has_mps)() };
        check(api).is_ok() && available
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Cpu,
    /// Apple's Metal Performance Shaders.
    Mps,
}

impl Device {
    /// The shim's `device_of_int` encoding.
    fn code(self) -> c_int {
        match self {
            Self::Cpu => -1,
            Self::Mps => -2,
        }
    }
}

/// An owned `torch::Tensor` (a reference-counted handle; views share storage).
pub struct Tensor(NonNull<c_void>);

// torch::Tensor handles may move between threads, and the operations bound here only read
// their inputs.
unsafe impl Send for Tensor {}
unsafe impl Sync for Tensor {}

impl Drop for Tensor {
    fn drop(&mut self) {
        // A tensor only exists once the runtime is loaded.
        if let Ok(api) = api() {
            unsafe { (api.at_free)(self.raw()) };
        }
    }
}

/// Runs an operation writing one tensor to `out__`.
fn op(call: impl FnOnce(&Api, *mut Raw)) -> Result<Tensor, String> {
    let api = api()?;
    let mut out: [Raw; 1] = [std::ptr::null_mut()];
    call(api, out.as_mut_ptr());
    check(api)?;
    NonNull::new(out[0]).map(Tensor).ok_or_else(|| "LibTorch가 텐서를 돌려주지 않았습니다".to_owned())
}

fn length(values: &[i64]) -> c_int {
    c_int::try_from(values.len()).expect("short dimension list")
}

impl Tensor {
    fn raw(&self) -> Raw {
        self.0.as_ptr()
    }

    /// A float32 CPU tensor holding `data` in row-major `shape`.
    pub fn from_f32(data: &[f32], shape: &[i64]) -> Result<Self, String> {
        let elements: i64 = shape.iter().product();
        if usize::try_from(elements).ok() != Some(data.len()) {
            return Err(format!("{} values for shape {shape:?}", data.len()));
        }
        let api = api()?;
        let raw = unsafe {
            (api.at_tensor_of_data)(data.as_ptr().cast(), shape.as_ptr(), shape.len(), size_of::<f32>(), FLOAT)
        };
        check(api)?;
        NonNull::new(raw).map(Self).ok_or_else(|| "LibTorch가 텐서를 만들지 못했습니다".to_owned())
    }

    pub fn shape(&self) -> Result<Vec<i64>, String> {
        let api = api()?;
        let dims = unsafe { (api.at_dim)(self.raw()) };
        check(api)?;
        let mut shape = vec![0i64; dims];
        unsafe { (api.at_shape)(self.raw(), shape.as_mut_ptr()) };
        check(api)?;
        Ok(shape)
    }

    /// The values in row-major order, copied to the CPU (waiting for the device). The
    /// tensor must be float32.
    pub fn to_f32_vec(&self) -> Result<Vec<f32>, String> {
        let elements: i64 = self.shape()?.iter().product();
        let mut data = vec![0f32; usize::try_from(elements).map_err(|e| e.to_string())?];
        let api = api()?;
        unsafe { (api.at_copy_data)(self.raw(), data.as_mut_ptr().cast(), data.len(), size_of::<f32>()) };
        check(api)?;
        Ok(data)
    }

    /// Another handle to the same tensor (no copy).
    pub fn shallow_clone(&self) -> Result<Self, String> {
        let api = api()?;
        let raw = unsafe { (api.at_shallow_clone)(self.raw()) };
        check(api)?;
        NonNull::new(raw).map(Self).ok_or_else(|| "LibTorch가 텐서를 돌려주지 않았습니다".to_owned())
    }

    pub fn to(&self, device: Device) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_to)(out, self.raw(), device.code()) })
    }

    /// `conv2d` with zero `padding` on every side and no dilation or groups.
    pub fn conv2d(&self, weight: &Self, bias: Option<&Self>, stride: i64, padding: i64) -> Result<Self, String> {
        self.conv2d_grouped(weight, bias, stride, padding, 1)
    }

    /// `conv2d` in `groups` groups (`groups` = channels: depthwise), with zero `padding` on
    /// every side and no dilation.
    pub fn conv2d_grouped(
        &self,
        weight: &Self,
        bias: Option<&Self>,
        stride: i64,
        padding: i64,
        groups: i64,
    ) -> Result<Self, String> {
        let (stride, padding, dilation) = ([stride; 2], [padding; 2], [1i64; 2]);
        let bias = bias.map_or(std::ptr::null_mut(), Self::raw);
        op(|api, out| unsafe {
            (api.atg_conv2d)(
                out,
                self.raw(),
                weight.raw(),
                bias,
                stride.as_ptr(),
                2,
                padding.as_ptr(),
                2,
                dilation.as_ptr(),
                2,
                groups,
            )
        })
    }

    /// `conv_transpose2d`; `weight` is `(in, out, kh, kw)`.
    pub fn conv_transpose2d(
        &self,
        weight: &Self,
        bias: Option<&Self>,
        stride: i64,
        padding: i64,
        output_padding: i64,
    ) -> Result<Self, String> {
        let (stride, padding, output_padding, dilation) = ([stride; 2], [padding; 2], [output_padding; 2], [1i64; 2]);
        let bias = bias.map_or(std::ptr::null_mut(), Self::raw);
        op(|api, out| unsafe {
            (api.atg_conv_transpose2d)(
                out,
                self.raw(),
                weight.raw(),
                bias,
                stride.as_ptr(),
                2,
                padding.as_ptr(),
                2,
                output_padding.as_ptr(),
                2,
                1,
                dilation.as_ptr(),
                2,
            )
        })
    }

    /// Reflection padding by `pad` on all four sides of the last two dimensions.
    pub fn reflection_pad2d(&self, pad: i64) -> Result<Self, String> {
        let padding = [pad; 4];
        op(|api, out| unsafe { (api.atg_reflection_pad2d)(out, self.raw(), padding.as_ptr(), 4) })
    }

    pub fn relu(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_relu)(out, self.raw()) })
    }

    pub fn sigmoid(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_sigmoid)(out, self.raw()) })
    }

    /// `x * sigmoid(x)`.
    pub fn silu(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_silu)(out, self.raw()) })
    }

    /// Element-wise sum, broadcasting.
    pub fn add(&self, other: &Self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_add)(out, self.raw(), other.raw()) })
    }

    /// Element-wise product, broadcasting.
    pub fn mul(&self, other: &Self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_mul)(out, self.raw(), other.raw()) })
    }

    /// The mean over `dims`, which stay as size 1 (`keepdim=True`).
    pub fn mean_keepdim(&self, dims: &[i64]) -> Result<Self, String> {
        // dtype -1: the input's.
        op(|api, out| unsafe { (api.atg_mean_dim)(out, self.raw(), dims.as_ptr(), length(dims), 1, -1) })
    }

    /// `group_norm` of `(N, C, …)` in `groups` groups with a per-channel affine `weight` and
    /// `bias`.
    pub fn group_norm(&self, groups: i64, weight: &Self, bias: &Self, eps: f64) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_group_norm)(out, self.raw(), groups, weight.raw(), bias.raw(), eps, 0) })
    }

    /// Nearest-neighbour resize of the last two dimensions to `height`×`width`
    /// (`F.interpolate(mode="nearest")`).
    pub fn upsample_nearest2d(&self, height: i64, width: i64) -> Result<Self, String> {
        let size = [height, width];
        // No scale factors (NaN, flagged absent): the size decides.
        op(|api, out| unsafe {
            (api.atg_upsample_nearest2d)(out, self.raw(), size.as_ptr(), 2, f64::NAN, 1, f64::NAN, 1)
        })
    }

    pub fn cat(tensors: &[&Self], dim: i64) -> Result<Self, String> {
        let raw: Vec<Raw> = tensors.iter().map(|tensor| tensor.raw()).collect();
        let count = c_int::try_from(raw.len()).map_err(|e| e.to_string())?;
        op(|api, out| unsafe { (api.atg_cat)(out, raw.as_ptr(), count, dim) })
    }

    /// A view of `length` entries along `dim` from `start`.
    pub fn narrow(&self, dim: i64, start: i64, length: i64) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_narrow)(out, self.raw(), dim, start, length) })
    }

    pub fn reshape(&self, shape: &[i64]) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_reshape)(out, self.raw(), shape.as_ptr(), length(shape)) })
    }

    pub fn permute(&self, dims: &[i64]) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_permute)(out, self.raw(), dims.as_ptr(), length(dims)) })
    }

    pub fn contiguous(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_contiguous)(out, self.raw()) })
    }

    /// A complex tensor as a real one with a trailing (re, im) dimension.
    pub fn view_as_real(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_view_as_real)(out, self.raw()) })
    }

    /// The inverse of `view_as_real`; the last dimension must be 2 and contiguous.
    pub fn view_as_complex(&self) -> Result<Self, String> {
        op(|api, out| unsafe { (api.atg_view_as_complex)(out, self.raw()) })
    }

    /// `torch.fft.rfftn(x, dim=(-2, -1), norm="ortho")`.
    pub fn rfft2(&self) -> Result<Self, String> {
        let dims = [-2i64, -1];
        op(|api, out| unsafe {
            (api.atg_fft_rfftn)(out, self.raw(), std::ptr::null(), 0, dims.as_ptr(), 2, ORTHO.as_ptr().cast(), 5)
        })
    }

    /// `torch.fft.irfftn(x, s=(height, width), dim=(-2, -1), norm="ortho")`.
    pub fn irfft2(&self, height: i64, width: i64) -> Result<Self, String> {
        let (size, dims) = ([height, width], [-2i64, -1]);
        op(|api, out| unsafe {
            (api.atg_fft_irfftn)(out, self.raw(), size.as_ptr(), 2, dims.as_ptr(), 2, ORTHO.as_ptr().cast(), 5)
        })
    }
}

const ORTHO: &[u8; 5] = b"ortho";

impl std::fmt::Debug for Tensor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Tensor").field(&self.shape()).finish()
    }
}
