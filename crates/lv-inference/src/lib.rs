pub mod fastembed_backend;
pub mod mlx_lm;

use lv_core::Result;
use lv_core::error::VibeError;
use std::path::PathBuf;

pub fn find_mlx_lm() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("MLX_LM_SERVER") {
        return Ok(PathBuf::from(path));
    }
    let output = std::process::Command::new("python3")
        .args(["-c", "import mlx_lm; print('ok')"])
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(PathBuf::from("python3")),
        _ => Err(VibeError::BackendUnavailable(
            "mlx-lm not found. Install with: pip install mlx-lm".into(),
        )),
    }
}
