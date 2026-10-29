//! The manga text segmentation model on LibTorch (`mtt-torch`): segmentation-models-pytorch's
//! UNet++ with SCSE attention and GroupNorm in place of the decoder's BatchNorm, over timm's
//! EfficientNetV2-RW-M encoder, as the original inference script builds it
//! (<https://huggingface.co/a-b-c-x-y-z/Manga-Text-Segmentation-2025/blob/2dde9eeb03e81692c1562059451c2bf30e1a13da/inference.py>).
//! The encoder's BatchNorm is folded into its convolutions when the weights load. It gives
//! each pixel's probability of being a glyph pixel.
//!
//! Ported from koharu (`koharu-ml/src/manga_text_mask/model.rs`;
//! https://github.com/koharu-rs/koharu), Copyright 2025-2026 Mayo Takanashi and Koharu
//! contributors, licensed under MIT OR Apache-2.0. Its comments link the
//! segmentation-models-pytorch and timm sources the network follows.

use std::path::Path;

use mtt_torch::{Device, Tensor};

use crate::torch_weights::{Conv, Weights};

/// The encoder's output stride: inputs are padded to a multiple of it.
pub const PAD_MULTIPLE: usize = 32;
/// `nn.GroupNorm`'s default.
const GROUP_NORM_EPS: f64 = 1e-5;

/// timm's EfficientNet blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    /// `EdgeResidual` (FusedMBConv): a 3×3 expansion and a 1×1 projection.
    Edge,
    /// `InvertedResidual` (MBConv): 1×1 expansion, 3×3 depthwise, squeeze-excitation, 1×1
    /// projection.
    Inverted,
}

struct Stage {
    kind: BlockKind,
    repeats: usize,
    input: usize,
    output: usize,
    /// Of the first block; the others keep the size.
    stride: i64,
    expansion: usize,
}

/// The stem's output channels (`stem_size` 24 times the 1.2 channel multiplier, rounded to 8).
const STEM: usize = 32;
/// `efficientnetv2_rw_m`: the RW-S definition with channels times 1.2 and depths times
/// 1.2 for the first four stages and 1.6 for the last two (timm's
/// `_gen_efficientnetv2_s(rw=True)`), which gives the checkpoint's block counts and widths.
const STAGES: [Stage; 6] = [
    Stage { kind: BlockKind::Edge, repeats: 3, input: STEM, output: 32, stride: 1, expansion: 1 },
    Stage { kind: BlockKind::Edge, repeats: 5, input: 32, output: 56, stride: 2, expansion: 4 },
    Stage { kind: BlockKind::Edge, repeats: 5, input: 56, output: 80, stride: 2, expansion: 4 },
    Stage { kind: BlockKind::Inverted, repeats: 8, input: 80, output: 152, stride: 2, expansion: 4 },
    Stage { kind: BlockKind::Inverted, repeats: 15, input: 152, output: 192, stride: 1, expansion: 6 },
    Stage { kind: BlockKind::Inverted, repeats: 24, input: 192, output: 328, stride: 2, expansion: 6 },
];
/// The stages whose output the decoder reads, at strides 2, 4, 8, 16 and 32 (timm's
/// `features_only` takes the last stage of each stride).
const FEATURE_STAGES: [usize; 5] = [0, 1, 2, 4, 5];
/// The squeeze-excitation width is a quarter of the block's input (not its expansion).
const SE_DIVISOR: usize = 4;
/// SCSE's channel squeeze.
const SCSE_REDUCTION: usize = 16;

/// UNet++ decoder blocks (`x_{depth}_{layer}`): input, skip and output channels for
/// decoder channels (256, 128, 64, 32, 16).
const DECODER: [(&str, usize, usize, usize); 11] = [
    ("x_0_0", 328, 192, 256),
    ("x_0_1", 256, 160, 128),
    ("x_1_1", 192, 80, 80),
    ("x_0_2", 128, 168, 64),
    ("x_1_2", 80, 112, 56),
    ("x_2_2", 80, 56, 56),
    ("x_0_3", 64, 128, 32),
    ("x_1_3", 56, 96, 32),
    ("x_2_3", 56, 64, 32),
    ("x_3_3", 56, 32, 32),
    ("x_0_4", 32, 0, 16),
];
/// The segmentation head's input: the last decoder block's output.
const HEAD_INPUT: usize = 16;

pub struct TorchSegmenter {
    device: Device,
    /// The stride-2 3×3 stem with `bn1`.
    stem: Conv,
    stages: Vec<Vec<Block>>,
    decoder: Vec<DecoderBlock>,
    /// 3×3 to one logit per pixel.
    head: Conv,
}

struct Block {
    kind: BlockKind,
    /// `conv_exp` (3×3, the block's stride) or `conv_pw` (1×1), with `bn1`.
    expand: Conv,
    /// `conv_dw` with `bn2` (inverted blocks).
    depthwise: Option<Conv>,
    /// `se.conv_reduce` and `se.conv_expand` (inverted blocks).
    squeeze: Option<(Conv, Conv)>,
    /// `conv_pwl` with the last BatchNorm.
    project: Conv,
    stride: i64,
    expanded: i64,
    skip: bool,
}

struct DecoderBlock {
    /// Over the upsampled input and the skip; absent without a skip (the checkpoint has
    /// one that is never used).
    attention1: Option<Scse>,
    conv1: ConvNorm,
    conv2: ConvNorm,
    attention2: Scse,
}

/// 3×3 convolution, GroupNorm, ReLU.
struct ConvNorm {
    conv: Conv,
    groups: i64,
    weight: Tensor,
    bias: Tensor,
}

/// Concurrent spatial and channel squeeze-excitation.
struct Scse {
    reduce: Conv,
    expand: Conv,
    spatial: Conv,
}

impl TorchSegmenter {
    /// Reads the checkpoint onto `device`. LibTorch must be loaded.
    pub fn load(path: &Path, device: Device) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("글자 분할 모델을 읽지 못했습니다 ({}): {e}", path.display()))?;
        let weights = Weights::parse(&bytes).map_err(|e| format!("글자 분할 모델을 읽지 못했습니다: {e}"))?;
        drop(bytes);
        Self::from_weights(&weights, device).map_err(|e| format!("글자 분할 모델 구조가 맞지 않습니다: {e}"))
    }

    pub fn device(&self) -> Device {
        self.device
    }

    fn from_weights(weights: &Weights, device: Device) -> Result<Self, String> {
        let encoder = "encoder.model";
        // Encoder convolutions have no bias; their BatchNorm folds in.
        let folded = |conv: &str, batch_norm: &str, shape: [usize; 4]| -> Result<Conv, String> {
            let name = format!("{encoder}.{conv}.weight");
            weights.check_shape(&name, &shape)?;
            weights.conv(&name, None, Some(&format!("{encoder}.{batch_norm}")), device)
        };
        let biased = |prefix: &str, shape: [usize; 4]| -> Result<Conv, String> {
            let (weight, bias) = (format!("{prefix}.weight"), format!("{prefix}.bias"));
            weights.check_shape(&weight, &shape)?;
            weights.check_shape(&bias, &shape[..1])?;
            weights.conv(&weight, Some(&bias), None, device)
        };
        let stem = folded("conv_stem", "bn1", [STEM, 3, 3, 3])?;
        let mut stages = Vec::with_capacity(STAGES.len());
        for (stage_index, stage) in STAGES.iter().enumerate() {
            let mut blocks = Vec::with_capacity(stage.repeats);
            for index in 0..stage.repeats {
                let at = |name: &str| format!("blocks.{stage_index}.{index}.{name}");
                let input = if index == 0 { stage.input } else { stage.output };
                let stride = if index == 0 { stage.stride } else { 1 };
                let expanded = input * stage.expansion;
                let block = match stage.kind {
                    BlockKind::Edge => Block {
                        kind: stage.kind,
                        expand: folded(&at("conv_exp"), &at("bn1"), [expanded, input, 3, 3])?,
                        depthwise: None,
                        squeeze: None,
                        project: folded(&at("conv_pwl"), &at("bn2"), [stage.output, expanded, 1, 1])?,
                        stride,
                        expanded: expanded as i64,
                        skip: stride == 1 && input == stage.output,
                    },
                    BlockKind::Inverted => {
                        let reduced = input / SE_DIVISOR;
                        Block {
                            kind: stage.kind,
                            expand: folded(&at("conv_pw"), &at("bn1"), [expanded, input, 1, 1])?,
                            depthwise: Some(folded(&at("conv_dw"), &at("bn2"), [expanded, 1, 3, 3])?),
                            squeeze: Some((
                                biased(&format!("{encoder}.{}", at("se.conv_reduce")), [reduced, expanded, 1, 1])?,
                                biased(&format!("{encoder}.{}", at("se.conv_expand")), [expanded, reduced, 1, 1])?,
                            )),
                            project: folded(&at("conv_pwl"), &at("bn3"), [stage.output, expanded, 1, 1])?,
                            stride,
                            expanded: expanded as i64,
                            skip: stride == 1 && input == stage.output,
                        }
                    }
                };
                blocks.push(block);
            }
            stages.push(blocks);
        }
        let scse = |prefix: &str, channels: usize| -> Result<Scse, String> {
            let reduced = channels / SCSE_REDUCTION;
            Ok(Scse {
                reduce: biased(&format!("{prefix}.cSE.1"), [reduced, channels, 1, 1])?,
                expand: biased(&format!("{prefix}.cSE.3"), [channels, reduced, 1, 1])?,
                spatial: biased(&format!("{prefix}.sSE.0"), [1, channels, 1, 1])?,
            })
        };
        let conv_norm = |prefix: &str, input: usize, output: usize| -> Result<ConvNorm, String> {
            let weight = format!("{prefix}.0.weight");
            weights.check_shape(&weight, &[output, input, 3, 3])?;
            let affine = |name: &str| {
                let name = format!("{prefix}.1.{name}");
                weights.check_shape(&name, &[output])?;
                weights.get(&name)?.tensor(device)
            };
            Ok(ConvNorm {
                conv: weights.conv(&weight, None, None, device)?,
                groups: group_count(output) as i64,
                weight: affine("weight")?,
                bias: affine("bias")?,
            })
        };
        let decoder = DECODER
            .iter()
            .map(|&(name, input, skip, output)| {
                let prefix = format!("decoder.blocks.{name}");
                let combined = input + skip;
                Ok(DecoderBlock {
                    attention1: (skip > 0)
                        .then(|| scse(&format!("{prefix}.attention1.attention"), combined))
                        .transpose()?,
                    conv1: conv_norm(&format!("{prefix}.conv1"), combined, output)?,
                    conv2: conv_norm(&format!("{prefix}.conv2"), output, output)?,
                    attention2: scse(&format!("{prefix}.attention2.attention"), output)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { device, stem, stages, decoder, head: biased("segmentation_head.0", [1, HEAD_INPUT, 3, 3])? })
    }

    /// Glyph probabilities of a normalized `(1, 3, H, W)` input with `H` and `W` multiples
    /// of `PAD_MULTIPLE`, as `H * W` values.
    pub fn probabilities(&self, input: &[f32], height: usize, width: usize) -> Result<Vec<f32>, String> {
        if !height.is_multiple_of(PAD_MULTIPLE) || !width.is_multiple_of(PAD_MULTIPLE) {
            return Err(format!("{width}×{height}은 {PAD_MULTIPLE}의 배수가 아닙니다"));
        }
        let input = Tensor::from_f32(input, &[1, 3, height as i64, width as i64])?.to(self.device)?;
        self.forward(&input)?.sigmoid()?.to_f32_vec()
    }

    /// The logits, `(1, 1, H, W)`.
    fn forward(&self, input: &Tensor) -> Result<Tensor, String> {
        let [f0, f1, f2, f4, f5] = self.encode(input)?;
        let cat = |tensors: &[&Tensor]| Tensor::cat(tensors, 1);
        let block = |name: &str| &self.decoder[DECODER.iter().position(|entry| entry.0 == name).expect("block")];
        // segmentation-models-pytorch's dense skip pathways, deepest first.
        let x_0_0 = block("x_0_0").forward(&f5, Some(&f4))?;
        let x_1_1 = block("x_1_1").forward(&f4, Some(&f2))?;
        let x_2_2 = block("x_2_2").forward(&f2, Some(&f1))?;
        let x_3_3 = block("x_3_3").forward(&f1, Some(&f0))?;
        let x_0_1 = block("x_0_1").forward(&x_0_0, Some(&cat(&[&x_1_1, &f2])?))?;
        let x_1_2 = block("x_1_2").forward(&x_1_1, Some(&cat(&[&x_2_2, &f1])?))?;
        let x_2_3 = block("x_2_3").forward(&x_2_2, Some(&cat(&[&x_3_3, &f0])?))?;
        let x_0_2 = block("x_0_2").forward(&x_0_1, Some(&cat(&[&x_1_2, &x_2_2, &f1])?))?;
        let x_1_3 = block("x_1_3").forward(&x_1_2, Some(&cat(&[&x_2_3, &x_3_3, &f0])?))?;
        let x_0_3 = block("x_0_3").forward(&x_0_2, Some(&cat(&[&x_1_3, &x_2_3, &x_3_3, &f0])?))?;
        let x_0_4 = block("x_0_4").forward(&x_0_3, None)?;
        self.head.forward_padded(&x_0_4, 1, 1, 1)
    }

    /// The encoder's features at strides 2, 4, 8, 16 and 32.
    fn encode(&self, input: &Tensor) -> Result<[Tensor; 5], String> {
        let mut x = self.stem.forward_padded(input, 2, 1, 1)?.silu()?;
        let mut features = Vec::with_capacity(FEATURE_STAGES.len());
        for (index, stage) in self.stages.iter().enumerate() {
            for block in stage {
                x = block.forward(&x)?;
            }
            if FEATURE_STAGES.contains(&index) {
                features.push(x.shallow_clone()?);
            }
        }
        features.try_into().map_err(|_| "인코더 특징 수가 맞지 않습니다".to_owned())
    }
}

impl Block {
    fn forward(&self, input: &Tensor) -> Result<Tensor, String> {
        let x = match self.kind {
            BlockKind::Edge => self.expand.forward_padded(input, self.stride, 1, 1)?.silu()?,
            BlockKind::Inverted => {
                let x = self.expand.forward_padded(input, 1, 0, 1)?.silu()?;
                let depthwise = self.depthwise.as_ref().ok_or("conv_dw 없음")?;
                let x = depthwise.forward_padded(&x, self.stride, 1, self.expanded)?.silu()?;
                let (reduce, expand) = self.squeeze.as_ref().ok_or("se 없음")?;
                let scale = reduce.forward_padded(&x.mean_keepdim(&[2, 3])?, 1, 0, 1)?.silu()?;
                x.mul(&expand.forward_padded(&scale, 1, 0, 1)?.sigmoid()?)?
            }
        };
        let x = self.project.forward_padded(&x, 1, 0, 1)?;
        if self.skip { x.add(input) } else { Ok(x) }
    }
}

impl DecoderBlock {
    fn forward(&self, input: &Tensor, skip: Option<&Tensor>) -> Result<Tensor, String> {
        let shape = input.shape()?;
        let mut x = input.upsample_nearest2d(shape[2] * 2, shape[3] * 2)?;
        if let (Some(skip), Some(attention)) = (skip, &self.attention1) {
            x = attention.forward(&Tensor::cat(&[&x, skip], 1)?)?;
        }
        let x = self.conv2.forward(&self.conv1.forward(&x)?)?;
        self.attention2.forward(&x)
    }
}

impl ConvNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor, String> {
        self.conv.forward_padded(x, 1, 1, 1)?.group_norm(self.groups, &self.weight, &self.bias, GROUP_NORM_EPS)?.relu()
    }
}

impl Scse {
    /// `x * cSE(x) + x * sSE(x)`, as `x * (cSE(x) + sSE(x))`.
    fn forward(&self, x: &Tensor) -> Result<Tensor, String> {
        let channel = self.reduce.forward_padded(&x.mean_keepdim(&[2, 3])?, 1, 0, 1)?.relu()?;
        let channel = self.expand.forward_padded(&channel, 1, 0, 1)?.sigmoid()?;
        let spatial = self.spatial.forward_padded(x, 1, 0, 1)?.sigmoid()?;
        x.mul(&channel.add(&spatial)?)
    }
}

/// The script's `convert_batchnorm_to_groupnorm`: 8 groups, or the largest count below 8
/// that divides the channels.
fn group_count(channels: usize) -> usize {
    if channels >= 8 && channels.is_multiple_of(8) {
        return 8;
    }
    (2..=channels.min(8)).rev().find(|groups| channels.is_multiple_of(*groups)).unwrap_or(1)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::torch_weights::Array;

    /// Every tensor the checkpoint has (BatchNorm counters aside) with its shape.
    pub(crate) fn checkpoint_shapes() -> Vec<(String, Vec<usize>)> {
        let mut shapes = Vec::new();
        let mut add = |name: String, shape: &[usize]| shapes.push((name, shape.to_vec()));
        let batch_norm = |add: &mut dyn FnMut(String, &[usize]), name: String, channels: usize| {
            for part in ["weight", "bias", "running_mean", "running_var"] {
                add(format!("{name}.{part}"), &[channels]);
            }
        };
        let biased = |add: &mut dyn FnMut(String, &[usize]), name: String, shape: [usize; 4]| {
            add(format!("{name}.weight"), &shape);
            add(format!("{name}.bias"), &shape[..1]);
        };
        let encoder = "encoder.model";
        add(format!("{encoder}.conv_stem.weight"), &[STEM, 3, 3, 3]);
        batch_norm(&mut add, format!("{encoder}.bn1"), STEM);
        for (stage_index, stage) in STAGES.iter().enumerate() {
            for index in 0..stage.repeats {
                let at = |name: &str| format!("{encoder}.blocks.{stage_index}.{index}.{name}");
                let input = if index == 0 { stage.input } else { stage.output };
                let expanded = input * stage.expansion;
                match stage.kind {
                    BlockKind::Edge => {
                        add(at("conv_exp.weight"), &[expanded, input, 3, 3]);
                        batch_norm(&mut add, at("bn1"), expanded);
                        add(at("conv_pwl.weight"), &[stage.output, expanded, 1, 1]);
                        batch_norm(&mut add, at("bn2"), stage.output);
                    }
                    BlockKind::Inverted => {
                        let reduced = input / SE_DIVISOR;
                        add(at("conv_pw.weight"), &[expanded, input, 1, 1]);
                        batch_norm(&mut add, at("bn1"), expanded);
                        add(at("conv_dw.weight"), &[expanded, 1, 3, 3]);
                        batch_norm(&mut add, at("bn2"), expanded);
                        biased(&mut add, at("se.conv_reduce"), [reduced, expanded, 1, 1]);
                        biased(&mut add, at("se.conv_expand"), [expanded, reduced, 1, 1]);
                        add(at("conv_pwl.weight"), &[stage.output, expanded, 1, 1]);
                        batch_norm(&mut add, at("bn3"), stage.output);
                    }
                }
            }
        }
        for (name, input, skip, output) in DECODER {
            let prefix = format!("decoder.blocks.{name}");
            for (attention, channels) in [("attention1", input + skip), ("attention2", output)] {
                let at = format!("{prefix}.{attention}.attention");
                let reduced = channels / SCSE_REDUCTION;
                biased(&mut add, format!("{at}.cSE.1"), [reduced, channels, 1, 1]);
                biased(&mut add, format!("{at}.cSE.3"), [channels, reduced, 1, 1]);
                biased(&mut add, format!("{at}.sSE.0"), [1, channels, 1, 1]);
            }
            for (conv, channels) in [("conv1", input + skip), ("conv2", output)] {
                add(format!("{prefix}.{conv}.0.weight"), &[output, channels, 3, 3]);
                add(format!("{prefix}.{conv}.1.weight"), &[output]);
                add(format!("{prefix}.{conv}.1.bias"), &[output]);
            }
        }
        biased(&mut add, "segmentation_head.0".into(), [1, HEAD_INPUT, 3, 3]);
        shapes
    }

    /// The checkpoint's tensors with fixed pseudo-random values (normalizations near
    /// identity).
    fn random_checkpoint() -> Weights {
        let mut state = 7u64;
        let mut random = move || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 40) as f32 / (1u64 << 24) as f32 - 0.5
        };
        let map: HashMap<String, Array> = checkpoint_shapes()
            .into_iter()
            .map(|(name, shape)| {
                let count = shape.iter().product::<usize>();
                let fan_in = shape[1..].iter().product::<usize>().max(1) as f32;
                // Norm scales and variances 1, shifts, means and biases small, convolution
                // weights within ±1/√fan-in.
                let data = if shape.len() == 1 && (name.ends_with(".weight") || name.ends_with("running_var")) {
                    vec![1.0; count]
                } else if shape.len() == 1 {
                    (0..count).map(|_| random() * 0.1).collect()
                } else {
                    (0..count).map(|_| random() * 2.0 / fan_in.sqrt()).collect()
                };
                (name, Array { shape, data })
            })
            .collect();
        Weights(map)
    }

    #[test]
    fn shapes_match_the_published_checkpoint() {
        let shapes: HashMap<String, Vec<usize>> = checkpoint_shapes().into_iter().collect();
        // The checkpoint has 1228 float tensors (and 168 BatchNorm counters).
        assert_eq!(shapes.len(), 1228);
        // koharu's model test and the safetensors header.
        assert_eq!(shapes["encoder.model.blocks.5.23.se.conv_reduce.weight"], [82, 1968, 1, 1]);
        assert_eq!(shapes["encoder.model.blocks.3.0.conv_dw.weight"], [320, 1, 3, 3]);
        assert_eq!(shapes["encoder.model.blocks.3.0.se.conv_expand.weight"], [320, 20, 1, 1]);
        assert_eq!(shapes["decoder.blocks.x_0_0.conv1.0.weight"], [256, 520, 3, 3]);
        assert_eq!(shapes["decoder.blocks.x_0_4.attention1.attention.cSE.1.weight"], [2, 32, 1, 1]);
        assert_eq!(shapes["decoder.blocks.x_0_4.attention2.attention.cSE.1.weight"], [1, 16, 1, 1]);
        assert_eq!(shapes["segmentation_head.0.weight"], [1, 16, 3, 3]);
    }

    #[test]
    fn group_norm_groups_follow_the_script() {
        assert_eq!(group_count(256), 8);
        assert_eq!(group_count(16), 8);
        assert_eq!(group_count(12), 6);
        assert_eq!(group_count(7), 7);
        assert_eq!(group_count(11), 1);
    }

    /// Needs a LibTorch runtime: `MTT_TORCH_RUNTIME=<dir> cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a LibTorch runtime in MTT_TORCH_RUNTIME"]
    fn network_runs_and_cpu_and_mps_agree() {
        let runtime = std::env::var("MTT_TORCH_RUNTIME").expect("MTT_TORCH_RUNTIME");
        mtt_torch::load(runtime.as_ref()).unwrap();
        let weights = random_checkpoint();
        let (width, height) = (96, 64);
        let input: Vec<f32> = (0..3 * width * height).map(|i| ((i * 37 % 101) as f32 / 50.0) - 1.0).collect();
        let mut devices = vec![Device::Cpu];
        if mtt_torch::mps_available() {
            devices.push(Device::Mps);
        }
        let results: Vec<Vec<f32>> = devices
            .into_iter()
            .map(|device| {
                TorchSegmenter::from_weights(&weights, device).unwrap().probabilities(&input, height, width).unwrap()
            })
            .collect();
        for result in &results {
            assert_eq!(result.len(), width * height);
            assert!(result.iter().all(|p| (0.0..=1.0).contains(p)));
        }
        if let [cpu, mps] = results.as_slice() {
            let worst = cpu.iter().zip(mps).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            assert!(worst < 1e-3, "CPU and MPS agree ({worst})");
        }
        let model = TorchSegmenter::from_weights(&weights, Device::Cpu).unwrap();
        assert!(model.probabilities(&input[..3 * 50 * 64], 64, 50).is_err(), "not a multiple of 32");
    }

    /// The published weights against koharu's reference values from the Python model:
    /// `MTT_TORCH_RUNTIME=<dir> MTT_SEGMENTATION_WEIGHTS=<model.safetensors> cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a LibTorch runtime and the published weights"]
    fn published_weights_match_the_reference() {
        let runtime = std::env::var("MTT_TORCH_RUNTIME").expect("MTT_TORCH_RUNTIME");
        let path = std::env::var("MTT_SEGMENTATION_WEIGHTS").expect("MTT_SEGMENTATION_WEIGHTS");
        mtt_torch::load(runtime.as_ref()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let weights = Weights::parse(&bytes).unwrap();
        let expected: HashMap<String, Vec<usize>> = checkpoint_shapes().into_iter().collect();
        let actual: HashMap<String, Vec<usize>> =
            weights.0.iter().map(|(name, array)| (name.clone(), array.shape.clone())).collect();
        assert_eq!(actual, expected);
        // koharu's checkpoint test image.
        let image = image::RgbImage::from_fn(96, 64, |x, y| {
            image::Rgb([((x * 7 + y * 3) % 256) as u8, ((x * 5 + y * 11) % 256) as u8, ((x * 13 + y * 2) % 256) as u8])
        });
        let model = TorchSegmenter::from_weights(&weights, Device::Cpu).unwrap();
        let (input, padded_width, padded_height) = crate::text_segmentation::model_input(&image);
        let padded = model.probabilities(&input, padded_height, padded_width).unwrap();
        let probabilities: Vec<f32> =
            (0..64).flat_map(|y| padded[y * padded_width..][..96].iter().copied()).collect::<Vec<_>>();
        let reference = [
            0.007_002_114_3,
            0.020_354_025,
            0.010_177_039,
            0.000_451_958_16,
            0.000_130_330_62,
            0.000_302_413_57,
            0.000_051_601_994,
            0.006_196_571_5,
        ];
        for (&index, expected) in [0, 1, 95, 96, 777, 2048, 4095, 6143].iter().zip(reference) {
            let actual = probabilities[index];
            assert!((actual - expected).abs() < 1e-5, "probability {index}: {actual}, expected {expected}");
        }
        let sum: f32 = probabilities.iter().sum();
        assert!((sum - 1.352_233).abs() < 1e-3, "sum {sum}");
    }
}
