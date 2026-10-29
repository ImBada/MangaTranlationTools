//! The LibTorch runtime LaMa and the text segmentation model run on: koharu's LibTorch
//! builds with their C shim (<https://github.com/koharu-rs/torch>), downloaded once into
//! `<data>/tools/libtorch/<release>-<asset>/`. macOS gets the MPS build. Windows and Linux
//! get the CPU one or, with an NVIDIA GPU whose driver runs CUDA 13.3, the CUDA build plus
//! NVIDIA's runtime libraries from their PyPI wheels (`tools/libtorch/nvidia/<wheel>/`):
//! the packages, libraries and load order of koharu's `koharu-runtime`
//! (`runtime/packages/{torch,cuda}.rs`, <https://github.com/koharu-rs/koharu>, MIT OR
//! Apache-2.0). Every archive is pinned by size and SHA-256.
//!
//! One build loads per process ([`activate`]): CUDA when it's installed and wanted and the
//! driver reports a GPU it runs on, otherwise the CPU build. When CUDA fails the models run
//! on the CPU, and [`RuntimeStatus::note`] says why.

use std::cmp::Reverse;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use mtt_core::paths::AppPaths;
use mtt_torch::{CudaDriver, CudaGpu, Device};
use sha2::{Digest, Sha256};

/// The koharu-rs/torch release; its shim must match `mtt-torch`'s bindings.
pub const RELEASE: &str = "v2.13.0.7";

/// The CUDA build's libraries are CUDA 13.3's, which need a driver that runs 13.3
/// (`cuDriverGetVersion` ≥ 13030), as koharu requires.
const MIN_DRIVER_VERSION: i32 = 13030;
/// Its kernels are for compute capability 7.5 (Turing) and newer.
const MIN_COMPUTE_CAPABILITY: u32 = 75;

/// In the runtime folder: `cpu` when the user chose the CPU.
const PREFERENCE_FILE: &str = "device.txt";
/// In the CUDA build's folder: why it failed to load, after a failure that left part of
/// it loaded (the CPU build can't load in that process). The next run uses the CPU.
const FAILED_FILE: &str = "load-failed.txt";

/// A platform LibTorch builds are published for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacArm,
    WindowsX64,
    LinuxX64,
}

impl Platform {
    pub const fn host() -> Option<Self> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::MacArm)
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::WindowsX64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::LinuxX64)
        } else {
            None
        }
    }

    /// Whether a CUDA build is published for it.
    pub fn has_cuda(self) -> bool {
        self != Self::MacArm
    }
}

/// A LibTorch build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// The CPU build; on Apple Silicon the MPS one.
    Cpu,
    /// The CUDA 13.3 build with NVIDIA's libraries (Windows and Linux x64).
    Cuda,
}

/// The build the user wants, chosen in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preference {
    /// CUDA when the GPU and its driver can run it, otherwise the CPU.
    #[default]
    Gpu,
    Cpu,
}

/// A koharu-rs/torch archive.
struct Build {
    asset: &'static str,
    size: u64,
    sha256: &'static str,
}

const MAC_METAL: Build = Build {
    asset: "aarch64-apple-darwin-metal",
    size: 75_866_269,
    sha256: "cc13ec11bd014eb334595e6255ec450f8a3bbc6d8c90ff87fb04769066163754",
};
const WINDOWS_CPU: Build = Build {
    asset: "x86_64-pc-windows-msvc-cpu",
    size: 75_078_065,
    sha256: "db6917ea59ef4a3d5eff1d4f4232c364768296f3c98bad50713b93e3594da135",
};
const LINUX_CPU: Build = Build {
    asset: "x86_64-unknown-linux-gnu-cpu",
    size: 118_493_021,
    sha256: "467c182f60b15c8aae0db4e56e0b23652e6a0ef9d0daed3e10d7e96434ca5f20",
};
const WINDOWS_CUDA: Build = Build {
    asset: "x86_64-pc-windows-msvc-cuda",
    size: 819_352_047,
    sha256: "6ac600e10eac53e45b9a8ca4c3d04771a0423c0b2e70adf422df46ff07766f8d",
};
const LINUX_CUDA: Build = Build {
    asset: "x86_64-unknown-linux-gnu-cuda",
    size: 1_069_890_727,
    sha256: "12498b1fe8cc5e7132911211f4c68b94be7a34ebfab4da8b5dc39f0b07b65134",
};

/// The CUDA builds' libraries in load order, the shim last (koharu's `Torch::Cuda`; the
/// release packs `c10*`, `torch*`, `caffe2_nvrtc` and the shim, without OpenMP).
const WINDOWS_CUDA_LIBRARIES: &[&str] = &[
    "c10.dll",
    "c10_cuda.dll",
    "caffe2_nvrtc.dll",
    "torch_global_deps.dll",
    "torch_cpu.dll",
    "torch_cuda.dll",
    "torch.dll",
    "koharu-torch.dll",
];
const LINUX_CUDA_LIBRARIES: &[&str] = &[
    "libc10.so",
    "libc10_cuda.so",
    "libcaffe2_nvrtc.so",
    "libtorch_global_deps.so",
    "libtorch_cpu.so",
    "libtorch_cuda.so",
    "libtorch.so",
    "libkoharu-torch.so",
];

/// One of NVIDIA's wheels for the CUDA build (koharu's `Cuda` packages, the files its
/// `pypi.rs` picks), with the libraries it provides in load order.
struct Wheel {
    /// `<project>-<version>`, also its folder.
    name: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
    libraries: &'static [&'static str],
}

/// Both lists in koharu's order: each wheel's libraries need only those before it.
const WINDOWS_WHEELS: &[Wheel] = &[
    Wheel {
        name: "nvidia-cuda-runtime-13.3.29",
        url: "https://files.pythonhosted.org/packages/d2/27/b53a5e0397842a5c11f0e1a39d4e5b2f22638a4126e83b3c4e196f62c969/nvidia_cuda_runtime-13.3.29-py3-none-win_amd64.whl",
        size: 2_630_354,
        sha256: "0667ec61c3d897388efa305ed4f7609ace88849a753ba9c6311d06dca55fff4f",
        libraries: &["cudart64_13.dll"],
    },
    Wheel {
        name: "nvidia-nvjitlink-13.3.33",
        url: "https://files.pythonhosted.org/packages/67/f2/ec9c05a108095828dfc58840978c627b3c313fdf2a567c6de9ffbbb46901/nvidia_nvjitlink-13.3.33-py3-none-win_amd64.whl",
        size: 37_766_359,
        sha256: "4297ee49639b4f2e07255a1d69b3acc7ab2d011bb892b403e91ac98368962e3b",
        libraries: &["nvJitLink_130_0.dll"],
    },
    Wheel {
        name: "nvidia-cuda-nvrtc-13.3.33",
        url: "https://files.pythonhosted.org/packages/a1/42/edce72f2c5a0f587168109c867f25f4a9a6cd7289ecf0d68ed2b1070f273/nvidia_cuda_nvrtc-13.3.33-py3-none-win_amd64.whl",
        size: 45_319_163,
        sha256: "7d2af818851c0c224d5f92221e9226e51ee23c236df4b51f9194563979c888be",
        libraries: &["nvrtc-builtins64_133.dll", "nvrtc64_130_0.dll"],
    },
    Wheel {
        name: "nvidia-cublas-13.6.0.2",
        url: "https://files.pythonhosted.org/packages/08/8f/890a96ea1ff615100296977cce23296052dcb8c114d4e451201ec39df9bf/nvidia_cublas-13.6.0.2-py3-none-win_amd64.whl",
        size: 394_568_225,
        sha256: "3b5bcd6bfb6f65010ebf195851bcb9b2aa34b9fe08479432002991c1fe84b67d",
        libraries: &["cublasLt64_13.dll", "cublas64_13.dll"],
    },
    Wheel {
        name: "nvidia-cufft-12.3.0.29",
        url: "https://files.pythonhosted.org/packages/94/64/8e9d808720559d3cbfcd1d1bc8a2e6f55deb29d692513d5a93c8d417b7e5/nvidia_cufft-12.3.0.29-py3-none-win_amd64.whl",
        size: 183_939_745,
        sha256: "510036a2bbab5c83ae93dc5c907c3a49d3518e3066ac3a2052ff0f7f9b27dfc4",
        libraries: &["cufft64_12.dll"],
    },
    Wheel {
        name: "nvidia-curand-10.4.3.29",
        url: "https://files.pythonhosted.org/packages/2a/eb/63f7710fc84837e0118002bc29671542807921aef3a0c710da83a5e7e711/nvidia_curand-10.4.3.29-py3-none-win_amd64.whl",
        size: 55_429_221,
        sha256: "34b18d5a2a8e5db4c3846475ae4eef0cacdf3ac5e9c501f3a4efb422f137a74e",
        libraries: &["curand64_10.dll"],
    },
    Wheel {
        name: "nvidia-cusparse-12.8.2.51",
        url: "https://files.pythonhosted.org/packages/91/3b/2193ab417f147d8a9fb43ea00b447ecb3fa352ba9d423fc55c8f0649a96b/nvidia_cusparse-12.8.2.51-py3-none-win_amd64.whl",
        size: 152_492_210,
        sha256: "2ee59291cd362038f3d40d57c7cd09b26d689f3873ae5c94b31c3270772d41b8",
        libraries: &["cusparse64_12.dll"],
    },
    Wheel {
        name: "nvidia-cusolver-12.2.6.9",
        url: "https://files.pythonhosted.org/packages/56/ed/520ef455cfacd3d44847b11c9858287af70d13ea4c38a64cfe16eb040139/nvidia_cusolver-12.2.6.9-py3-none-win_amd64.whl",
        size: 236_912_336,
        sha256: "1dbb828e0fb27701c084a8fb80f770c562cb9a6d29b6f001320e6f542c6d024f",
        libraries: &["cusolver64_12.dll", "cusolverMg64_12.dll"],
    },
    Wheel {
        name: "nvidia-cudnn-cu13-9.25.1.1",
        url: "https://files.pythonhosted.org/packages/fd/0f/d7e4141c1126899c7b8d202eb3085380164beefef32f94cc8967ed3a00ff/nvidia_cudnn_cu13-9.25.1.1-py3-none-win_amd64.whl",
        size: 407_762_053,
        sha256: "e1de75bf1ad9040414f9b13cc87135d660d13dd3c859180dd6b63353e571f860",
        libraries: &[
            "cudnn64_9.dll",
            "cudnn_adv64_9.dll",
            "cudnn_cnn64_9.dll",
            "cudnn_engines_precompiled64_9.dll",
            "cudnn_engines_runtime_compiled64_9.dll",
            "cudnn_engines_tensor_ir64_9.dll",
            "cudnn_ext64_9.dll",
            "cudnn_graph64_9.dll",
            "cudnn_heuristic64_9.dll",
            "cudnn_ops64_9.dll",
        ],
    },
];
const LINUX_WHEELS: &[Wheel] = &[
    Wheel {
        name: "nvidia-cuda-runtime-13.3.29",
        url: "https://files.pythonhosted.org/packages/97/be/5699b6e642b372f7d24c59c2f41383e2696825e20bab85f7399c7c6a56f7/nvidia_cuda_runtime-13.3.29-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
        size: 2_339_786,
        sha256: "e04420616e72f563167a7733272992d7e6df6dc5cb54b2f94f9f1520ea9e30c1",
        libraries: &["libcudart.so.13"],
    },
    Wheel {
        name: "nvidia-nvjitlink-13.3.33",
        url: "https://files.pythonhosted.org/packages/f0/ee/580ca6f29dcab0221db8706badca1bbbb084f1975c4d4e83329c3a7e31f0/nvidia_nvjitlink-13.3.33-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl",
        size: 40_742_423,
        sha256: "26a6de7fb4c8fdaa7703d3dad720d6d427ddfea5c48a528fd97c11733ad830e5",
        libraries: &["libnvJitLink.so.13"],
    },
    Wheel {
        name: "nvidia-cuda-nvrtc-13.3.33",
        url: "https://files.pythonhosted.org/packages/8b/2c/86916c8a34dcdb0c3ddd1c0e30545041bd781184e437b9cb76fcda70560b/nvidia_cuda_nvrtc-13.3.33-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl",
        size: 51_110_910,
        sha256: "82530788b8c6164a54d3fd9ae8bcca8893d397c4aeb998861982a03bbe41e204",
        libraries: &["libnvrtc-builtins.so.13.3", "libnvrtc.so.13"],
    },
    Wheel {
        name: "nvidia-cublas-13.6.0.2",
        url: "https://files.pythonhosted.org/packages/fd/6c/173c7a3db77a6592210f73f194f0f8ed5e51b6ec61cfed7b1eee06ac5fd3/nvidia_cublas-13.6.0.2-py3-none-manylinux_2_27_x86_64.whl",
        size: 410_473_152,
        sha256: "b82c80c886cea6da6e149a5c3bdba274f12b7e4ec4b00a050b916b0446fb4153",
        libraries: &["libcublasLt.so.13", "libcublas.so.13"],
    },
    Wheel {
        name: "nvidia-cufft-12.3.0.29",
        url: "https://files.pythonhosted.org/packages/e7/00/fab4a29fa1d7eb43bc6b94de4e86312c5e425d5582e58b9641300b9dffc7/nvidia_cufft-12.3.0.29-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
        size: 184_730_406,
        sha256: "edb25c0626bd202ee5acc035b5dd361a3b89ed3b75a81a52df72c89150cb57c2",
        libraries: &["libcufft.so.12"],
    },
    Wheel {
        name: "nvidia-curand-10.4.3.29",
        url: "https://files.pythonhosted.org/packages/ee/49/4ca4ce4a9334c9a1ef68ab85b358f019bf85e8c3f51c2471d4cc257a273d/nvidia_curand-10.4.3.29-py3-none-manylinux_2_27_x86_64.whl",
        size: 60_023_614,
        sha256: "1859bf37a62754d2c65001393096ca79de399f995971fa7826d0adfd88c3cf7b",
        libraries: &["libcurand.so.10"],
    },
    Wheel {
        name: "nvidia-cusparse-12.8.2.51",
        url: "https://files.pythonhosted.org/packages/a6/dc/752e401a5d6fc5f1948cf190add5afc27e440e5ca08bc3fab66826c16213/nvidia_cusparse-12.8.2.51-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
        size: 154_538_196,
        sha256: "65cbcc4e37a34fca4ee7df2fd57da103593842cda1bbb4a144664ecfe59873a5",
        libraries: &["libcusparse.so.12"],
    },
    Wheel {
        name: "nvidia-cusolver-12.2.6.9",
        url: "https://files.pythonhosted.org/packages/1c/3a/23e46a0919ba5ecf1a864400542a6c78d18c76893a0a77e1856e7033af31/nvidia_cusolver-12.2.6.9-py3-none-manylinux_2_27_x86_64.whl",
        size: 244_667_799,
        sha256: "e32cb40c8a4d3df475b556caaf6af5808ef064df7e291a049528a0d28017b674",
        libraries: &["libcusolver.so.12", "libcusolverMg.so.12"],
    },
    Wheel {
        name: "nvidia-cudnn-cu13-9.25.1.1",
        url: "https://files.pythonhosted.org/packages/f9/df/15839f2ac0074db8a3b77dccfa91fad2e90c480ad5143c25a7733c50870e/nvidia_cudnn_cu13-9.25.1.1-py3-none-manylinux_2_27_x86_64.whl",
        size: 503_870_572,
        sha256: "195199e8c7f8bca0c366ea675dd2441a406646cf95a98bcda66c999d57dd1f26",
        libraries: &[
            "libcudnn.so.9",
            "libcudnn_adv.so.9",
            "libcudnn_cnn.so.9",
            "libcudnn_engines_precompiled.so.9",
            "libcudnn_engines_runtime_compiled.so.9",
            "libcudnn_engines_tensor_ir.so.9",
            "libcudnn_ext.so.9",
            "libcudnn_graph.so.9",
            "libcudnn_heuristic.so.9",
            "libcudnn_ops.so.9",
        ],
    },
];

fn build(platform: Platform, flavor: Flavor) -> Option<&'static Build> {
    match (platform, flavor) {
        (Platform::MacArm, Flavor::Cpu) => Some(&MAC_METAL),
        (Platform::WindowsX64, Flavor::Cpu) => Some(&WINDOWS_CPU),
        (Platform::LinuxX64, Flavor::Cpu) => Some(&LINUX_CPU),
        (Platform::WindowsX64, Flavor::Cuda) => Some(&WINDOWS_CUDA),
        (Platform::LinuxX64, Flavor::Cuda) => Some(&LINUX_CUDA),
        (Platform::MacArm, Flavor::Cuda) => None,
    }
}

/// The build's own libraries in load order. The CPU builds' are `mtt-torch`'s, which are
/// this platform's.
fn torch_libraries(platform: Platform, flavor: Flavor) -> &'static [&'static str] {
    match (platform, flavor) {
        (_, Flavor::Cpu) => mtt_torch::LIBRARIES,
        (Platform::WindowsX64, Flavor::Cuda) => WINDOWS_CUDA_LIBRARIES,
        (Platform::LinuxX64, Flavor::Cuda) => LINUX_CUDA_LIBRARIES,
        (Platform::MacArm, Flavor::Cuda) => &[],
    }
}

fn wheels(platform: Platform, flavor: Flavor) -> &'static [Wheel] {
    match (platform, flavor) {
        (Platform::WindowsX64, Flavor::Cuda) => WINDOWS_WHEELS,
        (Platform::LinuxX64, Flavor::Cuda) => LINUX_WHEELS,
        _ => &[],
    }
}

/// The runtime folder, `<data>/tools/libtorch`.
pub fn libtorch_dir(paths: &AppPaths) -> PathBuf {
    paths.tools_dir.join("libtorch")
}

/// An archive to fetch and the folder its libraries go to (flat, by file name).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Part {
    url: String,
    size: u64,
    sha256: &'static str,
    dir: PathBuf,
    libraries: &'static [&'static str],
    /// A wheel (zip) rather than a koharu-rs/torch tarball.
    wheel: bool,
}

/// What a build is made of in `root`: its LibTorch archive, then for CUDA NVIDIA's wheels
/// in load order. Empty where the build isn't published.
fn parts(root: &Path, platform: Platform, flavor: Flavor) -> Vec<Part> {
    let Some(build) = build(platform, flavor) else { return Vec::new() };
    let torch = Part {
        url: format!("https://github.com/koharu-rs/torch/releases/download/{RELEASE}/{}.tar.gz", build.asset),
        size: build.size,
        sha256: build.sha256,
        dir: root.join(format!("{RELEASE}-{}", build.asset)),
        libraries: torch_libraries(platform, flavor),
        wheel: false,
    };
    let wheels = wheels(platform, flavor).iter().map(|wheel| Part {
        url: wheel.url.to_owned(),
        size: wheel.size,
        sha256: wheel.sha256,
        dir: root.join("nvidia").join(wheel.name),
        libraries: wheel.libraries,
        wheel: true,
    });
    std::iter::once(torch).chain(wheels).collect()
}

impl Part {
    fn installed(&self) -> bool {
        complete(&self.dir, self.libraries)
    }

    /// The libraries in load order.
    fn library_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.libraries.iter().map(|name| self.dir.join(name))
    }
}

fn complete(dir: &Path, libraries: &[&str]) -> bool {
    !libraries.is_empty() && libraries.iter().all(|name| dir.join(name).is_file())
}

/// The CUDA build when all of it is there.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CudaInstall {
    /// The LibTorch build's folder.
    torch: PathBuf,
    /// NVIDIA's libraries, in load order.
    dependencies: Vec<PathBuf>,
    /// Why it failed to load in an earlier run (`FAILED_FILE`).
    failed: Option<String>,
}

/// This platform's installed builds (file checks only).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Installed {
    cpu: Option<PathBuf>,
    cuda: Option<CudaInstall>,
}

impl Installed {
    fn find(root: &Path, platform: Platform) -> Self {
        let cpu = parts(root, platform, Flavor::Cpu).into_iter().next().filter(Part::installed).map(|part| part.dir);
        let cuda = parts(root, platform, Flavor::Cuda);
        let cuda = (!cuda.is_empty() && cuda.iter().all(Part::installed)).then(|| CudaInstall {
            torch: cuda[0].dir.clone(),
            dependencies: cuda[1..].iter().flat_map(Part::library_paths).collect(),
            failed: std::fs::read_to_string(cuda[0].dir.join(FAILED_FILE)).ok().map(|reason| reason.trim().to_owned()),
        });
        Self { cpu, cuda }
    }
}

/// The preference in `root` (the GPU unless the user chose the CPU).
fn read_preference(root: &Path) -> Preference {
    match std::fs::read_to_string(root.join(PREFERENCE_FILE)) {
        Ok(text) if text.trim() == "cpu" => Preference::Cpu,
        _ => Preference::Gpu,
    }
}

/// Saves the user's choice. Choosing the GPU again retries a CUDA build that failed to
/// load. A running process keeps the build it loaded.
pub fn set_preference(paths: &AppPaths, preference: Preference) -> Result<(), String> {
    save_preference(&libtorch_dir(paths), Platform::host(), preference)
}

fn save_preference(root: &Path, platform: Option<Platform>, preference: Preference) -> Result<(), String> {
    let file = root.join(PREFERENCE_FILE);
    match preference {
        Preference::Cpu => {
            std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
            std::fs::write(&file, "cpu\n").map_err(|e| e.to_string())
        }
        Preference::Gpu => {
            if let Some(cuda) = platform.and_then(|platform| parts(root, platform, Flavor::Cuda).into_iter().next()) {
                let _ = std::fs::remove_file(cuda.dir.join(FAILED_FILE));
            }
            match std::fs::remove_file(&file) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
                _ => Ok(()),
            }
        }
    }
}

/// The GPU the CUDA build runs on: the driver must run CUDA 13.3 and the GPU be compute
/// capability 7.5 or newer; discrete GPUs first, then the most memory (koharu's order).
fn usable_gpu(driver: &Result<CudaDriver, String>) -> Result<CudaGpu, String> {
    let driver = driver.as_ref().map_err(Clone::clone)?;
    if driver.version < MIN_DRIVER_VERSION {
        return Err(format!(
            "NVIDIA 드라이버가 CUDA {}까지만 지원합니다 (CUDA {} 이상 필요). 드라이버를 업데이트해 주세요.",
            cuda_version(driver.version),
            cuda_version(MIN_DRIVER_VERSION)
        ));
    }
    let best = driver
        .gpus
        .iter()
        .filter(|gpu| gpu.compute_capability >= MIN_COMPUTE_CAPABILITY)
        .min_by_key(|gpu| (gpu.integrated, Reverse(gpu.memory), Reverse(gpu.compute_capability), gpu.index));
    match (best, driver.gpus.first()) {
        (Some(gpu), _) => Ok(gpu.clone()),
        (None, None) => Err("NVIDIA GPU를 찾지 못했습니다".to_owned()),
        (None, Some(gpu)) => Err(format!(
            "{}의 연산 능력 {}.{}은 지원하지 않습니다 ({}.{} 이상 필요)",
            gpu.name,
            gpu.compute_capability / 10,
            gpu.compute_capability % 10,
            MIN_COMPUTE_CAPABILITY / 10,
            MIN_COMPUTE_CAPABILITY % 10
        )),
    }
}

/// `cuDriverGetVersion`'s number as `major.minor` (13030 → `13.3`).
fn cuda_version(version: i32) -> String {
    format!("{}.{}", version / 1000, version % 1000 / 10)
}

/// The probed GPU, or why CUDA can't run (without asking the driver where there is no
/// CUDA build).
fn host_gpu(platform: Platform) -> Result<CudaGpu, String> {
    if platform.has_cuda() {
        usable_gpu(&mtt_torch::cuda_driver())
    } else {
        Err("CUDA 빌드가 없습니다".to_owned())
    }
}

/// The build "LaMa 받기" fetches: CUDA when the platform has one, the user wants the GPU,
/// the driver offers one it runs on and an installed CUDA build hasn't failed to load.
fn wanted(platform: Platform, preference: Preference, gpu: &Result<CudaGpu, String>, installed: &Installed) -> Flavor {
    let failed = installed.cuda.as_ref().is_some_and(|cuda| cuda.failed.is_some());
    if platform.has_cuda() && preference == Preference::Gpu && gpu.is_ok() && !failed {
        Flavor::Cuda
    } else {
        Flavor::Cpu
    }
}

/// The build `activate` loads.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Choice {
    /// The CUDA build on the GPU.
    Cuda,
    /// The CPU build, with why CUDA isn't used when it's installed and wanted.
    Cpu(Option<String>),
    /// Nothing can run, with why when a CUDA build is installed.
    Nothing(Option<String>),
}

/// CUDA when it's installed and wanted, the GPU allows it and it didn't fail to load
/// before; otherwise the CPU build when it's installed.
fn choose(preference: Preference, installed: &Installed, gpu: &Result<CudaGpu, String>) -> Choice {
    let why_not_cuda = match (&installed.cuda, preference) {
        (None, _) | (_, Preference::Cpu) => None,
        (Some(cuda), Preference::Gpu) => match (&cuda.failed, gpu) {
            (Some(reason), _) => Some(format!("지난 실행에서 CUDA LibTorch를 불러오지 못했습니다 ({reason})")),
            (None, Err(reason)) => Some(reason.clone()),
            (None, Ok(_)) => return Choice::Cuda,
        },
    };
    if installed.cpu.is_some() {
        Choice::Cpu(why_not_cuda.map(|why| format!("CUDA 대신 CPU로 실행합니다: {why}")))
    } else {
        Choice::Nothing(why_not_cuda.map(|why| format!("CUDA를 쓸 수 없고({why}) CPU용 LibTorch도 없습니다.")))
    }
}

/// Whether a build that can run is installed: file checks only, for the UI thread (the
/// GPU is only asked when the build loads).
pub fn available(paths: &AppPaths) -> bool {
    Platform::host().is_some_and(|platform| available_in(&libtorch_dir(paths), platform))
}

fn available_in(root: &Path, platform: Platform) -> bool {
    let installed = Installed::find(root, platform);
    installed.cpu.is_some()
        || (read_preference(root) == Preference::Gpu
            && installed.cuda.as_ref().is_some_and(|cuda| cuda.failed.is_none()))
}

/// The LibTorch runtime as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    /// The build "LaMa 받기" fetches.
    pub wanted: Flavor,
    /// Its LibTorch folder; `None` where nothing is published for this platform.
    pub path: Option<PathBuf>,
    /// Whether all of it is installed.
    pub installed: bool,
    /// Its download size (all parts, compressed).
    pub size: u64,
    /// The CUDA build's download size; 0 where there is none.
    pub cuda_size: u64,
    /// Whether some build can run: the wanted one or the CPU one standing in.
    pub usable: bool,
    /// Whether the platform has a CUDA build (Windows, Linux x64).
    pub cuda_offered: bool,
    /// The GPU CUDA runs on, or why it can't.
    pub gpu: Result<String, String>,
    /// Where the models run, or will once loaded: `CUDA (…)`, `GPU (MPS)` or `CPU`.
    pub device: String,
    /// Why CUDA isn't used, or that a change waits for a restart.
    pub note: Option<String>,
}

/// The runtime's status. Asks the NVIDIA driver (once per process), so not on the UI
/// thread.
pub fn status(paths: &AppPaths) -> RuntimeStatus {
    let root = libtorch_dir(paths);
    match Platform::host() {
        Some(platform) => {
            status_of(&root, platform, read_preference(&root), &host_gpu(platform), ACTIVE.get().map(Active::summary))
        }
        None => RuntimeStatus {
            wanted: Flavor::Cpu,
            path: None,
            installed: false,
            size: 0,
            cuda_size: 0,
            usable: false,
            cuda_offered: false,
            gpu: Err("이 플랫폼용 LibTorch가 없습니다.".to_owned()),
            device: "CPU".to_owned(),
            note: None,
        },
    }
}

/// The loaded build as `status_of` needs it: its flavor, device label and note.
type Loaded<'a> = (Flavor, &'a str, Option<&'a str>);

fn status_of(
    root: &Path,
    platform: Platform,
    preference: Preference,
    gpu: &Result<CudaGpu, String>,
    loaded: Option<Loaded<'_>>,
) -> RuntimeStatus {
    let installed = Installed::find(root, platform);
    let wanted = wanted(platform, preference, gpu, &installed);
    let wanted_parts = parts(root, platform, wanted);
    let choice = choose(preference, &installed, gpu);
    let chosen = match &choice {
        Choice::Cuda => Some(Flavor::Cuda),
        Choice::Cpu(_) => Some(Flavor::Cpu),
        Choice::Nothing(_) => None,
    };
    let (device, note) = match loaded {
        Some((flavor, label, note)) => {
            let restart = (chosen.is_some_and(|chosen| chosen != flavor))
                .then_some("바꾼 실행 장치는 앱을 다시 시작하면 적용됩니다.");
            let note = match (note, restart) {
                (Some(note), Some(restart)) => Some(format!("{note} {restart}")),
                (note, restart) => note.or(restart).map(str::to_owned),
            };
            (label.to_owned(), note)
        }
        None => {
            let device = match (&choice, gpu) {
                (Choice::Cuda, Ok(gpu)) => device_label(Device::Cuda(gpu.index), Some(&gpu.name)),
                _ if platform == Platform::MacArm => device_label(Device::Mps, None),
                _ => device_label(Device::Cpu, None),
            };
            let note = match choice {
                Choice::Cpu(note) | Choice::Nothing(note) => note,
                Choice::Cuda => None,
            };
            (device, note)
        }
    };
    RuntimeStatus {
        wanted,
        path: wanted_parts.first().map(|part| part.dir.clone()),
        installed: !wanted_parts.is_empty() && wanted_parts.iter().all(Part::installed),
        size: wanted_parts.iter().map(|part| part.size).sum(),
        cuda_size: parts(root, platform, Flavor::Cuda).iter().map(|part| part.size).sum(),
        usable: chosen.is_some(),
        cuda_offered: platform.has_cuda(),
        gpu: gpu.as_ref().map(|gpu| gpu.name.clone()).map_err(Clone::clone),
        device,
        note,
    }
}

fn device_label(device: Device, gpu: Option<&str>) -> String {
    match (device, gpu) {
        (Device::Cpu, _) => "CPU".to_owned(),
        (Device::Mps, _) => "GPU (MPS)".to_owned(),
        (Device::Cuda(_), Some(name)) => format!("CUDA ({name})"),
        (Device::Cuda(index), None) => format!("CUDA {index}"),
    }
}

/// The build this process loaded and where the models run.
#[derive(Debug)]
struct Active {
    flavor: Flavor,
    device: Device,
    label: String,
    /// Why CUDA isn't used.
    note: Option<String>,
}

impl Active {
    fn summary(&self) -> Loaded<'_> {
        (self.flavor, &self.label, self.note.as_deref())
    }
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

/// Loads LibTorch from the runtime folder `root` (once per process) and returns the device
/// the models run on: the CUDA build on the GPU, falling back to the CPU build when CUDA
/// can't be used or fails to load.
pub(crate) fn activate(root: &Path) -> Result<Device, String> {
    static ACTIVATING: Mutex<()> = Mutex::new(());
    let _activating = ACTIVATING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(active) = ACTIVE.get() {
        return Ok(active.device);
    }
    let platform = Platform::host().ok_or_else(|| "이 플랫폼용 LibTorch가 없습니다.".to_owned())?;
    let active = load_runtime(root, platform, host_gpu(platform))?;
    log::info!("LibTorch {:?} build, models on {}", active.flavor, active.label);
    let device = active.device;
    let _ = ACTIVE.set(active);
    Ok(device)
}

/// Loads the build `choose` picks in `root`; when CUDA fails to load, the CPU build unless
/// part of the CUDA one has loaded (then the next run uses the CPU).
fn load_runtime(root: &Path, platform: Platform, gpu: Result<CudaGpu, String>) -> Result<Active, String> {
    let installed = Installed::find(root, platform);
    match (choose(read_preference(root), &installed, &gpu), &installed.cuda, gpu) {
        (Choice::Cuda, Some(cuda), Ok(gpu)) => match load_cuda(platform, cuda, &gpu) {
            Ok(active) => Ok(active),
            Err(error) if error.partial => {
                // The CPU build can't load next to part of the CUDA one: the next run uses it.
                log::error!("CUDA LibTorch failed to load part-way, the next run uses the CPU: {error}");
                let _ = std::fs::write(cuda.torch.join(FAILED_FILE), &error.message);
                let next = if installed.cpu.is_some() {
                    "앱을 다시 시작하면 CPU로 실행합니다."
                } else {
                    "설정 › 인페인트에서 CPU용 LibTorch를 받은 뒤 앱을 다시 시작해 주세요."
                };
                Err(format!("CUDA LibTorch를 불러오지 못했습니다: {error}. {next}"))
            }
            Err(error) => {
                log::warn!("CUDA LibTorch failed to load, using the CPU build: {error}");
                let why = format!("CUDA LibTorch를 불러오지 못했습니다 ({error})");
                let Some(cpu) = &installed.cpu else { return Err(format!("{why}. CPU용 LibTorch도 없습니다.")) };
                load_cpu(cpu, Some(format!("CUDA 대신 CPU로 실행합니다: {why}")))
                    .map_err(|e| format!("{why}. CPU용 LibTorch도 불러오지 못했습니다: {e}"))
            }
        },
        (Choice::Cpu(note), ..) => {
            if let Some(note) = &note {
                log::warn!("{note}");
            }
            load_cpu(installed.cpu.as_ref().expect("the CPU build is chosen when installed"), note)
        }
        (Choice::Nothing(why), ..) => Err(why.unwrap_or_else(|| "LibTorch가 설치되어 있지 않습니다.".to_owned())),
        (Choice::Cuda, ..) => unreachable!("CUDA is chosen when installed with a GPU"),
    }
}

fn load_cpu(dir: &Path, note: Option<String>) -> Result<Active, String> {
    mtt_torch::load(dir)?;
    let device = if mtt_torch::mps_available() { Device::Mps } else { Device::Cpu };
    Ok(Active { flavor: Flavor::Cpu, device, label: device_label(device, None), note })
}

/// Loads the CUDA build: NVIDIA's libraries first, so LibTorch's resolve to them (koharu
/// loads them the same way rather than through `PATH` or `LD_LIBRARY_PATH`). Once it has
/// loaded it is the process's LibTorch, so a GPU it can't use leaves it on the CPU.
fn load_cuda(platform: Platform, cuda: &CudaInstall, gpu: &CudaGpu) -> Result<Active, mtt_torch::LoadError> {
    mtt_torch::load_with(&cuda.dependencies, &cuda.torch, torch_libraries(platform, Flavor::Cuda))?;
    let device = cuda_device(mtt_torch::has_cuda(), mtt_torch::cuda_device_count(), gpu)
        .and_then(|device| smoke_test(device).map(|()| device));
    Ok(match device {
        Ok(device) => Active { flavor: Flavor::Cuda, device, label: device_label(device, Some(&gpu.name)), note: None },
        Err(why) => {
            log::warn!("CUDA LibTorch loaded but runs on the CPU: {why}");
            let note = Some(format!("CUDA 대신 CPU로 실행합니다: {why}"));
            Active { flavor: Flavor::Cuda, device: Device::Cpu, label: device_label(Device::Cpu, None), note }
        }
    })
}

/// The device on the loaded CUDA build: `gpu` when LibTorch sees it.
fn cuda_device(has_cuda: bool, devices: usize, gpu: &CudaGpu) -> Result<Device, String> {
    if !has_cuda {
        Err("불러온 LibTorch에 CUDA가 없습니다".to_owned())
    } else if devices == 0 {
        Err("LibTorch가 CUDA 장치를 찾지 못했습니다".to_owned())
    } else {
        Ok(Device::Cuda(if gpu.index < devices { gpu.index } else { 0 }))
    }
}

/// A small operation on `device`, which fails when its kernels can't run there.
fn smoke_test(device: Device) -> Result<(), String> {
    let values = mtt_torch::Tensor::from_f32(&[-1.0, 2.0], &[2])?.to(device)?.relu()?.to_f32_vec()?;
    if values == [0.0, 2.0] { Ok(()) } else { Err(format!("GPU 시험 연산 결과가 틀립니다: {values:?}")) }
}

/// Downloads, checks and unpacks whatever is missing of the build "LaMa 받기" wants (the
/// parts in turn, progress over all of them). Concurrent calls (LaMa's and the
/// segmentation model's downloads) wait for each other.
pub fn download(paths: &AppPaths, mut progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let Some(platform) = Platform::host() else {
        return Err("이 플랫폼용 LibTorch가 없습니다.".into());
    };
    static DOWNLOADING: Mutex<()> = Mutex::new(());
    let _downloading = DOWNLOADING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = libtorch_dir(paths);
    let installed = Installed::find(&root, platform);
    let flavor = wanted(platform, read_preference(&root), &host_gpu(platform), &installed);
    let missing: Vec<Part> = parts(&root, platform, flavor).into_iter().filter(|part| !part.installed()).collect();
    let total = missing.iter().map(|part| part.size).sum::<u64>();
    let mut done = 0;
    for part in &missing {
        log::info!("downloading {}", part.url);
        part.fetch(|written| progress(done + written, Some(total)))?;
        done += part.size;
    }
    Ok(())
}

impl Part {
    /// What the part is, in messages.
    fn label(&self) -> String {
        match self.dir.file_name().filter(|_| self.wheel) {
            Some(name) => format!("CUDA 라이브러리({})", name.to_string_lossy()),
            None => "LibTorch".to_owned(),
        }
    }

    fn fetch(&self, mut progress: impl FnMut(u64)) -> Result<(), String> {
        let parent = self.dir.parent().expect("a part's folder has a parent");
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let archive = with_suffix(&self.dir, if self.wheel { ".whl" } else { ".tar.gz" });
        crate::lama::download(&self.url, &archive, |written, _| progress(written))?;
        let result = verify(&archive, self.size, self.sha256, &self.label()).and_then(|()| self.unpack(&archive));
        let _ = std::fs::remove_file(&archive);
        result
    }

    /// The libraries from `archive`, flat into the part's folder (via a `.partial` folder,
    /// so a cut-off unpack is never taken for an install).
    fn unpack(&self, archive: &Path) -> Result<(), String> {
        let partial = with_suffix(&self.dir, ".partial");
        let _ = std::fs::remove_dir_all(&partial);
        std::fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
        let label = self.label();
        let extracted = if self.wheel {
            unzip(archive, &partial, self.libraries)
        } else {
            untar(archive, &partial, self.libraries)
        };
        let unpacked = extracted.map_err(|e| format!("{label} 압축을 풀지 못했습니다: {e}")).and_then(|()| {
            complete(&partial, self.libraries)
                .then_some(())
                .ok_or_else(|| format!("{label} 압축에 필요한 파일이 없습니다."))
        });
        if let Err(error) = unpacked {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(error);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
        std::fs::rename(&partial, &self.dir).map_err(|e| e.to_string())
    }
}

/// `path` with `suffix` appended to its last component (`with_extension` would cut at the
/// release's dots).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether `archive` has the pinned `size` and SHA-256; `label` names it in the error.
pub(crate) fn verify(archive: &Path, size: u64, expected: &str, label: &str) -> Result<(), String> {
    let actual = std::fs::metadata(archive).map_err(|e| e.to_string())?.len();
    if actual != size {
        return Err(format!(
            "{label} 파일 크기가 맞지 않습니다 ({actual}바이트, {size}바이트여야 합니다). 다시 받아 주세요."
        ));
    }
    let actual = sha256(archive)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{label} 파일이 손상되었습니다 (SHA-256 {actual}). 다시 받아 주세요."))
    }
}

/// The file's SHA-256 in lowercase hex.
pub(crate) fn sha256(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The listed libraries of a koharu-rs/torch tarball, by file name: nothing lands outside
/// `into`.
fn untar(archive: &Path, into: &Path, libraries: &[&str]) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        if entry.header().entry_type().is_file() && libraries.contains(&name) {
            entry.unpack(into.join(name)).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    Ok(())
}

/// The listed libraries of a wheel, by file name (a wheel nests them, e.g.
/// `nvidia/cu13/lib/`).
fn unzip(archive: &Path, into: &Path, libraries: &[&str]) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let Some(path) = entry.enclosed_name() else { continue };
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        if entry.is_file() && libraries.contains(&name) {
            let mut output = std::fs::File::create(into.join(name)).map_err(|e| format!("{name}: {e}"))?;
            std::io::copy(&mut entry, &mut output).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORMS: [Platform; 3] = [Platform::MacArm, Platform::WindowsX64, Platform::LinuxX64];

    /// A gzipped tar of `files`.
    fn tarball(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("runtime.tar.gz");
        let encoder = flate2::write::GzEncoder::new(std::fs::File::create(&path).unwrap(), Default::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    /// A wheel (zip, deflated like NVIDIA's) of `files`.
    fn wheel(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let path = dir.join("runtime.whl");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }

    /// Empty files for every library of `flavor` in `root`.
    fn install(root: &Path, platform: Platform, flavor: Flavor) {
        for part in parts(root, platform, flavor) {
            std::fs::create_dir_all(&part.dir).unwrap();
            for path in part.library_paths() {
                std::fs::write(path, b"").unwrap();
            }
        }
    }

    fn gpu(index: usize, name: &str, compute_capability: u32, memory_gb: u64, integrated: bool) -> CudaGpu {
        CudaGpu { index, name: name.to_owned(), compute_capability, memory: memory_gb << 30, integrated }
    }

    fn rtx() -> CudaGpu {
        gpu(0, "NVIDIA GeForce RTX 4070", 89, 12, false)
    }

    #[test]
    fn each_platform_gets_its_archives() {
        let root = Path::new("/data/tools/libtorch");
        let asset = |platform, flavor| {
            parts(root, platform, flavor).first().map(|part| part.url.rsplit('/').next().unwrap().to_owned())
        };
        assert_eq!(asset(Platform::MacArm, Flavor::Cpu).unwrap(), "aarch64-apple-darwin-metal.tar.gz");
        assert_eq!(asset(Platform::WindowsX64, Flavor::Cpu).unwrap(), "x86_64-pc-windows-msvc-cpu.tar.gz");
        assert_eq!(asset(Platform::LinuxX64, Flavor::Cpu).unwrap(), "x86_64-unknown-linux-gnu-cpu.tar.gz");
        assert_eq!(asset(Platform::WindowsX64, Flavor::Cuda).unwrap(), "x86_64-pc-windows-msvc-cuda.tar.gz");
        assert_eq!(asset(Platform::LinuxX64, Flavor::Cuda).unwrap(), "x86_64-unknown-linux-gnu-cuda.tar.gz");
        assert!(parts(root, Platform::MacArm, Flavor::Cuda).is_empty());
        assert!(!Platform::MacArm.has_cuda() && Platform::WindowsX64.has_cuda() && Platform::LinuxX64.has_cuda());
        for platform in PLATFORMS {
            assert_eq!(parts(root, platform, Flavor::Cpu).len(), 1, "{platform:?}: the CPU build is one archive");
        }
    }

    #[test]
    fn cuda_builds_bring_koharus_nvidia_wheels_in_load_order() {
        let root = Path::new("/data/tools/libtorch");
        let order = [
            "nvidia-cuda-runtime-13.3.29",
            "nvidia-nvjitlink-13.3.33",
            "nvidia-cuda-nvrtc-13.3.33",
            "nvidia-cublas-13.6.0.2",
            "nvidia-cufft-12.3.0.29",
            "nvidia-curand-10.4.3.29",
            "nvidia-cusparse-12.8.2.51",
            "nvidia-cusolver-12.2.6.9",
            "nvidia-cudnn-cu13-9.25.1.1",
        ];
        for (platform, tag, extension, total) in [
            (Platform::WindowsX64, "win_amd64", ".dll", 2_336_171_713u64),
            (Platform::LinuxX64, "manylinux", ".so", 2_722_387_585),
        ] {
            let parts = parts(root, platform, Flavor::Cuda);
            assert_eq!(parts.len(), 1 + order.len());
            assert!(!parts[0].wheel && parts[0].url.contains(RELEASE));
            assert_eq!(parts[0].libraries.last().unwrap(), &torch_libraries(platform, Flavor::Cuda)[7]);
            assert!(parts[0].libraries.last().unwrap().contains("koharu-torch"), "the shim loads last");
            for (part, name) in parts[1..].iter().zip(order) {
                assert!(part.wheel);
                assert_eq!(part.dir, root.join("nvidia").join(name));
                let file = part.url.rsplit('/').next().unwrap();
                let (project, version) = name.rsplit_once('-').unwrap();
                assert!(file.starts_with(&format!("{}-{version}-", project.replace('-', "_"))), "{file}");
                assert!(file.contains(tag) && file.ends_with(".whl"), "{file}");
                assert!(part.url.starts_with("https://files.pythonhosted.org/packages/"));
                assert!(part.libraries.iter().all(|library| library.contains(extension)));
            }
            assert_eq!(parts.iter().map(|part| part.size).sum::<u64>(), total, "{platform:?} CUDA download");
            let libraries: Vec<&str> = parts[1..].iter().flat_map(|part| part.libraries.iter().copied()).collect();
            let position = |name: &str| libraries.iter().position(|library| library.starts_with(name)).unwrap();
            // cuBLASLt before cuBLAS, nvJitLink before what links it, cuDNN last.
            assert!(
                position(if extension == ".dll" { "cublasLt" } else { "libcublasLt" })
                    < position(if extension == ".dll" { "cublas64" } else { "libcublas.so" })
            );
            assert!(
                position(if extension == ".dll" { "nvJitLink" } else { "libnvJitLink" })
                    < position(if extension == ".dll" { "cusparse" } else { "libcusparse" })
            );
            assert!(libraries.last().unwrap().contains("cudnn_ops"));
        }
    }

    #[test]
    fn pins_are_well_formed() {
        let root = Path::new("/r");
        for platform in PLATFORMS {
            for flavor in [Flavor::Cpu, Flavor::Cuda] {
                for part in parts(root, platform, flavor) {
                    assert_eq!(part.sha256.len(), 64, "{}", part.url);
                    assert!(part.sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
                    assert!(part.size > 1_000_000 && part.url.starts_with("https://"));
                    assert!(!part.libraries.is_empty());
                }
            }
        }
    }

    #[test]
    fn cpu_layout_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_root(dir.path());
        let root = libtorch_dir(&paths);
        assert_eq!(root, dir.path().join("tools").join("libtorch"));
        let platform = Platform::host().expect("tests run on a platform with a build");
        let cpu = &parts(&root, platform, Flavor::Cpu)[0];
        let asset = build(platform, Flavor::Cpu).unwrap().asset;
        assert_eq!(cpu.dir, root.join(format!("{RELEASE}-{asset}")));
        assert_eq!(with_suffix(&cpu.dir, ".tar.gz"), root.join(format!("{RELEASE}-{asset}.tar.gz")));
        assert_eq!(with_suffix(&cpu.dir, ".partial"), root.join(format!("{RELEASE}-{asset}.partial")));
        assert_eq!(cpu.libraries, mtt_torch::LIBRARIES);
        assert!(!cpu.wheel);
    }

    #[test]
    fn driver_versions_read_as_cuda_versions() {
        assert_eq!(cuda_version(13030), "13.3");
        assert_eq!(cuda_version(13000), "13.0");
        assert_eq!(cuda_version(12080), "12.8");
        assert_eq!(cuda_version(MIN_DRIVER_VERSION), "13.3");
    }

    #[test]
    fn the_gpu_needs_a_recent_driver_and_turing_or_newer() {
        let driver = |version, gpus: Vec<CudaGpu>| Ok(CudaDriver { version, gpus });
        assert_eq!(usable_gpu(&Err("드라이버 없음".into())), Err("드라이버 없음".into()));
        let old = usable_gpu(&driver(13020, vec![rtx()])).unwrap_err();
        assert!(old.contains("CUDA 13.2까지만") && old.contains("13.3 이상"), "{old}");
        assert!(usable_gpu(&driver(13030, Vec::new())).unwrap_err().contains("GPU를 찾지 못했습니다"));
        let pascal = usable_gpu(&driver(13040, vec![gpu(0, "GTX 1080", 61, 8, false)])).unwrap_err();
        assert!(pascal.contains("GTX 1080") && pascal.contains("6.1") && pascal.contains("7.5"), "{pascal}");
        assert_eq!(usable_gpu(&driver(13030, vec![rtx()])), Ok(rtx()));
        // Discrete before integrated, then memory, then compute capability.
        let gpus = vec![
            gpu(0, "Orin", 87, 64, true),
            gpu(1, "T4", 75, 16, false),
            gpu(2, "GTX 1080 Ti", 61, 24, false),
            gpu(3, "RTX 3060", 86, 16, false),
            gpu(4, "RTX 4060 Ti", 89, 16, false),
        ];
        assert_eq!(usable_gpu(&driver(13030, gpus)).unwrap().name, "RTX 4060 Ti");
        assert_eq!(usable_gpu(&driver(13030, vec![gpu(0, "Orin", 87, 64, true)])).unwrap().name, "Orin");
    }

    fn installed(cpu: bool, cuda: bool, failed: Option<&str>) -> Installed {
        Installed {
            cpu: cpu.then(|| PathBuf::from("/cpu")),
            cuda: cuda.then(|| CudaInstall {
                torch: PathBuf::from("/cuda"),
                dependencies: Vec::new(),
                failed: failed.map(str::to_owned),
            }),
        }
    }

    #[test]
    fn cuda_runs_when_installed_wanted_and_the_gpu_allows() {
        let ok: Result<CudaGpu, String> = Ok(rtx());
        let no_gpu: Result<CudaGpu, String> = Err("NVIDIA GPU를 찾지 못했습니다".into());
        let gpu = Preference::Gpu;
        assert_eq!(choose(gpu, &installed(true, true, None), &ok), Choice::Cuda);
        assert_eq!(choose(gpu, &installed(false, true, None), &ok), Choice::Cuda);
        // No GPU: the CPU build, saying why.
        let Choice::Cpu(Some(note)) = choose(gpu, &installed(true, true, None), &no_gpu) else { panic!() };
        assert!(note.contains("CPU로 실행") && note.contains("GPU를 찾지 못했습니다"), "{note}");
        let Choice::Nothing(Some(why)) = choose(gpu, &installed(false, true, None), &no_gpu) else { panic!() };
        assert!(why.contains("CPU용 LibTorch도 없습니다"), "{why}");
        // A build that failed to load part-way in an earlier run isn't tried again.
        let Choice::Cpu(Some(note)) = choose(gpu, &installed(true, true, Some("torch_cuda.dll: 126")), &ok) else {
            panic!()
        };
        assert!(note.contains("지난 실행") && note.contains("torch_cuda.dll"), "{note}");
        // The user's CPU choice, or no CUDA build: the CPU build without a note.
        assert_eq!(choose(Preference::Cpu, &installed(true, true, None), &ok), Choice::Cpu(None));
        assert_eq!(choose(Preference::Cpu, &installed(false, true, None), &ok), Choice::Nothing(None));
        assert_eq!(choose(gpu, &installed(true, false, None), &ok), Choice::Cpu(None));
        assert_eq!(choose(gpu, &installed(false, false, None), &ok), Choice::Nothing(None));
    }

    #[test]
    fn la_ma_download_fetches_cuda_only_with_a_usable_gpu() {
        let ok: Result<CudaGpu, String> = Ok(rtx());
        let no_gpu: Result<CudaGpu, String> = Err("x".into());
        let none = Installed::default();
        for platform in [Platform::WindowsX64, Platform::LinuxX64] {
            assert_eq!(wanted(platform, Preference::Gpu, &ok, &none), Flavor::Cuda);
            assert_eq!(wanted(platform, Preference::Gpu, &no_gpu, &none), Flavor::Cpu);
            assert_eq!(wanted(platform, Preference::Cpu, &ok, &none), Flavor::Cpu);
            assert_eq!(wanted(platform, Preference::Gpu, &ok, &installed(false, true, Some("x"))), Flavor::Cpu);
        }
        assert_eq!(wanted(Platform::MacArm, Preference::Gpu, &ok, &none), Flavor::Cpu);
    }

    #[test]
    fn installs_are_found_with_their_libraries_in_load_order() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let platform = Platform::LinuxX64;
        assert_eq!(Installed::find(root, platform), Installed::default());
        install(root, platform, Flavor::Cuda);
        let found = Installed::find(root, platform);
        assert_eq!(found.cpu, None);
        let cuda = found.cuda.unwrap();
        assert_eq!(cuda.torch, root.join(format!("{RELEASE}-x86_64-unknown-linux-gnu-cuda")));
        assert_eq!(cuda.dependencies.len(), 21);
        assert_eq!(cuda.dependencies[0], root.join("nvidia/nvidia-cuda-runtime-13.3.29/libcudart.so.13"));
        assert_eq!(cuda.dependencies[20], root.join("nvidia/nvidia-cudnn-cu13-9.25.1.1/libcudnn_ops.so.9"));
        assert_eq!(cuda.failed, None);
        assert!(available_in(root, platform));
        assert!(Installed::find(root, Platform::WindowsX64).cuda.is_none(), "Windows has its own folders");

        std::fs::write(cuda.torch.join(FAILED_FILE), "libtorch_cuda.so: undefined symbol\n").unwrap();
        let failed = Installed::find(root, platform).cuda.unwrap().failed;
        assert_eq!(failed.as_deref(), Some("libtorch_cuda.so: undefined symbol"));
        assert!(!available_in(root, platform), "a failed CUDA build alone can't run");

        // Choosing the GPU again retries it; choosing the CPU leaves only a CPU build usable.
        save_preference(root, Some(platform), Preference::Gpu).unwrap();
        assert!(Installed::find(root, platform).cuda.unwrap().failed.is_none());
        save_preference(root, Some(platform), Preference::Cpu).unwrap();
        assert_eq!(read_preference(root), Preference::Cpu);
        assert!(!available_in(root, platform));
        install(root, platform, Flavor::Cpu);
        assert!(available_in(root, platform));
        save_preference(root, Some(platform), Preference::Gpu).unwrap();
        assert_eq!(read_preference(root), Preference::Gpu);
        assert!(!root.join(PREFERENCE_FILE).exists());

        std::fs::remove_file(&cuda.dependencies[5]).unwrap();
        assert!(Installed::find(root, platform).cuda.is_none(), "a missing library is no install");
    }

    #[test]
    fn status_shows_the_device_and_what_to_download() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let platform = Platform::LinuxX64;
        let ok: Result<CudaGpu, String> = Ok(rtx());

        let status = status_of(root, platform, Preference::Gpu, &ok, None);
        assert_eq!((status.wanted, status.installed, status.usable), (Flavor::Cuda, false, false));
        assert_eq!(status.path, Some(root.join(format!("{RELEASE}-x86_64-unknown-linux-gnu-cuda"))));
        assert_eq!((status.size, status.cuda_size), (2_722_387_585, 2_722_387_585));
        assert_eq!(status.gpu, Ok("NVIDIA GeForce RTX 4070".to_owned()));
        assert!(status.cuda_offered);

        // CPU installed, CUDA wanted: runs on the CPU meanwhile.
        install(root, platform, Flavor::Cpu);
        let status = status_of(root, platform, Preference::Gpu, &ok, None);
        assert_eq!((status.wanted, status.installed, status.usable), (Flavor::Cuda, false, true));
        assert_eq!((status.device.as_str(), status.note.as_deref()), ("CPU", None));

        install(root, platform, Flavor::Cuda);
        let status = status_of(root, platform, Preference::Gpu, &ok, None);
        assert!(status.installed && status.usable);
        assert_eq!(status.device, "CUDA (NVIDIA GeForce RTX 4070)");

        // The CPU build loaded before CUDA arrived: a restart switches.
        let status = status_of(root, platform, Preference::Gpu, &ok, Some((Flavor::Cpu, "CPU", None)));
        assert_eq!(status.device, "CPU");
        assert!(status.note.unwrap().contains("다시 시작"));
        let note = "CUDA 대신 CPU로 실행합니다: LibTorch가 CUDA 장치를 찾지 못했습니다";
        let status = status_of(root, platform, Preference::Gpu, &ok, Some((Flavor::Cuda, "CPU", Some(note))));
        assert_eq!(status.note.as_deref(), Some(note));

        // No usable GPU: the CPU build, with why.
        let no_gpu: Result<CudaGpu, String> = Err("NVIDIA 드라이버가 CUDA 12.8까지만 지원합니다".into());
        let status = status_of(root, platform, Preference::Gpu, &no_gpu, None);
        assert_eq!((status.wanted, status.installed, status.device.as_str()), (Flavor::Cpu, true, "CPU"));
        assert!(status.note.unwrap().contains("12.8"));
        let status = status_of(root, platform, Preference::Cpu, &ok, None);
        assert_eq!((status.wanted, status.note), (Flavor::Cpu, None));
    }

    #[test]
    fn macs_keep_the_mps_build() {
        let temp = tempfile::tempdir().unwrap();
        let ok: Result<CudaGpu, String> = Ok(rtx());
        let status = status_of(temp.path(), Platform::MacArm, Preference::Gpu, &ok, None);
        assert_eq!((status.wanted, status.cuda_offered, status.usable), (Flavor::Cpu, false, false));
        assert_eq!(status.device, "GPU (MPS)");
        assert_eq!((status.size, status.cuda_size), (75_866_269, 0));
        assert_eq!(status.note, None);
        install(temp.path(), Platform::MacArm, Flavor::Cpu);
        assert_eq!(choose(Preference::Gpu, &Installed::find(temp.path(), Platform::MacArm), &ok), Choice::Cpu(None));
    }

    #[test]
    fn the_loaded_cuda_build_uses_the_gpu_libtorch_sees() {
        let gpu = gpu(1, "RTX 4070", 89, 12, false);
        assert!(cuda_device(false, 2, &gpu).unwrap_err().contains("CUDA가 없습니다"));
        assert!(cuda_device(true, 0, &gpu).unwrap_err().contains("CUDA 장치를 찾지 못했습니다"));
        assert_eq!(cuda_device(true, 2, &gpu), Ok(Device::Cuda(1)));
        assert_eq!(cuda_device(true, 1, &gpu), Ok(Device::Cuda(0)));
        assert_eq!(device_label(Device::Cuda(1), Some("RTX 4070")), "CUDA (RTX 4070)");
        assert_eq!(device_label(Device::Mps, None), "GPU (MPS)");
        assert_eq!(device_label(Device::Cpu, None), "CPU");
    }

    #[test]
    fn a_cuda_build_that_fails_to_load_falls_back_to_the_cpu_build() {
        // Empty files: the first NVIDIA library fails before any of LibTorch opens, so the
        // CPU build is tried next (and fails too, being empty).
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let platform = Platform::LinuxX64;
        install(root, platform, Flavor::Cuda);
        let error = load_runtime(root, platform, Ok(rtx())).unwrap_err();
        assert!(error.contains("CUDA LibTorch를 불러오지 못했습니다") && error.contains("libcudart.so.13"), "{error}");
        assert!(error.contains("CPU용 LibTorch도 없습니다"), "{error}");
        install(root, platform, Flavor::Cpu);
        let error = load_runtime(root, platform, Ok(rtx())).unwrap_err();
        assert!(error.contains("CPU용 LibTorch도 불러오지 못했습니다"), "{error}");
        let cuda = Installed::find(root, platform).cuda.unwrap();
        assert_eq!(cuda.failed, None, "nothing of LibTorch loaded, so CUDA is tried again next time");
        // Without a usable GPU the CUDA build isn't touched.
        let error = load_runtime(root, platform, Err("NVIDIA GPU를 찾지 못했습니다".into())).unwrap_err();
        assert!(!error.contains("libcudart"), "{error}");
    }

    #[test]
    fn tarballs_unpack_only_the_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let mut files: Vec<(String, &[u8])> =
            mtt_torch::LIBRARIES.iter().map(|name| (format!("./{name}"), b"lib".as_slice())).collect();
        files.push(("./README.md".into(), b"x"));
        files.push(("./include/torch.h".into(), b"x"));
        let files: Vec<(&str, &[u8])> = files.iter().map(|(name, data)| (name.as_str(), *data)).collect();
        let part =
            Part { dir: temp.path().join("runtime"), ..parts(temp.path(), Platform::MacArm, Flavor::Cpu).remove(0) };
        part.unpack(&tarball(temp.path(), &files)).unwrap();
        assert!(part.installed());
        let mut expected: Vec<String> = mtt_torch::LIBRARIES.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(names(&part.dir), expected);
    }

    #[test]
    fn wheels_unpack_their_nested_libraries_flat() {
        let temp = tempfile::tempdir().unwrap();
        let part = parts(temp.path(), Platform::LinuxX64, Flavor::Cuda).remove(4);
        assert_eq!(part.libraries, ["libcublasLt.so.13", "libcublas.so.13"]);
        let archive = wheel(
            temp.path(),
            &[
                ("nvidia/__init__.py", b""),
                ("nvidia/cu13/lib/libcublasLt.so.13", b"lt"),
                ("nvidia/cu13/lib/libcublas.so.13", b"blas"),
                ("nvidia/cu13/lib/libnvblas.so.13", b"nv"),
                ("nvidia/cu13/include/cublas.h", b"h"),
                ("../escape/libcublas.so.13", b"evil"),
            ],
        );
        part.unpack(&archive).unwrap();
        assert!(part.installed());
        assert_eq!(names(&part.dir), ["libcublas.so.13", "libcublasLt.so.13"]);
        assert_eq!(std::fs::read(part.dir.join("libcublas.so.13")).unwrap(), b"blas");
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    fn incomplete_archives_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let part =
            Part { dir: temp.path().join("runtime"), ..parts(temp.path(), Platform::MacArm, Flavor::Cpu).remove(0) };
        let error = part.unpack(&tarball(temp.path(), &[(mtt_torch::LIBRARIES[0], b"lib")])).unwrap_err();
        assert!(error.contains("필요한 파일이 없습니다"), "{error}");
        assert!(!part.dir.exists() && !with_suffix(&part.dir, ".partial").exists());
        let part = parts(temp.path(), Platform::WindowsX64, Flavor::Cuda).remove(9);
        let error = part.unpack(&wheel(temp.path(), &[("nvidia/cudnn/bin/cudnn64_9.dll", b"x")])).unwrap_err();
        assert!(error.contains("CUDA 라이브러리(nvidia-cudnn-cu13-9.25.1.1)"), "{error}");
        assert!(!part.dir.exists());
    }

    /// The whole path on this machine: `download`, `status`, `activate` and an operation on
    /// the device. On Windows or Linux with an NVIDIA GPU that's the CUDA build (~2.2–2.6GB):
    /// `MTT_RUNTIME_TEST_DATA=<folder> cargo test -p mtt-inpaint -- --ignored --nocapture
    /// downloads_and_loads` (a temporary folder without it).
    #[test]
    #[ignore = "downloads this machine's LibTorch build (CUDA's on an NVIDIA GPU)"]
    fn downloads_and_loads_this_machines_build() {
        let temp = tempfile::tempdir().unwrap();
        let data = std::env::var_os("MTT_RUNTIME_TEST_DATA").map_or_else(|| temp.path().to_owned(), PathBuf::from);
        let paths = AppPaths::from_data_root(&data);
        let before = status(&paths);
        println!("GPU: {:?}; downloading the {:?} build ({} bytes)", before.gpu, before.wanted, before.size);
        let mut last = 0;
        download(&paths, |written, total| {
            if written - last > 100 << 20 || Some(written) == total {
                println!("{written}/{total:?}");
                last = written;
            }
        })
        .unwrap();
        let after = status(&paths);
        assert!(after.installed && after.usable, "{after:?}");
        let device = activate(&libtorch_dir(&paths)).unwrap();
        smoke_test(device).unwrap();
        let loaded = status(&paths);
        println!("device {device:?}, shown as {:?}, note {:?}", loaded.device, loaded.note);
        match after.wanted {
            Flavor::Cuda => {
                assert!(mtt_torch::has_cuda());
                assert!(matches!(device, Device::Cuda(_)), "{:?}", loaded.note);
                assert!(loaded.device.starts_with("CUDA ("));
            }
            Flavor::Cpu => {
                assert!(!mtt_torch::has_cuda() && mtt_torch::cuda_device_count() == 0);
                let expected = if mtt_torch::mps_available() { "GPU (MPS)" } else { "CPU" };
                assert_eq!(loaded.device, expected);
            }
        }
        assert_eq!(loaded.note, None);
    }

    #[test]
    fn downloads_are_checked_by_size_and_hash() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("data");
        std::fs::write(&file, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        verify(&file, 3, abc, "LibTorch").unwrap();
        assert!(verify(&file, 4, abc, "LibTorch").unwrap_err().contains("크기"));
        assert!(verify(&file, 3, "00", "LibTorch").unwrap_err().contains("SHA-256"));
    }
}
