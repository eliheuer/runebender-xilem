// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Detached, bounded invocation of the installed sketch2glyph inference script.
//!
//! The script's tracer fits a caller-declared ink box; it does not implement C2's exact
//! full-image placement. This adapter verifies that box and derives explicit integer CLI metrics.
//! Generated GLIF points are decoded directly into editable contours without rasterization.

use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::process::{self, ProcessCancellation, ProcessLimits, ProcessOutcome};
use crate::formats::image_trace::TraceCalibration;
use crate::outline::drawing::{DrawingContour, DrawingPoint, DrawingPointType};

const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
const MAX_PIXELS: u64 = 262_144;
const MAX_EDGE: u32 = 1_024;
const MAX_GLYPH_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUNTIME_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MODEL_FILES: &[&str] = &["config.json", "vocab.txt", "weights.safetensors"];
const MODULE_FILES: &[&str] = &[
    "sketch2glyph.py",
    "sketch_sim.py",
    "tokenizer.py",
    "generate.py",
    "model.py",
    "conform.py",
];
static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(1);

/// Installed runtime and one concrete checkpoint, without following a run-directory alias.
#[derive(Clone, Debug)]
pub struct SketchRuntime {
    /// Root containing the installed `glyphlab` Python package.
    pub repository: PathBuf,
    /// Python executable from the installed environment.
    pub python: PathBuf,
    /// Concrete run directory containing config, vocabulary and weights.
    pub checkpoint: PathBuf,
    /// Home directory whose `.cargo/bin/img2bez` is invoked by the Python tracer.
    pub script_home: PathBuf,
}

/// Byte identities checked before and after inference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SketchRuntimeIdentity {
    /// Invoked Python entrypoint, retaining the virtualenv's symlink name.
    pub python_path: PathBuf,
    /// Resolved target of that entrypoint, whose bytes are hashed.
    pub python_resolved_path: PathBuf,
    /// SHA-256 of the resolved Python executable.
    pub python_sha256: String,
    /// Resolved repository root.
    pub repository: PathBuf,
    /// SHA-256 of the six named local Python modules used by this entrypoint.
    pub modules_sha256: String,
    /// Concrete checkpoint directory, never an alias such as `runs/sketch1`.
    pub checkpoint: PathBuf,
    /// SHA-256 of config.json, vocab.txt and weights.safetensors in fixed order.
    pub checkpoint_sha256: String,
    /// Resolved home directory passed to the script.
    pub script_home: PathBuf,
    /// Resolved img2bez helper invoked by the script.
    pub img2bez_path: PathBuf,
    /// SHA-256 of the helper executable.
    pub img2bez_sha256: String,
}

/// Full-image calibration plus the exact dark-ink box used by this legacy runner.
#[derive(Clone, Copy, Debug)]
pub struct SketchPlacement {
    /// Image boundary and baseline mapping, including white padding.
    pub calibration: TraceCalibration,
    /// Half-open box `[left, top, right, bottom]` of pixels darker than 128.
    pub ink_box_px: [u32; 4],
}

/// One bounded, scratch-only inference request.
#[derive(Clone, Debug)]
pub struct SketchRequest {
    /// Encoded PNG bytes copied into a private temporary directory.
    pub png: Vec<u8>,
    /// Target glyph name echoed by the model GLIF.
    pub glyph: String,
    /// Optional Unicode scalar for model conditioning.
    pub codepoint: Option<u32>,
    /// Integer target advance, retained only for model conditioning.
    pub advance: f64,
    /// Explicit image and ink placement.
    pub placement: SketchPlacement,
    /// One through three grammar-constrained samples.
    pub candidates: u8,
    /// Finite sampling temperature from zero through two.
    pub temperature: f64,
    /// Deterministic local sampling seed.
    pub seed: u32,
    /// Wall-clock limit, from one second through ten minutes.
    pub timeout: Duration,
}

/// Detached editable outline and measured invocation provenance.
#[derive(Clone, Debug)]
pub struct SketchCandidate {
    /// Exact runtime and checkpoint bytes used for the invocation.
    pub runtime: SketchRuntimeIdentity,
    /// SHA-256 of the source PNG.
    pub image_sha256: String,
    /// Original image dimensions, including padding.
    pub image_size_px: [u32; 2],
    /// Supplied placement and checked dark-ink box.
    pub placement: SketchPlacement,
    /// Score reported by the script; not a visual-quality judgment.
    pub script_score: f64,
    /// Editable ordinary contours in font coordinates.
    pub contours: Vec<DrawingContour>,
}

/// Hash the exact installed entrypoint, dependencies and concrete checkpoint.
///
/// This pins named local files, not every transitive Python wheel or system library.
pub fn inspect_runtime(runtime: &SketchRuntime) -> Result<SketchRuntimeIdentity, String> {
    if fs::symlink_metadata(&runtime.checkpoint)
        .map_err(|error| format!("checkpoint metadata: {error}"))?
        .file_type()
        .is_symlink()
    {
        return Err("checkpoint directory must be concrete, not a symlink alias".into());
    }
    let repository = runtime
        .repository
        .canonicalize()
        .map_err(|error| format!("repository path: {error}"))?;
    let checkpoint = runtime
        .checkpoint
        .canonicalize()
        .map_err(|error| format!("checkpoint path: {error}"))?;
    if !checkpoint.starts_with(repository.join("runs")) {
        return Err("checkpoint must be a concrete run under the installed repository".into());
    }
    let python_resolved_path = runtime
        .python
        .canonicalize()
        .map_err(|error| format!("Python path: {error}"))?;
    let python_parent = runtime
        .python
        .parent()
        .ok_or("Python entrypoint has no parent directory")?
        .canonicalize()
        .map_err(|error| format!("Python parent: {error}"))?;
    let python_path = python_parent.join(
        runtime
            .python
            .file_name()
            .ok_or("Python entrypoint has no file name")?,
    );
    let python_sha256 = hash_file(&python_resolved_path)?;
    let modules_sha256 = hash_named_files(&repository.join("glyphlab"), MODULE_FILES)?;
    let checkpoint_sha256 = hash_named_files(&checkpoint, MODEL_FILES)?;
    let script_home = runtime
        .script_home
        .canonicalize()
        .map_err(|error| format!("script home: {error}"))?;
    let img2bez_path = script_home
        .join(".cargo/bin/img2bez")
        .canonicalize()
        .map_err(|error| format!("script img2bez: {error}"))?;
    let img2bez_sha256 = hash_file(&img2bez_path)?;
    Ok(SketchRuntimeIdentity {
        python_path,
        python_resolved_path,
        python_sha256,
        repository,
        modules_sha256,
        checkpoint,
        checkpoint_sha256,
        script_home,
        img2bez_path,
        img2bez_sha256,
    })
}

/// Run the installed model without `--install` and return only detached contours.
///
/// This is a blocking worker operation; callers must run it off the editor event thread.
pub fn run(
    runtime: &SketchRuntime,
    expected: &SketchRuntimeIdentity,
    request: &SketchRequest,
    cancel: &ProcessCancellation,
) -> Result<SketchCandidate, String> {
    let validated = validate_request(request)?;
    let current = inspect_runtime(runtime)?;
    if &current != expected {
        return Err("local sketch runtime or checkpoint changed since it was pinned".into());
    }
    let scratch = Scratch::new()?;
    let image = scratch.path.join("sketch.png");
    let mut file = File::create(&image).map_err(|error| format!("scratch image: {error}"))?;
    file.write_all(&request.png)
        .map_err(|error| format!("scratch image: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("scratch image: {error}"))?;
    let mut command = Command::new(&current.python_path);
    command
        .current_dir(&current.repository)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONNOUSERSITE", "1")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env_remove("GLYPHLAB_FAST_TRACE")
        .env("HOME", &current.script_home)
        .env("TMPDIR", &scratch.path)
        .env("XDG_CACHE_HOME", &scratch.path)
        .args(["-m", "glyphlab.sketch2glyph", "--png"])
        .arg(&image)
        .args(["--glyph", &request.glyph, "--master", "regular", "--width"])
        .arg(request.advance.to_string())
        .args(["--target-height", &validated.height.to_string()])
        .args(["--y-offset", &validated.bottom.to_string()])
        .args(["--lsb", &validated.left.to_string(), "--run"])
        .arg(&current.checkpoint)
        .args(["--k", &request.candidates.to_string()])
        .args(["--temperature", &request.temperature.to_string()])
        .args(["--seed", &request.seed.to_string()]);
    if let Some(codepoint) = request.codepoint {
        command.args(["--unicode", &format!("{codepoint:04X}")]);
    }
    let output = process::run(
        &mut command,
        &[],
        ProcessLimits {
            deadline: request.timeout,
            stdin_bytes: 0,
            stdout_bytes: MAX_GLYPH_BYTES,
            stderr_bytes: 64 * 1024,
        },
        cancel,
        |_, _| {},
    );
    match output.outcome {
        ProcessOutcome::Exited { success: true, .. } => {}
        other => return Err(format!("local sketch inference: {other}")),
    }
    if inspect_runtime(runtime)? != current {
        return Err("local sketch runtime or checkpoint changed during inference".into());
    }
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("local sketch result JSON: {error}"))?;
    if let Some(error) = body.get("error").and_then(serde_json::Value::as_str) {
        return Err(format!("local sketch model: {error}"));
    }
    if body.get("installed").is_some() {
        return Err("local sketch returned an install receipt instead of a detached GLIF".into());
    }
    let glif = body
        .get("glif")
        .and_then(serde_json::Value::as_str)
        .ok_or("local sketch returned no GLIF")?;
    let score = body
        .get("score")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or("local sketch returned no finite score")?;
    let glyph = norad::Glyph::parse_raw(glif.as_bytes())
        .map_err(|error| format!("local sketch GLIF: {error}"))?;
    if glyph.name().as_str() != request.glyph || glyph.width != request.advance {
        return Err("local sketch GLIF name or advance differs from the request".into());
    }
    if glyph.codepoints.iter().collect::<Vec<_>>()
        != request
            .codepoint
            .and_then(char::from_u32)
            .into_iter()
            .collect::<Vec<_>>()
    {
        return Err("local sketch GLIF Unicode differs from the request".into());
    }
    if !glyph.components.is_empty() || !glyph.anchors.is_empty() {
        return Err("local sketch GLIF must contain ordinary contours only".into());
    }
    let contours = glyph
        .contours
        .iter()
        .map(|contour| DrawingContour {
            points: contour
                .points
                .iter()
                .map(|point| DrawingPoint {
                    x: point.x,
                    y: point.y,
                    kind: match point.typ {
                        norad::PointType::Move => DrawingPointType::Move,
                        norad::PointType::Line => DrawingPointType::Line,
                        norad::PointType::Curve => DrawingPointType::Curve,
                        norad::PointType::QCurve => DrawingPointType::Qcurve,
                        norad::PointType::OffCurve => DrawingPointType::Offcurve,
                    },
                    smooth: point.smooth,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    if contours.is_empty() {
        return Err("local sketch returned no editable contours".into());
    }
    crate::outline::drawing::validate(&contours)?;
    Ok(SketchCandidate {
        runtime: current,
        image_sha256: format!("sha256:{:x}", Sha256::digest(&request.png)),
        image_size_px: validated.image_size,
        placement: request.placement,
        script_score: score,
        contours,
    })
}

struct ValidatedPlacement {
    image_size: [u32; 2],
    height: i64,
    bottom: i64,
    left: i64,
}

fn validate_request(request: &SketchRequest) -> Result<ValidatedPlacement, String> {
    if request.png.is_empty() || request.png.len() > MAX_PNG_BYTES {
        return Err("sketch PNG exceeds the four-megabyte input bound".into());
    }
    if request.glyph.is_empty()
        || request.glyph.len() > 128
        || request
            .glyph
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err("sketch glyph name is absent or invalid".into());
    }
    if request
        .codepoint
        .is_some_and(|value| char::from_u32(value).is_none())
    {
        return Err("sketch Unicode scalar is invalid".into());
    }
    if !request.advance.is_finite()
        || !(1.0..=10_000.0).contains(&request.advance)
        || request.advance.fract() != 0.0
        || !(1..=3).contains(&request.candidates)
        || !request.temperature.is_finite()
        || !(0.0..=2.0).contains(&request.temperature)
        || !(Duration::from_secs(1)..=Duration::from_secs(600)).contains(&request.timeout)
    {
        return Err("sketch advance, sampling or deadline is outside bounded values".into());
    }
    let (width, height) = image::ImageReader::with_format(
        std::io::Cursor::new(&request.png),
        image::ImageFormat::Png,
    )
    .into_dimensions()
    .map_err(|error| format!("sketch PNG dimensions: {error}"))?;
    if width == 0
        || height == 0
        || width > MAX_EDGE
        || height > MAX_EDGE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("sketch PNG dimensions exceed the image bound".into());
    }
    let decoded = image::load_from_memory_with_format(&request.png, image::ImageFormat::Png)
        .map_err(|error| format!("sketch PNG decode: {error}"))?;
    let [left, top, right, bottom] = request.placement.ink_box_px;
    if left >= right || top >= bottom || right > width || bottom > height {
        return Err("sketch ink box is outside the full image".into());
    }
    let pixels = decoded.to_luma8();
    let mut actual = [width, height, 0, 0];
    for (x, y, pixel) in pixels.enumerate_pixels() {
        if pixel[0] < 128 {
            actual[0] = actual[0].min(x);
            actual[1] = actual[1].min(y);
            actual[2] = actual[2].max(x + 1);
            actual[3] = actual[3].max(y + 1);
        }
    }
    if actual != request.placement.ink_box_px {
        return Err(
            "sketch ink box must exactly cover dark pixels, including image padding".into(),
        );
    }
    let calibration = request.placement.calibration;
    if !calibration.font_units_per_pixel.is_finite()
        || !(0.01..=1_024.0).contains(&calibration.font_units_per_pixel)
        || !calibration.pixel_baseline_y.is_finite()
        || !(0.0..=f64::from(height)).contains(&calibration.pixel_baseline_y)
        || !calibration.font_x_at_left.is_finite()
        || !calibration.font_baseline_y.is_finite()
    {
        return Err("sketch pixel-to-font calibration is invalid".into());
    }
    let scale = calibration.font_units_per_pixel;
    let height_units = f64::from(bottom - top) * scale;
    let bottom_units =
        calibration.font_baseline_y + (calibration.pixel_baseline_y - f64::from(bottom)) * scale;
    let left_units = calibration.font_x_at_left + f64::from(left) * scale;
    Ok(ValidatedPlacement {
        image_size: [width, height],
        height: exact_cli_integer(height_units, 1, 10_000)?,
        bottom: exact_cli_integer(bottom_units, -1_000_000, 1_000_000)?,
        left: exact_cli_integer(left_units, -1_000_000, 1_000_000)?,
    })
}

fn exact_cli_integer(value: f64, minimum: i64, maximum: i64) -> Result<i64, String> {
    if !value.is_finite()
        || value.fract() != 0.0
        || value < minimum as f64
        || value > maximum as f64
    {
        return Err("calibration cannot be represented by sketch2glyph's integer CLI".into());
    }
    value
        .to_string()
        .parse::<i64>()
        .map_err(|_| "calibration cannot be represented by sketch2glyph's integer CLI".into())
}

fn hash_named_files(root: &Path, names: &[&str]) -> Result<String, String> {
    let mut digest = Sha256::new();
    for name in names {
        digest.update(name.as_bytes());
        digest.update([0]);
        let path = root.join(name);
        let mut file = File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        hash_stream(&mut file, &mut digest, &path)?;
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    hash_stream(&mut file, &mut digest, path)?;
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn hash_stream(file: &mut File, digest: &mut Sha256, path: &Path) -> Result<(), String> {
    let size = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if size == 0 || size > MAX_RUNTIME_FILE_BYTES {
        return Err(format!("{} has invalid or oversized bytes", path.display()));
    }
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(())
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Result<Self, String> {
        for _ in 0..32 {
            let id = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "runebender-local-sketch-{}-{id}",
                std::process::id()
            ));
            #[cfg(unix)]
            let result = {
                use std::os::unix::fs::DirBuilderExt as _;
                fs::DirBuilder::new().mode(0o700).create(&path)
            };
            #[cfg(not(unix))]
            let result = fs::create_dir(&path);
            match result {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("scratch directory: {error}")),
            }
        }
        Err("could not allocate a private sketch directory".into())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn fixture(response: &str) -> (Scratch, SketchRuntime, SketchRequest) {
        let root = Scratch::new().unwrap();
        let modules = root.path.join("glyphlab");
        let checkpoint = root.path.join("runs/model");
        fs::create_dir_all(&modules).unwrap();
        fs::create_dir_all(&checkpoint).unwrap();
        for name in MODULE_FILES {
            fs::write(modules.join(name), b"offline fake module").unwrap();
        }
        for name in MODEL_FILES {
            fs::write(checkpoint.join(name), b"offline fake checkpoint").unwrap();
        }
        let python_target = root.path.join("fake-python-target");
        let python = root.path.join("venv/bin/python");
        fs::create_dir_all(python.parent().unwrap()).unwrap();
        let script_home = root.path.join("home");
        fs::create_dir_all(script_home.join(".cargo/bin")).unwrap();
        fs::write(
            script_home.join(".cargo/bin/img2bez"),
            b"offline fake tracer",
        )
        .unwrap();
        let script = format!(
            "#!/bin/sh\nfor argument in \"$@\"; do\n  if [ \"$argument\" = --install ]; then exit 44; fi\ndone\ncase \"$*\" in\n  *\"--target-height 20 --y-offset 8 --lsb 18\"*) ;;\n  *) exit 45;;\nesac\nprintf '%s\\n' '{response}'\n"
        );
        fs::write(&python_target, script).unwrap();
        fs::set_permissions(&python_target, fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink(&python_target, &python).unwrap();
        let runtime = SketchRuntime {
            repository: root.path.clone(),
            python,
            checkpoint,
            script_home,
        };
        let mut image = image::GrayImage::from_pixel(32, 32, image::Luma([255_u8]));
        for y in 6..16 {
            for x in 4..14 {
                image.put_pixel(x, y, image::Luma([0_u8]));
            }
        }
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let request = SketchRequest {
            png,
            glyph: "demo".into(),
            codepoint: Some(u32::from('A')),
            advance: 400.0,
            placement: SketchPlacement {
                calibration: TraceCalibration {
                    font_units_per_pixel: 2.0,
                    pixel_baseline_y: 20.0,
                    font_x_at_left: 10.0,
                    font_baseline_y: 0.0,
                },
                ink_box_px: [4, 6, 14, 16],
            },
            candidates: 1,
            temperature: 0.0,
            seed: 7,
            timeout: Duration::from_secs(5),
        };
        (root, runtime, request)
    }

    fn glif_result() -> String {
        let mut glyph = norad::Glyph::new("demo");
        glyph.width = 400.0;
        glyph.codepoints.insert('A');
        glyph.contours.push(norad::Contour::new(
            vec![
                norad::ContourPoint::new(20.0, 0.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(80.0, 0.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(80.0, 100.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(20.0, 100.0, norad::PointType::Line, false, None, None),
            ],
            None,
        ));
        let xml = String::from_utf8(glyph.encode_xml().unwrap()).unwrap();
        serde_json::json!({"glif":xml,"score":123.5}).to_string()
    }

    #[test]
    fn fake_runner_returns_detached_editable_contours_and_exact_receipt() {
        let (_root, runtime, request) = fixture(&glif_result());
        let pinned = inspect_runtime(&runtime).unwrap();
        assert_eq!(
            pinned.python_path,
            runtime
                .python
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join(runtime.python.file_name().unwrap())
        );
        assert_ne!(pinned.python_resolved_path, pinned.python_path);
        let candidate = run(&runtime, &pinned, &request, &ProcessCancellation::default()).unwrap();
        assert_eq!(candidate.runtime, pinned);
        assert_eq!(candidate.image_size_px, [32, 32]);
        assert_eq!(candidate.placement.ink_box_px, [4, 6, 14, 16]);
        assert_eq!(candidate.script_score, 123.5);
        assert_eq!(candidate.contours.len(), 1);
        assert_eq!(candidate.contours[0].points.len(), 4);
        assert_eq!(candidate.contours[0].points[0].x, 20.0);
    }

    #[test]
    fn rejects_alias_drift_padding_error_and_lossy_calibration() {
        let (root, runtime, mut request) = fixture(&glif_result());
        let pinned = inspect_runtime(&runtime).unwrap();
        let alias = root.path.join("runs/alias");
        std::os::unix::fs::symlink(&runtime.checkpoint, &alias).unwrap();
        let mut aliased = runtime.clone();
        aliased.checkpoint = alias;
        assert!(inspect_runtime(&aliased).is_err());
        request.placement.ink_box_px = [0, 0, 14, 16];
        assert!(run(&runtime, &pinned, &request, &ProcessCancellation::default()).is_err());
        request.placement.ink_box_px = [4, 6, 14, 16];
        request.placement.calibration.font_x_at_left = 10.5;
        assert!(run(&runtime, &pinned, &request, &ProcessCancellation::default()).is_err());
        request.placement.calibration.font_x_at_left = 10.0;
        fs::write(runtime.checkpoint.join("vocab.txt"), b"changed checkpoint").unwrap();
        assert!(run(&runtime, &pinned, &request, &ProcessCancellation::default()).is_err());
    }

    #[test]
    fn fake_runner_cancel_and_invalid_output_never_create_a_candidate() {
        let (_root, runtime, request) = fixture("{\"error\":\"no valid candidate\"}");
        let pinned = inspect_runtime(&runtime).unwrap();
        let cancel = ProcessCancellation::default();
        cancel.cancel();
        assert!(run(&runtime, &pinned, &request, &cancel).is_err());
        let error = run(&runtime, &pinned, &request, &ProcessCancellation::default()).unwrap_err();
        assert!(error.contains("no valid candidate"), "{error}");
    }

    #[test]
    fn fake_runner_reports_process_exit_deadline_and_malformed_glif() {
        let (_root, runtime, mut request) = fixture("{\"glif\":\"broken\",\"score\":1}");
        let mut pinned = inspect_runtime(&runtime).unwrap();
        let error = run(&runtime, &pinned, &request, &ProcessCancellation::default()).unwrap_err();
        assert!(error.contains("GLIF"), "{error}");

        fs::write(&runtime.python, b"#!/bin/sh\nexit 9\n").unwrap();
        pinned = inspect_runtime(&runtime).unwrap();
        let error = run(&runtime, &pinned, &request, &ProcessCancellation::default()).unwrap_err();
        assert!(error.contains("status Some(9)"), "{error}");

        fs::write(&runtime.python, b"#!/bin/sh\nexec sleep 3\n").unwrap();
        pinned = inspect_runtime(&runtime).unwrap();
        request.timeout = Duration::from_secs(1);
        let error = run(&runtime, &pinned, &request, &ProcessCancellation::default()).unwrap_err();
        assert!(error.contains("deadline"), "{error}");
    }
}
