//! `cargo run -p mtt-torch --example smoke -- RUNTIME_DIR`: loads the runtime and checks
//! a convolution and an FFT round trip on the CPU and, when present, MPS.

use mtt_torch::{Device, Tensor};

fn main() -> Result<(), String> {
    let directory = std::env::args().nth(1).ok_or("RUNTIME_DIR")?;
    let started = std::time::Instant::now();
    mtt_torch::load(directory.as_ref())?;
    println!("loaded in {:.2}s, mps {}", started.elapsed().as_secs_f32(), mtt_torch::mps_available());
    let x: Vec<f32> = (0..2 * 6 * 8).map(|i| ((i * 7) % 11) as f32 - 5.0).collect();
    let w: Vec<f32> = (0..3 * 2 * 9).map(|i| ((i * 5) % 7) as f32 / 7.0 - 0.4).collect();
    let mut devices = vec![Device::Cpu];
    if mtt_torch::mps_available() {
        devices.push(Device::Mps);
    }
    for device in devices {
        let x = Tensor::from_f32(&x, &[1, 2, 6, 8])?.to(device)?;
        let w = Tensor::from_f32(&w, &[3, 2, 3, 3])?.to(device)?;
        let y = x.reflection_pad2d(1)?.conv2d(&w, None, 1, 0)?.relu()?;
        let back = x.rfft2()?.irfft2(6, 8)?;
        let error = back.to_f32_vec()?.iter().zip(&x.to_f32_vec()?).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        let y = y.to_f32_vec()?;
        println!("{device:?}: conv sum {:.4}, fft round trip error {error:e}", y.iter().sum::<f32>());
    }
    let bad = Tensor::from_f32(&[1.0; 4], &[2, 2])?.reshape(&[3]);
    println!("error path: {bad:?}");
    Ok(())
}
