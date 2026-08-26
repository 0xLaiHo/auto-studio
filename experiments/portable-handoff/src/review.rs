use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use autostudio_music_quality::ExperimentalMusicSpec;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{
    FEEDBACK_FILE, FEEDBACK_TEMPLATE_FILE, PREVIEW_CHANNELS, PREVIEW_FILE, PREVIEW_SAMPLE_RATE,
    REVIEW_BRIEF_IDS, REVIEW_MANIFEST_FILE, REVIEW_SCHEMA_VERSION,
};
use crate::error::{ContentReviewError, HandoffError};
use crate::evidence::write_portable_handoff;

#[derive(Clone, Debug)]
pub struct ContentReviewRequest {
    pub evidence_root: PathBuf,
    pub protocol_lock: PathBuf,
    pub soundfont: PathBuf,
    pub output_dir: PathBuf,
    pub fluidsynth_binary: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentReviewManifest {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub protocol: ContentReviewArtifact,
    pub formal_summary: ContentReviewArtifact,
    pub soundfont: LocalReviewAsset,
    pub renderer: String,
    pub mutable_feedback_file: String,
    pub samples: Vec<ContentReviewSample>,
    pub artifacts: Vec<ContentReviewArtifact>,
    pub release_claim: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentReviewVerification {
    pub samples: usize,
    pub artifacts: usize,
    pub feedback_ready: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentReviewSample {
    pub brief_id: String,
    pub source_spec_sha256: String,
    pub protocol_binding_sha256: String,
    pub preview: ContentReviewAudio,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentReviewAudio {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub duration_seconds: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentReviewArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalReviewAsset {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub usage: String,
    pub redistributed: bool,
}

#[derive(Debug, Deserialize)]
struct FormalSummary {
    expected_mode_b: usize,
    observed_candidates: usize,
    completed_candidates: usize,
    invalid_candidates: Vec<String>,
    mode_b_valid_and_compiled: usize,
    mode_b_device_gate_passed: bool,
}

#[derive(Debug, Deserialize)]
struct ProtocolLock {
    schema_version: String,
    modes: ProtocolModes,
}

#[derive(Debug, Deserialize)]
struct ProtocolModes {
    b_brief_ids: Vec<String>,
    c_brief_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProtocolBinding {
    protocol_id: String,
    protocol_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CreatorFeedbackFile {
    schema_version: String,
    samples: Vec<CreatorFeedbackSample>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CreatorFeedbackSample {
    brief_id: String,
    feedback: Vec<String>,
    ready_for_mode_c: bool,
}

trait PreviewRenderer {
    fn identity(&self) -> Result<String, ContentReviewError>;
    fn render(&self, midi: &Path, output: &Path) -> Result<(), ContentReviewError>;
}

struct FluidSynthRenderer<'a> {
    binary: &'a Path,
    soundfont: &'a Path,
}

impl PreviewRenderer for FluidSynthRenderer<'_> {
    fn identity(&self) -> Result<String, ContentReviewError> {
        let output = Command::new(self.binary)
            .args(["-ni", "-q", "-V"])
            .output()
            .map_err(|error| ContentReviewError::RendererFailed {
                binary: self.binary.display().to_string(),
                status: "not started".to_owned(),
                detail: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(renderer_failure(self.binary, &output));
        }
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        combined
            .lines()
            .map(str::trim)
            .find(|line| line.contains("FluidSynth runtime version"))
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ContentReviewError::InvalidInput(
                    "FluidSynth version output did not contain a runtime version".to_owned(),
                )
            })
    }

    fn render(&self, midi: &Path, output: &Path) -> Result<(), ContentReviewError> {
        let rendered = Command::new(self.binary)
            .args(["-ni", "-q", "-F"])
            .arg(output)
            .args(["-T", "wav", "-O", "s16", "-r"])
            .arg(PREVIEW_SAMPLE_RATE.to_string())
            .arg(self.soundfont)
            .arg(midi)
            .output()
            .map_err(|error| ContentReviewError::RendererFailed {
                binary: self.binary.display().to_string(),
                status: "not started".to_owned(),
                detail: error.to_string(),
            })?;
        if rendered.status.success() {
            Ok(())
        } else {
            Err(renderer_failure(self.binary, &rendered))
        }
    }
}

/// Builds the local-only six-sample Q0 content-review package.
///
/// The frozen Q0 sources are read and verified but never modified. The
/// `SoundFont` is hashed and used by `FluidSynth`, not copied into the package.
///
/// # Errors
///
/// Returns an error when the Q0 machine gate is not exactly 6/6, a source is
/// missing or has a mismatched protocol binding, rendering fails, or the final
/// output path already exists.
pub fn prepare_content_review_pack(
    request: &ContentReviewRequest,
) -> Result<ContentReviewManifest, HandoffError> {
    let renderer = FluidSynthRenderer {
        binary: &request.fluidsynth_binary,
        soundfont: &request.soundfont,
    };
    prepare_content_review_pack_with_renderer(request, &renderer)
}

fn prepare_content_review_pack_with_renderer(
    request: &ContentReviewRequest,
    renderer: &impl PreviewRenderer,
) -> Result<ContentReviewManifest, HandoffError> {
    validate_request(request)?;
    let protocol_bytes = fs::read(&request.protocol_lock)?;
    let protocol_sha256 = sha256(&protocol_bytes);
    let protocol: ProtocolLock = serde_json::from_slice(&protocol_bytes)?;
    validate_protocol(&protocol)?;

    let summary_path = request.evidence_root.join("formal-summary.json");
    let summary_bytes = fs::read(&summary_path)?;
    let summary: FormalSummary = serde_json::from_slice(&summary_bytes)?;
    validate_summary(&summary)?;

    let parent = request
        .output_dir
        .parent()
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".q0-content-review-")
        .tempdir_in(parent)?;
    let staging_root = staging.path();
    let renderer_identity = renderer.identity()?;
    let mut samples = Vec::with_capacity(REVIEW_BRIEF_IDS.len());

    for brief_id in REVIEW_BRIEF_IDS {
        let source_dir = request.evidence_root.join("mode-b").join(brief_id);
        let source_spec_path = source_dir.join("spec.json");
        let source_spec_bytes = fs::read(&source_spec_path)?;
        let spec = ExperimentalMusicSpec::parse_and_validate(&String::from_utf8_lossy(
            &source_spec_bytes,
        ))?;
        let binding_path = source_dir.join("protocol-binding.json");
        let binding_bytes = fs::read(&binding_path)?;
        let binding: ProtocolBinding = serde_json::from_slice(&binding_bytes)?;
        validate_binding(
            &binding,
            &protocol.schema_version,
            &protocol_sha256,
            brief_id,
        )?;

        let sample_dir = staging_root.join("samples").join(brief_id);
        write_portable_handoff(&sample_dir, &spec)?;
        copy_source(
            &source_dir.join("brief.json"),
            &sample_dir.join("brief.json"),
        )?;
        copy_source(&binding_path, &sample_dir.join("protocol-binding.json"))?;

        let preview_path = sample_dir.join(PREVIEW_FILE);
        renderer.render(&sample_dir.join("composition.mid"), &preview_path)?;
        let preview = inspect_preview(staging_root, &preview_path)?;
        samples.push(ContentReviewSample {
            brief_id: brief_id.to_owned(),
            source_spec_sha256: sha256(&source_spec_bytes),
            protocol_binding_sha256: sha256(&binding_bytes),
            preview,
        });
    }

    fs::write(staging_root.join("README.md"), review_instructions())?;
    let feedback_bytes = serde_json::to_vec_pretty(&feedback_template())?;
    fs::write(staging_root.join(FEEDBACK_TEMPLATE_FILE), &feedback_bytes)?;
    fs::write(staging_root.join(FEEDBACK_FILE), &feedback_bytes)?;
    let artifacts = collect_artifacts(staging_root)?;
    let manifest = ContentReviewManifest {
        schema_version: REVIEW_SCHEMA_VERSION.to_owned(),
        generated_at: Utc::now(),
        protocol: artifact_for_external("protocol-v3-l4.lock.json", &protocol_bytes),
        formal_summary: artifact_for_external("formal-summary.json", &summary_bytes),
        soundfont: local_soundfont(&request.soundfont)?,
        renderer: renderer_identity,
        mutable_feedback_file: FEEDBACK_FILE.to_owned(),
        samples,
        artifacts,
        release_claim: "Local content-review evidence only; not a production render, Factory Pack, or cross-DAW qualification".to_owned(),
    };
    fs::write(
        staging_root.join(REVIEW_MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    let staged_path = staging.keep();
    fs::rename(&staged_path, &request.output_dir)
        .map_err(|error| ContentReviewError::Persist(error.to_string()))?;
    Ok(manifest)
}

/// Verifies every immutable file in a generated Q0 content-review package.
///
/// Creator edits to `feedback.json` are validated structurally but excluded
/// from immutable artifact hashes.
///
/// # Errors
///
/// Returns an error when a hashed artifact is missing or changed, the package
/// contains an unexpected immutable file, or Creator feedback has an invalid
/// corpus identity or shape.
pub fn verify_content_review_pack(
    review_dir: &Path,
) -> Result<ContentReviewVerification, HandoffError> {
    let manifest_path = review_dir.join(REVIEW_MANIFEST_FILE);
    let manifest: ContentReviewManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    if manifest.schema_version != REVIEW_SCHEMA_VERSION
        || manifest.samples.len() != REVIEW_BRIEF_IDS.len()
        || manifest.mutable_feedback_file != FEEDBACK_FILE
    {
        return Err(ContentReviewError::InvalidInput(
            "review manifest has an unsupported schema or corpus shape".to_owned(),
        )
        .into());
    }

    for artifact in &manifest.artifacts {
        let relative = safe_relative_path(&artifact.path)?;
        let path = review_dir.join(relative);
        let bytes = fs::read(&path).map_err(|error| {
            ContentReviewError::InvalidInput(format!(
                "cannot read immutable artifact {}: {error}",
                path.display()
            ))
        })?;
        if artifact.bytes != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
            || artifact.sha256 != sha256(&bytes)
        {
            return Err(ContentReviewError::InvalidInput(format!(
                "immutable artifact changed: {}",
                artifact.path
            ))
            .into());
        }
    }

    let actual = collect_artifacts(review_dir)?;
    if actual != manifest.artifacts {
        return Err(ContentReviewError::InvalidInput(
            "review directory contains missing or unexpected immutable artifacts".to_owned(),
        )
        .into());
    }
    let feedback: CreatorFeedbackFile =
        serde_json::from_slice(&fs::read(review_dir.join(FEEDBACK_FILE))?)?;
    let feedback_ready = validate_feedback(&feedback)?;
    Ok(ContentReviewVerification {
        samples: manifest.samples.len(),
        artifacts: manifest.artifacts.len(),
        feedback_ready,
    })
}

fn validate_request(request: &ContentReviewRequest) -> Result<(), ContentReviewError> {
    if request.output_dir.exists() {
        return Err(ContentReviewError::OutputExists(
            request.output_dir.display().to_string(),
        ));
    }
    for (label, path) in [
        ("evidence root", &request.evidence_root),
        ("protocol lock", &request.protocol_lock),
        ("SoundFont", &request.soundfont),
    ] {
        if !path.exists() {
            return Err(ContentReviewError::InvalidInput(format!(
                "{label} does not exist: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_protocol(protocol: &ProtocolLock) -> Result<(), ContentReviewError> {
    let expected = REVIEW_BRIEF_IDS.map(str::to_owned).to_vec();
    if protocol.schema_version != "q0-protocol-v3-l4-rebaseline"
        || protocol.modes.b_brief_ids != expected
        || protocol.modes.c_brief_ids != expected
    {
        return Err(ContentReviewError::InvalidInput(
            "protocol lock does not contain the frozen six-pair L4 B/C corpus".to_owned(),
        ));
    }
    Ok(())
}

fn validate_summary(summary: &FormalSummary) -> Result<(), ContentReviewError> {
    if summary.expected_mode_b != REVIEW_BRIEF_IDS.len()
        || summary.observed_candidates != REVIEW_BRIEF_IDS.len()
        || summary.completed_candidates != REVIEW_BRIEF_IDS.len()
        || summary.mode_b_valid_and_compiled != REVIEW_BRIEF_IDS.len()
        || !summary.invalid_candidates.is_empty()
        || !summary.mode_b_device_gate_passed
    {
        return Err(ContentReviewError::InvalidInput(
            "formal summary must prove an exact 6/6 valid and compiled Mode B corpus".to_owned(),
        ));
    }
    Ok(())
}

fn validate_binding(
    binding: &ProtocolBinding,
    protocol_id: &str,
    protocol_sha256: &str,
    brief_id: &str,
) -> Result<(), ContentReviewError> {
    if binding.protocol_id != protocol_id || binding.protocol_sha256 != protocol_sha256 {
        return Err(ContentReviewError::InvalidInput(format!(
            "{brief_id} is not bound to the supplied protocol lock"
        )));
    }
    Ok(())
}

fn inspect_preview(root: &Path, path: &Path) -> Result<ContentReviewAudio, ContentReviewError> {
    let reader = hound::WavReader::open(path)
        .map_err(|error| ContentReviewError::InvalidWav(error.to_string()))?;
    let spec = reader.spec();
    if spec.sample_rate != PREVIEW_SAMPLE_RATE {
        return Err(ContentReviewError::SampleRateMismatch {
            actual: spec.sample_rate,
            expected: PREVIEW_SAMPLE_RATE,
        });
    }
    if spec.channels != PREVIEW_CHANNELS {
        return Err(ContentReviewError::ChannelMismatch {
            actual: spec.channels,
            expected: PREVIEW_CHANNELS,
        });
    }
    let duration_seconds = f64::from(reader.duration()) / f64::from(spec.sample_rate);
    let bytes = fs::read(path).map_err(|error| {
        ContentReviewError::InvalidWav(format!("cannot reread preview: {error}"))
    })?;
    Ok(ContentReviewAudio {
        path: relative_path(root, path)?,
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256(&bytes),
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        bits_per_sample: spec.bits_per_sample,
        duration_seconds,
    })
}

fn local_soundfont(path: &Path) -> Result<LocalReviewAsset, HandoffError> {
    let bytes = fs::read(path)?;
    Ok(LocalReviewAsset {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("local-soundfont")
            .to_owned(),
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256(&bytes),
        usage: "Local Q0 evaluation only; redistribution is not approved".to_owned(),
        redistributed: false,
    })
}

fn collect_artifacts(root: &Path) -> Result<Vec<ContentReviewArtifact>, ContentReviewError> {
    let mut pending = vec![root.to_path_buf()];
    let mut artifacts = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|error| {
            ContentReviewError::InvalidInput(format!("cannot enumerate review pack: {error}"))
        })? {
            let path = entry
                .map_err(|error| ContentReviewError::InvalidInput(error.to_string()))?
                .path();
            if path.is_dir() {
                pending.push(path);
            } else if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(REVIEW_MANIFEST_FILE | FEEDBACK_FILE)
            ) {
                let bytes = fs::read(&path).map_err(|error| {
                    ContentReviewError::InvalidInput(format!(
                        "cannot hash {}: {error}",
                        path.display()
                    ))
                })?;
                artifacts.push(ContentReviewArtifact {
                    path: relative_path(root, &path)?,
                    bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    sha256: sha256(&bytes),
                });
            }
        }
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(artifacts)
}

fn copy_source(source: &Path, destination: &Path) -> Result<(), HandoffError> {
    let bytes = fs::read(source)?;
    fs::write(destination, bytes)?;
    Ok(())
}

fn artifact_for_external(path: &str, bytes: &[u8]) -> ContentReviewArtifact {
    ContentReviewArtifact {
        path: path.to_owned(),
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256(bytes),
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, ContentReviewError> {
    path.strip_prefix(root)
        .map_err(|_| {
            ContentReviewError::InvalidInput(format!(
                "artifact escaped the review root: {}",
                path.display()
            ))
        })?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| ContentReviewError::InvalidInput("artifact path is not UTF-8".to_owned()))
}

fn safe_relative_path(path: &str) -> Result<&Path, ContentReviewError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(ContentReviewError::InvalidInput(format!(
            "unsafe review artifact path: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn renderer_failure(binary: &Path, output: &std::process::Output) -> ContentReviewError {
    let detail = String::from_utf8_lossy(&output.stderr);
    ContentReviewError::RendererFailed {
        binary: binary.display().to_string(),
        status: output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
        detail: detail.chars().take(1_024).collect(),
    }
}

fn feedback_template() -> CreatorFeedbackFile {
    CreatorFeedbackFile {
        schema_version: "q0-creator-feedback-v1".to_owned(),
        samples: REVIEW_BRIEF_IDS
            .into_iter()
            .map(|brief_id| CreatorFeedbackSample {
                brief_id: brief_id.to_owned(),
                feedback: Vec::new(),
                ready_for_mode_c: false,
            })
            .collect(),
    }
}

fn validate_feedback(feedback: &CreatorFeedbackFile) -> Result<bool, ContentReviewError> {
    if feedback.schema_version != "q0-creator-feedback-v1"
        || feedback.samples.len() != REVIEW_BRIEF_IDS.len()
    {
        return Err(ContentReviewError::InvalidInput(
            "Creator feedback has an unsupported schema or corpus size".to_owned(),
        ));
    }
    let mut all_ready = true;
    for (expected_id, sample) in REVIEW_BRIEF_IDS.iter().zip(&feedback.samples) {
        if sample.brief_id != *expected_id || sample.feedback.len() > 2 {
            return Err(ContentReviewError::InvalidInput(format!(
                "invalid Creator feedback shape for {expected_id}"
            )));
        }
        if sample.ready_for_mode_c
            && (sample.feedback.is_empty()
                || sample.feedback.iter().any(|item| item.trim().is_empty()))
        {
            return Err(ContentReviewError::InvalidInput(format!(
                "{expected_id} is marked ready without one or two concrete feedback items"
            )));
        }
        all_ready &= sample.ready_for_mode_c;
    }
    Ok(all_ready)
}

fn review_instructions() -> &'static str {
    "# Q0 Content Review Pack\n\n\
This local-only package contains the six frozen L4 Mode B candidates. It is for writing Creator feedback before Mode C; it is not the later blind B/C score package.\n\n\
For each sample, read `brief.json`, listen to `preview.wav`, and inspect `instrument-assignments.json` only when needed. Enter one or two concrete production changes in the mutable `feedback.json`, then change `ready_for_mode_c` to `true`. `feedback-template.json` is the immutable blank reference and must not be edited. Feedback must name the musical region, role, or behavior to preserve/change. Do not use generic requests such as `make it better`, and do not ask another LLM to invent Creator feedback.\n\n\
The WAV files are fixed 48 kHz stereo local FluidSynth previews using GeneralUser GS. They are not production renders and do not qualify Cubase, Studio One Pro, FL Studio, a Factory Pack, or sound-identical export. The SoundFont is not included in this directory.\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SilentRenderer;

    impl PreviewRenderer for SilentRenderer {
        fn identity(&self) -> Result<String, ContentReviewError> {
            Ok("test-silent-renderer".to_owned())
        }

        fn render(&self, _midi: &Path, output: &Path) -> Result<(), ContentReviewError> {
            let spec = hound::WavSpec {
                channels: PREVIEW_CHANNELS,
                sample_rate: PREVIEW_SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(output, spec)
                .map_err(|error| ContentReviewError::InvalidWav(error.to_string()))?;
            for _ in 0..PREVIEW_SAMPLE_RATE {
                writer
                    .write_sample::<i16>(0)
                    .map_err(|error| ContentReviewError::InvalidWav(error.to_string()))?;
                writer
                    .write_sample::<i16>(0)
                    .map_err(|error| ContentReviewError::InvalidWav(error.to_string()))?;
            }
            writer
                .finalize()
                .map_err(|error| ContentReviewError::InvalidWav(error.to_string()))
        }
    }

    #[test]
    fn prepares_six_protocol_bound_review_samples_without_copying_the_soundfont() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let output_root = tempfile::tempdir().expect("temporary output root");
        let soundfont_root = tempfile::tempdir().expect("temporary soundfont root");
        let soundfont = soundfont_root.path().join("review.sf2");
        fs::write(&soundfont, b"local-only-soundfont").expect("soundfont fixture");
        let request = ContentReviewRequest {
            evidence_root: crate_root.join("../music-quality/evidence/formal-v3-l4"),
            protocol_lock: crate_root.join("../music-quality/protocol-v3-l4.lock.json"),
            soundfont: soundfont.clone(),
            output_dir: output_root.path().join("review"),
            fluidsynth_binary: PathBuf::from("unused-by-fake"),
        };

        let manifest = prepare_content_review_pack_with_renderer(&request, &SilentRenderer)
            .expect("review pack");

        assert_eq!(manifest.samples.len(), REVIEW_BRIEF_IDS.len());
        assert_eq!(manifest.renderer, "test-silent-renderer");
        assert!(!manifest.soundfont.redistributed);
        assert_eq!(manifest.soundfont.sha256, sha256(b"local-only-soundfont"));
        assert!(!request.output_dir.join("review.sf2").exists());
        assert!(request.output_dir.join(REVIEW_MANIFEST_FILE).is_file());
        assert!(manifest.samples.iter().all(|sample| {
            sample.preview.sample_rate == PREVIEW_SAMPLE_RATE
                && sample.preview.channels == PREVIEW_CHANNELS
                && request.output_dir.join(&sample.preview.path).is_file()
        }));
        let feedback: CreatorFeedbackFile = serde_json::from_slice(
            &fs::read(request.output_dir.join(FEEDBACK_FILE)).expect("feedback JSON"),
        )
        .expect("valid feedback JSON");
        assert_eq!(feedback.samples.len(), REVIEW_BRIEF_IDS.len());
        assert!(
            !verify_content_review_pack(&request.output_dir)
                .expect("valid review pack")
                .feedback_ready
        );

        let completed_feedback = CreatorFeedbackFile {
            schema_version: "q0-creator-feedback-v1".to_owned(),
            samples: REVIEW_BRIEF_IDS
                .into_iter()
                .map(|brief_id| CreatorFeedbackSample {
                    brief_id: brief_id.to_owned(),
                    feedback: vec![
                        "Change one named musical region while preserving the motif".to_owned(),
                    ],
                    ready_for_mode_c: true,
                })
                .collect(),
        };
        fs::write(
            request.output_dir.join(FEEDBACK_FILE),
            serde_json::to_vec_pretty(&completed_feedback).expect("feedback serialization"),
        )
        .expect("completed feedback");
        assert!(
            verify_content_review_pack(&request.output_dir)
                .expect("valid pack after Creator feedback")
                .feedback_ready
        );

        fs::write(
            request.output_dir.join("samples/l4-song-neon/brief.json"),
            b"tampered",
        )
        .expect("tampered immutable artifact");
        let error = verify_content_review_pack(&request.output_dir)
            .expect_err("changed immutable artifact must be rejected");
        assert!(error.to_string().contains("immutable artifact changed"));
    }

    #[test]
    fn refuses_to_replace_an_existing_review_pack() {
        let root = tempfile::tempdir().expect("temporary root");
        let request = ContentReviewRequest {
            evidence_root: root.path().join("missing-evidence"),
            protocol_lock: root.path().join("missing-protocol"),
            soundfont: root.path().join("missing-soundfont"),
            output_dir: root.path().to_path_buf(),
            fluidsynth_binary: PathBuf::from("fluidsynth"),
        };

        let error = prepare_content_review_pack_with_renderer(&request, &SilentRenderer)
            .expect_err("existing output must be rejected");

        assert!(error.to_string().contains("output already exists"));
    }
}
