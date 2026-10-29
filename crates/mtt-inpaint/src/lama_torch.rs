//! LaMa on LibTorch (`mtt-torch`): Big-LaMa's `FFCResNetGenerator` written out with
//! torch operations, on MPS when the Mac has it and otherwise the CPU. BatchNorm is folded
//! into the convolutions when the weights load.

use std::path::Path;

use image::{GrayImage, RgbaImage};
use mtt_torch::{Device, Tensor};

use crate::torch_weights::{Array, Conv, Folded, Weights};

const BLOCKS: usize = 18;
/// Channels of the resnet blocks: `LOCAL` through plain convolutions, `GLOBAL` through
/// the spectral transform, whose Fourier unit works on `SPECTRAL` of them.
const LOCAL: i64 = 128;
const GLOBAL: i64 = 384;
const SPECTRAL: i64 = GLOBAL / 2;
/// The generator halves the size three times, so inputs are padded to a multiple of this.
const PAD_MULTIPLE: usize = 8;
/// Mask values above this are inpainted.
const MASK_THRESHOLD: u8 = 8;

pub struct TorchLama {
    device: Device,
    /// The 7×7 input convolution.
    init: Conv,
    /// Three stride-2 3×3 convolutions; the last gives the local then the global channels.
    down: [Conv; 3],
    blocks: Vec<[FfcLayer; 2]>,
    /// Transposed convolutions (stride 2).
    up: [Conv; 3],
    /// The 7×7 output convolution.
    out: Conv,
}

/// One `FFC_BN_ACT` of a resnet block.
struct FfcLayer {
    /// `convl2l` and `convg2l` against the local then the global channels.
    local: Conv,
    /// `convl2g`, with the global BatchNorm's shift.
    to_global: Conv,
    /// The spectral transform: `conv1`, the Fourier unit's convolution and `conv2`.
    spectral_in: Conv,
    fourier: Conv,
    spectral_out: Conv,
}

impl TorchLama {
    /// Reads the checkpoint onto `device`. LibTorch must be loaded.
    pub fn load(path: &Path, device: Device) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("LaMa 모델을 읽지 못했습니다 ({}): {e}", path.display()))?;
        let weights = Weights::parse(&bytes).map_err(|e| format!("LaMa 모델을 읽지 못했습니다: {e}"))?;
        Self::from_weights(&weights, device).map_err(|e| format!("LaMa 모델 구조가 맞지 않습니다: {e}"))
    }

    /// MPS when available, otherwise the CPU.
    pub fn default_device() -> Device {
        crate::torch_runtime::default_device()
    }

    pub fn device(&self) -> Device {
        self.device
    }

    fn from_weights(weights: &Weights, device: Device) -> Result<Self, String> {
        let conv =
            |name: &str, batch_norm: &str| weights.conv(&format!("{name}.weight"), None, Some(batch_norm), device);
        let down2 = {
            let local = weights.folded("model.4.ffc.convl2l.weight", None, Some("model.4.bn_l"), 0)?;
            let global = weights.folded("model.4.ffc.convl2g.weight", None, Some("model.4.bn_g"), 0)?;
            local.concat(global, 0)?.upload(device)?
        };
        let blocks = (0..BLOCKS)
            .map(|index| {
                let prefix = format!("model.{}", 5 + index);
                Ok([
                    weights.ffc_layer(&format!("{prefix}.conv1"), device)?,
                    weights.ffc_layer(&format!("{prefix}.conv2"), device)?,
                ])
            })
            .collect::<Result<Vec<_>, String>>()?;
        let up = |index: usize| {
            let bias = format!("model.{index}.bias");
            weights
                .folded(&format!("model.{index}.weight"), Some(&bias), Some(&format!("model.{}", index + 1)), 1)?
                .upload(device)
        };
        Ok(Self {
            device,
            init: conv("model.1.ffc.convl2l", "model.1.bn_l")?,
            down: [conv("model.2.ffc.convl2l", "model.2.bn_l")?, conv("model.3.ffc.convl2l", "model.3.bn_l")?, down2],
            blocks,
            up: [up(24)?, up(27)?, up(30)?],
            out: weights.conv("model.34.weight", Some("model.34.bias"), None, device)?,
        })
    }

    /// Inpaints `mask` (white = inpaint) in `image` as one window. Pixels outside the mask
    /// come back unchanged.
    pub fn inpaint(&self, image: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        self.try_inpaint(image, mask).map_err(|e| format!("LaMa 인페인트 실패: {e}"))
    }

    fn try_inpaint(&self, image: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        let (width, height) = (image.width() as usize, image.height() as usize);
        let padded_width = width.div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
        let padded_height = height.div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
        let input = model_input(image, mask, padded_width, padded_height);
        let input = Tensor::from_f32(&input, &[1, 4, padded_height as i64, padded_width as i64])?.to(self.device)?;
        let predicted = self.generate(&input)?.to_f32_vec()?;
        let plane = padded_width * padded_height;
        Ok(RgbaImage::from_fn(image.width(), image.height(), |x, y| {
            let mut pixel = *image.get_pixel(x, y);
            if mask.get_pixel(x, y)[0] > MASK_THRESHOLD {
                let index = y as usize * padded_width + x as usize;
                for channel in 0..3 {
                    pixel[channel] = (predicted[channel * plane + index] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            pixel[3] = 255;
            pixel
        }))
    }

    /// The generator on `(1, 4, H, W)` with `H` and `W` multiples of 8; returns
    /// `(1, 3, H, W)` in 0..1.
    fn generate(&self, input: &Tensor) -> Result<Tensor, String> {
        let mut x = self.init.forward(&input.reflection_pad2d(3)?, 1)?.relu()?;
        for conv in &self.down {
            x = conv.forward(&x.reflection_pad2d(1)?, 2)?.relu()?;
        }
        let (mut local, mut global) = (x.narrow(1, 0, LOCAL)?, x.narrow(1, LOCAL, GLOBAL)?);
        for [first, second] in &self.blocks {
            let (l, g) = first.forward(&local, &global)?;
            let (l, g) = second.forward(&l, &g)?;
            (local, global) = (local.add(&l)?, global.add(&g)?);
        }
        let mut x = Tensor::cat(&[&local, &global], 1)?;
        for conv in &self.up {
            x = x.conv_transpose2d(&conv.weight, conv.bias.as_ref(), 2, 1, 1)?.relu()?;
        }
        self.out.forward(&x.reflection_pad2d(3)?, 1)?.sigmoid()
    }
}

/// RGB with the masked pixels zeroed, then the mask, as `(4, H, W)` planes padded to
/// `width`×`height` with the image reflected and the mask clear.
fn model_input(image: &RgbaImage, mask: &GrayImage, width: usize, height: usize) -> Vec<f32> {
    let plane = width * height;
    let mut input = vec![0f32; 4 * plane];
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = (reflect(x, image.width() as usize), reflect(y, image.height() as usize));
            let p = image.get_pixel(sx as u32, sy as u32);
            let inside = x < image.width() as usize && y < image.height() as usize;
            let masked = inside && mask.get_pixel(sx as u32, sy as u32)[0] > MASK_THRESHOLD;
            let index = y * width + x;
            for channel in 0..3 {
                input[channel * plane + index] = if masked { 0.0 } else { f32::from(p[channel]) / 255.0 };
            }
            input[3 * plane + index] = if masked { 1.0 } else { 0.0 };
        }
    }
    input
}

/// `i` reflected into `0..size` without repeating the edge (`F.pad(mode="reflect")`).
fn reflect(i: usize, size: usize) -> usize {
    if size == 1 {
        return 0;
    }
    let period = 2 * (size - 1);
    let offset = i % period;
    if offset < size { offset } else { period - offset }
}

impl FfcLayer {
    fn forward(&self, local: &Tensor, global: &Tensor) -> Result<(Tensor, Tensor), String> {
        let (padded_local, padded_global) = (local.reflection_pad2d(1)?, global.reflection_pad2d(1)?);
        let local_out = self.local.forward(&Tensor::cat(&[&padded_local, &padded_global], 1)?, 1)?.relu()?;
        let global_out = self.to_global.forward(&padded_local, 1)?.add(&self.spectral(global)?)?.relu()?;
        Ok((local_out, global_out))
    }

    /// `SpectralTransform` with its `FourierUnit` (no LFU): the spectrum's real and
    /// imaginary parts interleaved as channels `2c` and `2c + 1`, like torch's.
    fn spectral(&self, x: &Tensor) -> Result<Tensor, String> {
        let x = self.spectral_in.forward(x, 1)?.relu()?;
        let shape = x.shape()?;
        let (height, width) = (shape[2], shape[3]);
        let bins = width / 2 + 1;
        let spectrum =
            x.rfft2()?.view_as_real()?.permute(&[0, 1, 4, 2, 3])?.reshape(&[1, 2 * SPECTRAL, height, bins])?;
        let spectrum = self.fourier.forward(&spectrum, 1)?.relu()?;
        let back = spectrum
            .reshape(&[1, SPECTRAL, 2, height, bins])?
            .permute(&[0, 1, 3, 4, 2])?
            .contiguous()?
            .view_as_complex()?
            .irfft2(height, width)?;
        self.spectral_out.forward(&x.add(&back)?, 1)
    }
}

impl Weights {
    fn ffc_layer(&self, prefix: &str, device: Device) -> Result<FfcLayer, String> {
        let ffc = format!("{prefix}.ffc");
        let spectral = format!("{ffc}.convg2g");
        let (local_scale, local_shift) = self.batch_norm(&format!("{prefix}.bn_l"))?;
        let (global_scale, global_shift) = self.batch_norm(&format!("{prefix}.bn_g"))?;
        let scaled = |name: &str, scale: &[f32]| self.get(&format!("{ffc}.{name}.weight"))?.scale_axis(0, scale);
        // The local output reads the local then the global channels. Both global terms
        // share bn_g, whose shift goes with `convl2g`.
        let local = scaled("convl2l", &local_scale)?.concat(scaled("convg2l", &local_scale)?, 1)?;
        let batch_norm = |name: &str| format!("{spectral}.{name}");
        Ok(FfcLayer {
            local: Folded { weight: local, bias: Some(Array::vector(local_shift)) }.upload(device)?,
            to_global: Folded { weight: scaled("convl2g", &global_scale)?, bias: Some(Array::vector(global_shift)) }
                .upload(device)?,
            spectral_in: self.conv(&batch_norm("conv1.0.weight"), None, Some(&batch_norm("conv1.1")), device)?,
            fourier: self.conv(&batch_norm("fu.conv_layer.weight"), None, Some(&batch_norm("fu.bn")), device)?,
            spectral_out: Folded { weight: scaled("convg2g.conv2", &global_scale)?, bias: None }.upload(device)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn reflection_mirrors_without_the_edge() {
        let mirrored: Vec<usize> = (0..9).map(|i| reflect(i, 4)).collect();
        assert_eq!(mirrored, vec![0, 1, 2, 3, 2, 1, 0, 1, 2]);
        assert_eq!(reflect(5, 1), 0);
    }

    #[test]
    fn input_pads_by_reflection_with_the_mask_clear() {
        let image = RgbaImage::from_fn(3, 2, |x, y| image::Rgba([(x * 50) as u8, (y * 100) as u8, 255, 255]));
        let mut mask = GrayImage::new(3, 2);
        mask.put_pixel(1, 0, image::Luma([255]));
        let input = model_input(&image, &mask, 4, 3);
        let plane = 12;
        // Red: (1, 0) is masked and zeroed; its mirror at (3, 0) is padding and keeps it.
        assert_eq!(&input[..4], &[0.0, 0.0, 100.0 / 255.0, 50.0 / 255.0]);
        assert_eq!(&input[4..8], &[0.0, 50.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0]);
        // Mask: only the real masked pixel, not its mirror at (3, 0).
        assert_eq!(&input[3 * plane..3 * plane + 4], &[0.0, 1.0, 0.0, 0.0]);
    }

    /// The checkpoint's tensors with fixed pseudo-random values (BatchNorm near identity).
    fn random_checkpoint() -> Weights {
        let mut map = HashMap::new();
        let mut state = 100u64;
        let mut add = |map: &mut HashMap<String, Array>, name: String, shape: &[usize]| {
            let fan_in = shape[1..].iter().product::<usize>().max(1) as f32;
            let data = (0..shape.iter().product::<usize>())
                .map(|_| {
                    state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    ((state >> 40) as f32 / (1u64 << 24) as f32 - 0.5) / fan_in.sqrt()
                })
                .collect();
            map.insert(name, Array { shape: shape.to_vec(), data });
        };
        let batch_norm = |map: &mut HashMap<String, Array>, name: &str, channels: usize| {
            for (part, value) in [("weight", 1.0), ("bias", 0.0), ("running_mean", 0.0), ("running_var", 1.0)] {
                map.insert(format!("{name}.{part}"), Array::vector(vec![value; channels]));
            }
        };
        let (local, global, spectral) = (LOCAL as usize, GLOBAL as usize, SPECTRAL as usize);
        let convs: [(&str, &[usize]); 5] = [
            ("model.1.ffc.convl2l", &[64, 4, 7, 7]),
            ("model.2.ffc.convl2l", &[128, 64, 3, 3]),
            ("model.3.ffc.convl2l", &[256, 128, 3, 3]),
            ("model.4.ffc.convl2l", &[local, 256, 3, 3]),
            ("model.4.ffc.convl2g", &[global, 256, 3, 3]),
        ];
        for (name, shape) in convs {
            add(&mut map, format!("{name}.weight"), shape);
        }
        for (name, channels) in [("model.1.bn_l", 64), ("model.2.bn_l", 128), ("model.3.bn_l", 256)] {
            batch_norm(&mut map, name, channels);
        }
        batch_norm(&mut map, "model.4.bn_l", local);
        batch_norm(&mut map, "model.4.bn_g", global);
        for block in 0..BLOCKS {
            for layer in ["conv1", "conv2"] {
                let prefix = format!("model.{}.{layer}", 5 + block);
                let ffc = format!("{prefix}.ffc");
                add(&mut map, format!("{ffc}.convl2l.weight"), &[local, local, 3, 3]);
                add(&mut map, format!("{ffc}.convl2g.weight"), &[global, local, 3, 3]);
                add(&mut map, format!("{ffc}.convg2l.weight"), &[local, global, 3, 3]);
                add(&mut map, format!("{ffc}.convg2g.conv1.0.weight"), &[spectral, global, 1, 1]);
                add(&mut map, format!("{ffc}.convg2g.fu.conv_layer.weight"), &[2 * spectral, 2 * spectral, 1, 1]);
                add(&mut map, format!("{ffc}.convg2g.conv2.weight"), &[global, spectral, 1, 1]);
                batch_norm(&mut map, &format!("{ffc}.convg2g.conv1.1"), spectral);
                batch_norm(&mut map, &format!("{ffc}.convg2g.fu.bn"), 2 * spectral);
                batch_norm(&mut map, &format!("{prefix}.bn_l"), local);
                batch_norm(&mut map, &format!("{prefix}.bn_g"), global);
            }
        }
        for (index, channels, out) in [(24, 512, 256), (27, 256, 128), (30, 128, 64)] {
            add(&mut map, format!("model.{index}.weight"), &[channels, out, 3, 3]);
            map.insert(format!("model.{index}.bias"), Array::vector(vec![0.01; out]));
            batch_norm(&mut map, &format!("model.{}", index + 1), out);
        }
        add(&mut map, "model.34.weight".into(), &[3, 64, 7, 7]);
        map.insert("model.34.bias".into(), Array::vector(vec![0.0; 3]));
        Weights(map)
    }

    /// Needs a LibTorch runtime: `MTT_TORCH_RUNTIME=<dir> cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a LibTorch runtime in MTT_TORCH_RUNTIME"]
    fn generator_runs_on_odd_sizes_and_keeps_unmasked_pixels() {
        let runtime = std::env::var("MTT_TORCH_RUNTIME").expect("MTT_TORCH_RUNTIME");
        mtt_torch::load(runtime.as_ref()).unwrap();
        let image = RgbaImage::from_fn(21, 13, |x, y| image::Rgba([(x * 12) as u8, (y * 19) as u8, 90, 200]));
        let mut mask = GrayImage::new(21, 13);
        for (x, y) in [(5, 5), (6, 5), (12, 9)] {
            mask.put_pixel(x, y, image::Luma([255]));
        }
        mask.put_pixel(1, 1, image::Luma([MASK_THRESHOLD])); // not above the threshold
        let weights = random_checkpoint();
        let mut devices = vec![Device::Cpu];
        if mtt_torch::mps_available() {
            devices.push(Device::Mps);
        }
        let results: Vec<RgbaImage> = devices
            .into_iter()
            .map(|device| TorchLama::from_weights(&weights, device).unwrap().inpaint(&image, &mask).unwrap())
            .collect();
        for result in &results {
            assert_eq!(result.dimensions(), image.dimensions());
            for (x, y, pixel) in result.enumerate_pixels() {
                assert_eq!(pixel[3], 255);
                if mask.get_pixel(x, y)[0] <= MASK_THRESHOLD {
                    assert_eq!(pixel.0[..3], image.get_pixel(x, y).0[..3], "({x}, {y}) unchanged");
                }
            }
        }
        if let [cpu, mps] = results.as_slice() {
            let worst = cpu.pixels().zip(mps.pixels()).map(|(a, b)| (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap());
            assert!(worst.max().unwrap() <= 1, "CPU and MPS agree");
        }
    }
}
