//! The SigLIP 2 image encoder, run with ONNX Runtime.
//!
//! ONNX Runtime is loaded from a file at run time (`ort`'s `load-dynamic`),
//! once per process. A session tries the GPU first (DirectML on Windows,
//! CoreML on macOS) and falls back to the CPU, using half the cores so the
//! rest of the app stays responsive.
//!
//! Preprocessing follows the model's `preprocessor_config.json`: RGB,
//! squashed to 224×224 (bilinear), scaled to 0–1, then `(x − 0.5) / 0.5`.

use std::fmt::Display;
use std::path::Path;
use std::sync::OnceLock;

use fast_image_resize::{
    images::Image as FirImage, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
};
use image::DynamicImage;
use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use serde::Serialize;

use super::embedding::Embedding;
use super::AiError;

/// The model's input size.
pub const SIZE: u32 = 224;

/// Where inference runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Accel {
    DirectMl,
    CoreMl,
    Cpu,
}

impl Accel {
    pub fn describe(self) -> &'static str {
        match self {
            Accel::DirectMl => "graphics card (DirectML)",
            Accel::CoreMl => "Apple Neural Engine / GPU (CoreML)",
            Accel::Cpu => "processor (CPU)",
        }
    }
}

fn err(e: impl Display) -> AiError {
    AiError::Inference(e.to_string())
}

static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();

/// Load ONNX Runtime from `runtime` (first call only; later calls reuse it,
/// whatever path they pass).
fn environment(runtime: &Path) -> Result<Environment, AiError> {
    let env = ENV.get_or_init(|| {
        let b = ort::init_from(runtime).map_err(|e| format!("{}: {e}", runtime.display()))?;
        b.with_name("keepsake").build().map_err(|e| e.to_string())
    });
    env.clone().map_err(AiError::Runtime)
}

/// A loaded image encoder. Not shared between threads: each worker loads
/// its own (the AI worker is one thread).
pub struct ImageModel {
    session: Session,
    accel: Accel,
}

impl std::fmt::Debug for ImageModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageModel")
            .field("accel", &self.accel)
            .finish()
    }
}

fn cpu_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
}

fn cpu_session(env: &Environment, model: &Path) -> Result<Session, AiError> {
    Session::builder(env)
        .map_err(err)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(err)?
        .with_intra_threads(cpu_threads())
        .map_err(err)?
        .commit_from_file(model)
        .map_err(err)
}

#[cfg(windows)]
fn gpu_session(env: &Environment, model: &Path) -> Result<(Session, Accel), AiError> {
    // DirectML needs memory patterns and parallel execution off.
    let s = Session::builder(env)
        .map_err(err)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(err)?
        .with_memory_pattern(false)
        .map_err(err)?
        .with_parallel_execution(false)
        .map_err(err)?
        .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
        .map_err(err)?
        .commit_from_file(model)
        .map_err(err)?;
    Ok((s, Accel::DirectMl))
}

#[cfg(target_os = "macos")]
fn gpu_session(env: &Environment, model: &Path) -> Result<(Session, Accel), AiError> {
    let s = Session::builder(env)
        .map_err(err)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(err)?
        .with_execution_providers([ort::ep::CoreML::default().build().error_on_failure()])
        .map_err(err)?
        .commit_from_file(model)
        .map_err(err)?;
    Ok((s, Accel::CoreMl))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn gpu_session(_env: &Environment, _model: &Path) -> Result<(Session, Accel), AiError> {
    Err(AiError::Inference("no GPU support on this platform".into()))
}

/// A session for the model at `model`, with ONNX Runtime from `runtime`:
/// on the graphics card if `gpu` and it works, else on the CPU. Shared by
/// the image encoder and the face models (ADR-040).
pub(crate) fn open_session(
    model: &Path,
    runtime: &Path,
    gpu: bool,
) -> Result<(Session, Accel), AiError> {
    if !model.is_file() {
        return Err(AiError::ModelMissing(model.to_path_buf()));
    }
    if !runtime.is_file() {
        return Err(AiError::Runtime(format!("{} not found", runtime.display())));
    }
    let env = environment(runtime)?;
    if gpu {
        match gpu_session(&env, model) {
            Ok(s) => return Ok(s),
            Err(e) => tracing::warn!("AI: GPU not used, falling back to the CPU: {e}"),
        }
    }
    Ok((cpu_session(&env, model)?, Accel::Cpu))
}

/// ONNX Runtime from the app's own folders, or why it is missing.
pub(crate) fn installed_runtime() -> Result<std::path::PathBuf, AiError> {
    super::install::find_runtime().ok_or_else(|| {
        AiError::Runtime(format!(
            "{} not found in vendor/onnxruntime",
            super::install::runtime_file_name()
        ))
    })
}

/// An inference error from ONNX Runtime.
pub(crate) fn ort_err(e: impl Display) -> AiError {
    err(e)
}

impl ImageModel {
    /// Load the model at `model` with ONNX Runtime from `runtime`.
    /// `gpu`: try the graphics card first.
    pub fn load(model: &Path, runtime: &Path, gpu: bool) -> Result<Self, AiError> {
        let (session, accel) = open_session(model, runtime, gpu)?;
        Ok(Self { session, accel })
    }

    /// Load the installed model and runtime ([`super::install`]).
    pub fn load_installed(gpu: bool) -> Result<Self, AiError> {
        let model = super::install::find_model().ok_or_else(|| {
            AiError::ModelMissing(super::install::user_model_path().unwrap_or_default())
        })?;
        Self::load(&model, &installed_runtime()?, gpu)
    }

    pub fn accel(&self) -> Accel {
        self.accel
    }

    /// The image's unit embedding.
    pub fn embed(&mut self, img: &DynamicImage) -> Result<Embedding, AiError> {
        let pixels = preprocess(img)?;
        let s = SIZE as usize;
        let input = Tensor::from_array(([1usize, 3, s, s], pixels)).map_err(err)?;
        let outputs = self
            .session
            .run(ort::inputs!["pixel_values" => input])
            .map_err(err)?;
        let out = outputs
            .get("pooler_output")
            .ok_or_else(|| AiError::Inference("the model has no pooler_output".into()))?;
        let (_, values) = out.try_extract_tensor::<f32>().map_err(err)?;
        Embedding::new(values.to_vec())
    }
}

/// The model's input: CHW floats, 3 × 224 × 224.
pub fn preprocess(img: &DynamicImage) -> Result<Vec<f32>, AiError> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    if w == 0 || h == 0 {
        return Err(AiError::Image("empty image".into()));
    }
    let src = FirImage::from_vec_u8(w, h, rgb.into_raw(), PixelType::U8x3)
        .map_err(|e| AiError::Image(e.to_string()))?;
    let mut dst = FirImage::new(SIZE, SIZE, PixelType::U8x3);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
        )
        .map_err(|e| AiError::Image(e.to_string()))?;
    let px = dst.buffer();
    let plane = (SIZE * SIZE) as usize;
    let mut out = vec![0f32; plane * 3];
    for (i, p) in px.as_chunks::<3>().0.iter().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = (f32::from(p[c]) / 255.0 - 0.5) / 0.5;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn preprocess_makes_normalised_chw() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(500, 300, Rgb([255, 0, 128])));
        let v = preprocess(&img).unwrap();
        let plane = (SIZE * SIZE) as usize;
        assert_eq!(v.len(), plane * 3);
        assert!((v[0] - 1.0).abs() < 1e-6); // red 255 → 1
        assert!((v[plane] + 1.0).abs() < 1e-6); // green 0 → −1
        assert!(v[2 * plane].abs() < 0.01); // blue 128 → ~0
    }

    #[test]
    fn missing_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path().join("none.onnx");
        assert!(matches!(
            ImageModel::load(&m, &m, false),
            Err(AiError::ModelMissing(_))
        ));
    }
}
