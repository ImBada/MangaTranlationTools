//! Checkpoints for the LibTorch models (`lama_torch.rs`, `text_segmentation_torch.rs`):
//! safetensors read on the CPU, BatchNorm folded into the convolutions before them, then
//! uploaded to the device.

use std::collections::HashMap;

use mtt_torch::{Device, Tensor};

/// PyTorch's `BatchNorm2d` default.
pub(crate) const BN_EPS: f32 = 1e-5;

/// A convolution's weight and bias on the device.
pub(crate) struct Conv {
    pub(crate) weight: Tensor,
    pub(crate) bias: Option<Tensor>,
}

impl Conv {
    /// No padding (the caller pads), no groups.
    pub(crate) fn forward(&self, x: &Tensor, stride: i64) -> Result<Tensor, String> {
        x.conv2d(&self.weight, self.bias.as_ref(), stride, 0)
    }

    /// Zero `padding` on every side, in `groups` groups.
    pub(crate) fn forward_padded(&self, x: &Tensor, stride: i64, padding: i64, groups: i64) -> Result<Tensor, String> {
        x.conv2d_grouped(&self.weight, self.bias.as_ref(), stride, padding, groups)
    }
}

/// A checkpoint tensor on the CPU.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Array {
    pub(crate) shape: Vec<usize>,
    pub(crate) data: Vec<f32>,
}

impl Array {
    pub(crate) fn vector(data: Vec<f32>) -> Self {
        Self { shape: vec![data.len()], data }
    }

    /// Each entry along `axis` multiplied by `scale[index]`.
    pub(crate) fn scale_axis(mut self, axis: usize, scale: &[f32]) -> Result<Self, String> {
        if self.shape.get(axis) != Some(&scale.len()) {
            return Err(format!("{:?} 축 {axis}에 {}개 배율", self.shape, scale.len()));
        }
        let inner: usize = self.shape[axis + 1..].iter().product();
        for (index, value) in self.data.iter_mut().enumerate() {
            *value *= scale[(index / inner) % scale.len()];
        }
        Ok(self)
    }

    /// `self` and `other` joined along `axis`.
    pub(crate) fn concat(self, other: Self, axis: usize) -> Result<Self, String> {
        let same_rest =
            |a: &[usize], b: &[usize]| a.len() == b.len() && (0..a.len()).all(|i| i == axis || a[i] == b[i]);
        if !same_rest(&self.shape, &other.shape) {
            return Err(format!("{:?}와 {:?}를 축 {axis}로 붙일 수 없습니다", self.shape, other.shape));
        }
        let outer: usize = self.shape[..axis].iter().product();
        let (a, b) = (self.data.len() / outer, other.data.len() / outer);
        let data =
            (0..outer).flat_map(|o| self.data[o * a..][..a].iter().chain(&other.data[o * b..][..b])).copied().collect();
        let mut shape = self.shape;
        shape[axis] += other.shape[axis];
        Ok(Self { shape, data })
    }

    pub(crate) fn tensor(&self, device: Device) -> Result<Tensor, String> {
        let shape: Vec<i64> = self.shape.iter().map(|&side| side as i64).collect();
        Tensor::from_f32(&self.data, &shape)?.to(device)
    }
}

/// A convolution's folded weight and bias before they go to the device.
pub(crate) struct Folded {
    pub(crate) weight: Array,
    pub(crate) bias: Option<Array>,
}

impl Folded {
    pub(crate) fn concat(self, other: Self, axis: usize) -> Result<Self, String> {
        let bias = match (self.bias, other.bias) {
            (Some(a), Some(b)) => Some(a.concat(b, 0)?),
            (None, None) => None,
            _ => return Err("편향이 한쪽에만 있습니다".into()),
        };
        Ok(Self { weight: self.weight.concat(other.weight, axis)?, bias })
    }

    pub(crate) fn upload(self, device: Device) -> Result<Conv, String> {
        Ok(Conv { weight: self.weight.tensor(device)?, bias: self.bias.map(|bias| bias.tensor(device)).transpose()? })
    }
}

pub(crate) struct Weights(pub(crate) HashMap<String, Array>);

impl Weights {
    /// The float32 tensors of a safetensors file (BatchNorm's `num_batches_tracked`
    /// counters are integers, and unused).
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let file = safetensors::SafeTensors::deserialize(bytes).map_err(|e| e.to_string())?;
        let mut tensors = HashMap::new();
        for (name, view) in file.tensors() {
            if view.dtype() != safetensors::Dtype::F32 {
                if name.ends_with(".num_batches_tracked") {
                    continue;
                }
                return Err(format!("{name}: {:?}", view.dtype()));
            }
            let data = view.data().as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();
            tensors.insert(name, Array { shape: view.shape().to_vec(), data });
        }
        Ok(Self(tensors))
    }

    pub(crate) fn get(&self, name: &str) -> Result<Array, String> {
        self.0.get(name).cloned().ok_or_else(|| format!("{name} 없음"))
    }

    /// Whether `name` is there with `shape`.
    pub(crate) fn check_shape(&self, name: &str, shape: &[usize]) -> Result<(), String> {
        let array = self.0.get(name).ok_or_else(|| format!("{name} 없음"))?;
        if array.shape != shape {
            return Err(format!("{name}: {:?}, {shape:?}여야 합니다", array.shape));
        }
        Ok(())
    }

    /// BatchNorm `prefix` as a per-channel scale and shift.
    pub(crate) fn batch_norm(&self, prefix: &str) -> Result<(Vec<f32>, Vec<f32>), String> {
        let part = |name: &str| self.get(&format!("{prefix}.{name}")).map(|array| array.data);
        let (gamma, beta, mean, variance) =
            (part("weight")?, part("bias")?, part("running_mean")?, part("running_var")?);
        let scale: Vec<f32> = gamma.iter().zip(&variance).map(|(g, v)| g / (v + BN_EPS).sqrt()).collect();
        let shift = beta.iter().zip(&mean).zip(&scale).map(|((b, m), s)| b - m * s).collect();
        Ok((scale, shift))
    }

    /// Convolution `weight` (output channels along `out_axis`) with its own `bias` and
    /// then BatchNorm `batch_norm` folded in.
    pub(crate) fn folded(
        &self,
        weight: &str,
        bias: Option<&str>,
        batch_norm: Option<&str>,
        out_axis: usize,
    ) -> Result<Folded, String> {
        let weight = self.get(weight)?;
        let bias = bias.map(|name| self.get(name)).transpose()?;
        let Some(batch_norm) = batch_norm else { return Ok(Folded { weight, bias }) };
        let (scale, shift) = self.batch_norm(batch_norm)?;
        let bias = match bias {
            Some(bias) => bias.data.iter().zip(&scale).zip(&shift).map(|((b, s), t)| b * s + t).collect(),
            None => shift,
        };
        Ok(Folded { weight: weight.scale_axis(out_axis, &scale)?, bias: Some(Array::vector(bias)) })
    }

    pub(crate) fn conv(
        &self,
        weight: &str,
        bias: Option<&str>,
        batch_norm: Option<&str>,
        device: Device,
    ) -> Result<Conv, String> {
        self.folded(weight, bias, batch_norm, 0)?.upload(device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_norm_folds_into_the_output_channels() {
        let mut map = HashMap::new();
        let array = |shape: Vec<usize>, data: Vec<f32>| Array { shape, data };
        map.insert("conv.weight".into(), array(vec![2, 1, 1, 2], vec![1.0, 2.0, 3.0, 4.0]));
        map.insert("conv.bias".into(), array(vec![2], vec![1.0, -1.0]));
        map.insert("bn.weight".into(), array(vec![2], vec![2.0, 1.0]));
        map.insert("bn.bias".into(), array(vec![2], vec![0.5, 0.0]));
        map.insert("bn.running_mean".into(), array(vec![2], vec![1.0, 2.0]));
        map.insert("bn.running_var".into(), array(vec![2], vec![1.0 - BN_EPS, 4.0 - BN_EPS]));
        let weights = Weights(map);
        let folded = weights.folded("conv.weight", Some("conv.bias"), Some("bn"), 0).unwrap();
        // scale (2, 0.5), shift (0.5 − 2, 0 − 1).
        assert_eq!(folded.weight.data, vec![2.0, 4.0, 1.5, 2.0]);
        assert_eq!(folded.bias.unwrap().data, vec![1.0 * 2.0 - 1.5, -0.5 - 1.0]);
        assert!(weights.check_shape("conv.weight", &[2, 1, 1, 2]).is_ok());
        assert!(weights.check_shape("conv.weight", &[2, 1, 2, 1]).is_err());
        assert!(weights.check_shape("other.weight", &[2]).is_err());
    }

    #[test]
    fn arrays_concatenate_along_an_axis() {
        let a = Array { shape: vec![2, 1], data: vec![1.0, 2.0] };
        let b = Array { shape: vec![2, 2], data: vec![3.0, 4.0, 5.0, 6.0] };
        assert_eq!(a.concat(b, 1).unwrap(), Array { shape: vec![2, 3], data: vec![1.0, 3.0, 4.0, 2.0, 5.0, 6.0] });
    }
}
