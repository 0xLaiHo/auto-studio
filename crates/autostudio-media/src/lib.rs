//! Media probing, preview, analysis, and DAW handoff.

pub mod constants;
mod error;

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use autostudio_core::production::{
    AssetVersionDraft, AudioMetadata, GeneratedAssetSink, HandoffExportDraft, HandoffFile,
    HandoffRequest, HandoffSink, PreviewByteRange, PreviewChunk, PreviewSource, ProvenanceRecord,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::constants::MAX_PREVIEW_RESPONSE_BYTES;
pub use crate::error::MediaError;

pub struct ProjectMedia {
    package_root: PathBuf,
    staging_root: PathBuf,
}

impl ProjectMedia {
    /// Opens the constrained media paths for one Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the package or staging directories cannot be
    /// created or canonicalized.
    pub fn new(package_root: &Path, staging_root: &Path) -> Result<Self, MediaError> {
        fs::create_dir_all(package_root).map_err(MediaError::Io)?;
        fs::create_dir_all(staging_root).map_err(MediaError::Io)?;
        Ok(Self {
            package_root: package_root.canonicalize().map_err(MediaError::Io)?,
            staging_root: staging_root.canonicalize().map_err(MediaError::Io)?,
        })
    }

    /// Validates a Provider staging WAV and atomically commits it as an immutable Asset.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] for path escape, unsupported audio, malformed WAV,
    /// hashing, copying, syncing, or atomic rename failures.
    pub fn commit_generated_audio(
        &self,
        staging_file: &Path,
        provenance: ProvenanceRecord,
    ) -> Result<AssetVersionDraft, MediaError> {
        let source = staging_file.canonicalize().map_err(MediaError::Io)?;
        if !source.starts_with(&self.staging_root) || !source.is_file() {
            return Err(MediaError::StagingPathEscape);
        }

        let assets = self.package_root.join("assets");
        fs::create_dir_all(&assets).map_err(MediaError::Io)?;
        let temporary = assets.join(format!(".ingest.{}.partial", Uuid::new_v4()));
        let staged = (|| {
            copy_and_sync(&source, &temporary)?;
            let reader = hound::WavReader::open(&temporary).map_err(MediaError::InvalidWav)?;
            let spec = reader.spec();
            if spec.sample_format != hound::SampleFormat::Int
                || !matches!(spec.sample_rate, 44_100 | 48_000)
                || !matches!(spec.channels, 1 | 2)
                || !matches!(spec.bits_per_sample, 16 | 24 | 32)
            {
                return Err(MediaError::UnsupportedWav);
            }
            let duration_micros = u64::from(reader.duration())
                .checked_mul(1_000_000)
                .and_then(|value| value.checked_div(u64::from(spec.sample_rate)))
                .filter(|duration| *duration > 0)
                .ok_or(MediaError::UnsupportedWav)?;
            drop(reader);
            Ok::<_, MediaError>((sha256_file(&temporary)?, spec, duration_micros))
        })();
        let (hash, spec, duration_micros) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
        };
        let file_name = format!("{hash}.wav");
        let destination = assets.join(&file_name);
        if destination.exists() {
            let existing_hash = sha256_file(&destination)?;
            let _ = fs::remove_file(&temporary);
            if existing_hash != hash {
                return Err(MediaError::AssetHashMismatch);
            }
        } else {
            fs::rename(&temporary, &destination).map_err(MediaError::Io)?;
        }

        Ok(AssetVersionDraft {
            relative_path: format!("assets/{file_name}"),
            sha256: format!("sha256:{hash}"),
            media_type: "audio/wav".to_owned(),
            audio: AudioMetadata {
                sample_rate_hz: spec.sample_rate,
                channels: spec.channels,
                duration_micros,
                bit_depth: spec.bits_per_sample,
            },
            provenance,
        })
    }

    /// Publishes the current Selection as a deterministic, idempotent DAW Handoff Package.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the selected Asset escapes the package, its
    /// content no longer matches its recorded hash, serialization fails, or the
    /// package cannot be atomically published.
    pub fn export_handoff(
        &self,
        request: &HandoffRequest,
    ) -> Result<HandoffExportDraft, MediaError> {
        let source = self
            .package_root
            .join(request.asset().relative_path())
            .canonicalize()
            .map_err(MediaError::Io)?;
        if !source.starts_with(&self.package_root) || !source.is_file() {
            return Err(MediaError::ProjectPathEscape);
        }
        let actual_audio_hash = format!("sha256:{}", sha256_file(&source)?);
        if actual_audio_hash != request.asset().sha256() {
            return Err(MediaError::AssetHashMismatch);
        }

        let audio_file = HandoffFile {
            relative_path: "audio/selected.wav".to_owned(),
            sha256: actual_audio_hash,
            media_type: request.asset().media_type().to_owned(),
        };
        let readme = b"Auto Studio DAW Handoff\n\n1. Import audio/selected.wav into a new audio track.\n2. Set tempo/key only when manifest.json contains those hints.\n3. Preserve manifest.json with the session for provenance and rights review.\n";
        let readme_file = HandoffFile {
            relative_path: "README.txt".to_owned(),
            sha256: sha256_bytes(readme),
            media_type: "text/plain; charset=utf-8".to_owned(),
        };
        let files = vec![audio_file.clone(), readme_file];
        let mut missing_capabilities = vec!["stems"];
        if request.tempo_hint_bpm().is_none() {
            missing_capabilities.push("tempo");
        }
        if request.key_hint().is_none() {
            missing_capabilities.push("key");
        }
        if request.markers_micros().is_empty() {
            missing_capabilities.push("markers");
        }
        let manifest = serde_json::to_vec_pretty(&HandoffManifest {
            schema_version: "autostudio.daw-handoff/1",
            source: request,
            full_mix: &audio_file,
            stems: &[],
            files: &files,
            missing_capabilities,
        })
        .map_err(MediaError::Manifest)?;
        let manifest_sha256 = sha256_bytes(&manifest);

        let selection_id = request.selection_id().as_str();
        let relative_path = format!(
            "exports/handoff-r{}-{selection_id}",
            request.source_project_revision()
        );
        let exports_root = self.package_root.join("exports");
        fs::create_dir_all(&exports_root).map_err(MediaError::Io)?;
        let destination = self.package_root.join(&relative_path);
        if destination.exists() {
            verify_existing_handoff(&destination, &source, readme, &manifest)?;
            return Ok(HandoffExportDraft {
                relative_path,
                manifest_sha256,
                files,
            });
        }

        let temporary = exports_root.join(format!(".handoff-{}.partial", Uuid::new_v4()));
        let result = (|| {
            fs::create_dir(&temporary).map_err(MediaError::Io)?;
            fs::create_dir(temporary.join("audio")).map_err(MediaError::Io)?;
            copy_and_sync(&source, &temporary.join("audio/selected.wav"))?;
            write_and_sync(&temporary.join("README.txt"), readme)?;
            write_and_sync(&temporary.join("manifest.json"), &manifest)?;
            fs::rename(&temporary, &destination).map_err(MediaError::Io)?;
            sync_directory(&exports_root)?;
            Ok(())
        })();
        if result.is_err() && temporary.exists() {
            let _ = fs::remove_dir_all(&temporary);
        }
        result?;

        Ok(HandoffExportDraft {
            relative_path,
            manifest_sha256,
            files,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HandoffManifest<'a> {
    schema_version: &'static str,
    source: &'a HandoffRequest,
    full_mix: &'a HandoffFile,
    stems: &'a [HandoffFile],
    files: &'a [HandoffFile],
    missing_capabilities: Vec<&'static str>,
}

impl GeneratedAssetSink for ProjectMedia {
    fn commit_audio(
        &self,
        staging_file: &Path,
        provenance: ProvenanceRecord,
    ) -> Result<AssetVersionDraft, String> {
        self.commit_generated_audio(staging_file, provenance)
            .map_err(|error| error.to_string())
    }
}

impl HandoffSink for ProjectMedia {
    fn export(&self, request: &HandoffRequest) -> Result<HandoffExportDraft, String> {
        self.export_handoff(request)
            .map_err(|error| error.to_string())
    }
}

impl PreviewSource for ProjectMedia {
    fn read(
        &self,
        asset: &autostudio_core::production::AssetVersion,
        range: Option<PreviewByteRange>,
    ) -> Result<PreviewChunk, String> {
        self.read_preview(asset, range)
            .map_err(|error| error.to_string())
    }
}

impl ProjectMedia {
    fn read_preview(
        &self,
        asset: &autostudio_core::production::AssetVersion,
        range: Option<PreviewByteRange>,
    ) -> Result<PreviewChunk, MediaError> {
        let path = self
            .package_root
            .join(asset.relative_path())
            .canonicalize()
            .map_err(MediaError::Io)?;
        if !path.starts_with(&self.package_root) || !path.is_file() {
            return Err(MediaError::ProjectPathEscape);
        }
        if format!("sha256:{}", sha256_file(&path)?) != asset.sha256() {
            return Err(MediaError::AssetHashMismatch);
        }
        let total_size = path.metadata().map_err(MediaError::Io)?.len();
        if total_size == 0 {
            return Err(MediaError::PreviewRange);
        }
        let (start, end_inclusive) = match range {
            Some(range) => (
                range.start,
                range.end_inclusive.unwrap_or(total_size.saturating_sub(1)),
            ),
            None => (0, total_size.saturating_sub(1)),
        };
        if start >= total_size
            || end_inclusive < start
            || end_inclusive >= total_size
            || end_inclusive - start + 1 > MAX_PREVIEW_RESPONSE_BYTES
        {
            return Err(MediaError::PreviewRange);
        }
        let length = end_inclusive - start + 1;
        let mut file = File::open(path).map_err(MediaError::Io)?;
        file.seek(SeekFrom::Start(start)).map_err(MediaError::Io)?;
        let mut bytes =
            Vec::with_capacity(usize::try_from(length).map_err(|_| MediaError::PreviewRange)?);
        file.take(length)
            .read_to_end(&mut bytes)
            .map_err(MediaError::Io)?;
        if bytes.len() as u64 != length {
            return Err(MediaError::PreviewRange);
        }
        Ok(PreviewChunk {
            bytes,
            media_type: asset.media_type().to_owned(),
            total_size,
            start,
            end_inclusive,
        })
    }
}

fn sha256_file(path: &Path) -> Result<String, MediaError> {
    let mut file = File::open(path).map_err(MediaError::Io)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer).map_err(MediaError::Io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn copy_and_sync(source: &Path, destination: &Path) -> Result<(), MediaError> {
    let mut source = File::open(source).map_err(MediaError::Io)?;
    let mut destination = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(MediaError::Io)?;
    std::io::copy(&mut source, &mut destination).map_err(MediaError::Io)?;
    destination.flush().map_err(MediaError::Io)?;
    destination.sync_all().map_err(MediaError::Io)
}

fn write_and_sync(path: &Path, contents: &[u8]) -> Result<(), MediaError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(MediaError::Io)?;
    file.write_all(contents).map_err(MediaError::Io)?;
    file.flush().map_err(MediaError::Io)?;
    file.sync_all().map_err(MediaError::Io)
}

fn sync_directory(path: &Path) -> Result<(), MediaError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(MediaError::Io)
}

fn sha256_bytes(contents: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(contents))
}

fn verify_existing_handoff(
    destination: &Path,
    source: &Path,
    readme: &[u8],
    manifest: &[u8],
) -> Result<(), MediaError> {
    let matches = fs::read(destination.join("audio/selected.wav")).map_err(MediaError::Io)?
        == fs::read(source).map_err(MediaError::Io)?
        && fs::read(destination.join("README.txt")).map_err(MediaError::Io)? == readme
        && fs::read(destination.join("manifest.json")).map_err(MediaError::Io)? == manifest;
    if matches {
        Ok(())
    } else {
        Err(MediaError::ExistingHandoffConflict)
    }
}
