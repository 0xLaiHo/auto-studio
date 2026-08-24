use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::error::ProductionError;

use crate::agent::AgentRunId;

/// Export seam that turns one immutable Selection snapshot into a DAW Handoff Package.
pub trait HandoffSink: Send + Sync {
    /// Materializes a package using only the constrained Project data in `request`.
    ///
    /// # Errors
    ///
    /// Returns a redacted error when package validation or atomic publication fails.
    fn export(&self, request: &HandoffRequest) -> Result<HandoffExportDraft, String>;
}

/// Constrained read seam used for non-authoritative Preview Playback.
pub trait PreviewSource: Send + Sync {
    /// Reads an optional inclusive byte range from a verified Project Audio Asset.
    ///
    /// # Errors
    ///
    /// Returns a redacted error when the Asset is missing, changed, unsafe, or unreadable.
    fn read(
        &self,
        asset: &AssetVersion,
        range: Option<PreviewByteRange>,
    ) -> Result<PreviewChunk, String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewByteRange {
    pub start: u64,
    pub end_inclusive: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewChunk {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub total_size: u64,
    pub start: u64,
    pub end_inclusive: u64,
}

/// Media seam that converts a constrained staging file into an immutable Project Asset.
pub trait GeneratedAssetSink: Send + Sync {
    /// Validates and commits one generated audio artifact.
    ///
    /// # Errors
    ///
    /// Returns a redacted error when media validation or atomic commit fails.
    fn commit_audio(
        &self,
        staging_file: &Path,
        provenance: ProvenanceRecord,
    ) -> Result<AssetVersionDraft, String>;
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub fn as_str(&self) -> String {
                self.0.to_string()
            }

            /// Parses an identity received through a transport.
            ///
            /// # Errors
            ///
            /// Returns [`ProductionError::InvalidId`] for a malformed UUID.
            pub fn parse(value: &str) -> Result<Self, ProductionError> {
                Uuid::parse_str(value)
                    .map(Self)
                    .map_err(|_| ProductionError::InvalidId)
            }
        }
    };
}

uuid_id!(AssetId);
uuid_id!(AssetVersionId);
uuid_id!(CandidateId);
uuid_id!(SelectionId);
uuid_id!(AudioClipId);
uuid_id!(HandoffExportId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMetadata {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_micros: u64,
    pub bit_depth: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RightsDeclaration {
    CreatorOwned,
    LicensedForProject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceRecord {
    pub provider_kind: String,
    pub model: String,
    pub adapter_version: String,
    pub external_job_id: Option<String>,
    pub input_hash: String,
    pub rights: RightsDeclaration,
    pub credits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersionDraft {
    pub relative_path: String,
    pub sha256: String,
    pub media_type: String,
    pub audio: AudioMetadata,
    pub provenance: ProvenanceRecord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersion {
    asset_id: AssetId,
    id: AssetVersionId,
    relative_path: String,
    sha256: String,
    media_type: String,
    audio: AudioMetadata,
    provenance: ProvenanceRecord,
}

impl AssetVersion {
    fn parse(draft: AssetVersionDraft) -> Result<Self, ProductionError> {
        validate_asset_path(&draft.relative_path)?;
        if draft.media_type != "audio/wav" {
            return Err(ProductionError::UnsupportedMediaType);
        }
        validate_audio(&draft.audio)?;
        validate_provenance(&draft.provenance)?;
        validate_sha256(&draft.sha256)?;
        Ok(Self {
            asset_id: AssetId::new(),
            id: AssetVersionId::new(),
            relative_path: draft.relative_path,
            sha256: draft.sha256,
            media_type: draft.media_type,
            audio: draft.audio,
            provenance: draft.provenance,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &AssetVersionId {
        &self.id
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn audio(&self) -> &AudioMetadata {
        &self.audio
    }

    #[must_use]
    pub const fn provenance(&self) -> &ProvenanceRecord {
        &self.provenance
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    fn validate_restored(&self) -> Result<(), ProductionError> {
        AssetId::parse(&self.asset_id.as_str())?;
        AssetVersionId::parse(&self.id.as_str())?;
        validate_asset_path(&self.relative_path)?;
        if self.media_type != "audio/wav" {
            return Err(ProductionError::UnsupportedMediaType);
        }
        validate_audio(&self.audio)?;
        validate_provenance(&self.provenance)?;
        validate_sha256(&self.sha256)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDraft {
    pub label: String,
    pub asset: AssetVersionDraft,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    id: CandidateId,
    source_run_id: AgentRunId,
    label: String,
    asset: AssetVersion,
    note: Option<String>,
}

impl Candidate {
    pub(crate) fn parse(
        source_run_id: &AgentRunId,
        draft: CandidateDraft,
    ) -> Result<Self, ProductionError> {
        let label = draft.label.trim();
        if label.is_empty() {
            return Err(ProductionError::EmptyCandidateLabel);
        }
        Ok(Self {
            id: CandidateId::new(),
            source_run_id: source_run_id.clone(),
            label: label.to_owned(),
            asset: AssetVersion::parse(draft.asset)?,
            note: draft
                .note
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        })
    }

    #[must_use]
    pub const fn id(&self) -> &CandidateId {
        &self.id
    }

    #[must_use]
    pub const fn asset(&self) -> &AssetVersion {
        &self.asset
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn source_run_id(&self) -> &AgentRunId {
        &self.source_run_id
    }

    pub(crate) fn validate_restored(&self) -> Result<(), ProductionError> {
        CandidateId::parse(&self.id.as_str())?;
        AgentRunId::parse(&self.source_run_id.as_str()).map_err(|_| ProductionError::InvalidId)?;
        if self.label.trim().is_empty() {
            return Err(ProductionError::EmptyCandidateLabel);
        }
        self.asset.validate_restored()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    id: SelectionId,
    candidate_id: CandidateId,
    project_revision: u64,
}

impl Selection {
    pub(crate) fn new(candidate_id: CandidateId, project_revision: u64) -> Self {
        Self {
            id: SelectionId::new(),
            candidate_id,
            project_revision,
        }
    }

    #[must_use]
    pub const fn candidate_id(&self) -> &CandidateId {
        &self.candidate_id
    }

    #[must_use]
    pub const fn id(&self) -> &SelectionId {
        &self.id
    }

    pub(crate) const fn project_revision(&self) -> u64 {
        self.project_revision
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClip {
    id: AudioClipId,
    asset_version_id: AssetVersionId,
    start_micros: u64,
    source_in_micros: u64,
    duration_micros: u64,
}

impl AudioClip {
    fn selected(asset: &AssetVersion, start_micros: u64) -> Self {
        Self {
            id: AudioClipId::new(),
            asset_version_id: asset.id().clone(),
            start_micros,
            source_in_micros: 0,
            duration_micros: asset.audio().duration_micros,
        }
    }

    #[must_use]
    pub const fn start_micros(&self) -> u64 {
        self.start_micros
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClipTimeline {
    clips: Vec<AudioClip>,
    tempo_hint_bpm: Option<u16>,
    key_hint: Option<String>,
    markers_micros: Vec<u64>,
}

impl AudioClipTimeline {
    pub(crate) fn select(&mut self, asset: &AssetVersion, start_micros: u64) {
        self.clips.clear();
        self.clips.push(AudioClip::selected(asset, start_micros));
    }

    #[must_use]
    pub fn clips(&self) -> &[AudioClip] {
        &self.clips
    }

    #[must_use]
    pub const fn tempo_hint_bpm(&self) -> Option<u16> {
        self.tempo_hint_bpm
    }

    #[must_use]
    pub fn key_hint(&self) -> Option<&str> {
        self.key_hint.as_deref()
    }

    #[must_use]
    pub fn markers_micros(&self) -> &[u64] {
        &self.markers_micros
    }

    pub(crate) fn validate_restored(
        &self,
        candidates: &[Candidate],
        selection: Option<&Selection>,
    ) -> Result<(), ProductionError> {
        if let Some(tempo) = self.tempo_hint_bpm
            && !(20..=400).contains(&tempo)
        {
            return Err(ProductionError::InvalidTimeline);
        }
        if self
            .key_hint
            .as_ref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err(ProductionError::InvalidTimeline);
        }
        let Some(selection) = selection else {
            return if self.clips.is_empty() {
                Ok(())
            } else {
                Err(ProductionError::InvalidTimeline)
            };
        };
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.id() == selection.candidate_id())
            .ok_or(ProductionError::CandidateNotFound)?;
        if self.clips.len() != 1 {
            return Err(ProductionError::InvalidTimeline);
        }
        let clip = &self.clips[0];
        AudioClipId::parse(&clip.id.as_str())?;
        if clip.asset_version_id != *candidate.asset().id()
            || clip.source_in_micros != 0
            || clip.duration_micros != candidate.asset().audio().duration_micros
        {
            return Err(ProductionError::InvalidTimeline);
        }
        Ok(())
    }
}

/// Frozen facts supplied to a DAW Handoff implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffRequest {
    project_id: String,
    project_name: String,
    source_project_revision: u64,
    selection_id: SelectionId,
    candidate_id: CandidateId,
    candidate_label: String,
    brief_summary: String,
    asset: AssetVersion,
    tempo_hint_bpm: Option<u16>,
    key_hint: Option<String>,
    markers_micros: Vec<u64>,
}

impl HandoffRequest {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        project_id: String,
        project_name: String,
        source_project_revision: u64,
        selection_id: SelectionId,
        candidate_id: CandidateId,
        candidate_label: String,
        brief_summary: String,
        asset: AssetVersion,
        tempo_hint_bpm: Option<u16>,
        key_hint: Option<String>,
        markers_micros: Vec<u64>,
    ) -> Self {
        Self {
            project_id,
            project_name,
            source_project_revision,
            selection_id,
            candidate_id,
            candidate_label,
            brief_summary,
            asset,
            tempo_hint_bpm,
            key_hint,
            markers_micros,
        }
    }

    #[must_use]
    pub const fn source_project_revision(&self) -> u64 {
        self.source_project_revision
    }

    #[must_use]
    pub const fn selection_id(&self) -> &SelectionId {
        &self.selection_id
    }

    #[must_use]
    pub const fn asset(&self) -> &AssetVersion {
        &self.asset
    }

    #[must_use]
    pub const fn tempo_hint_bpm(&self) -> Option<u16> {
        self.tempo_hint_bpm
    }

    #[must_use]
    pub fn key_hint(&self) -> Option<&str> {
        self.key_hint.as_deref()
    }

    #[must_use]
    pub fn markers_micros(&self) -> &[u64] {
        &self.markers_micros
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffFile {
    pub relative_path: String,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffExportDraft {
    pub relative_path: String,
    pub manifest_sha256: String,
    pub files: Vec<HandoffFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffExport {
    id: HandoffExportId,
    source_project_revision: u64,
    selection_id: SelectionId,
    relative_path: String,
    manifest_sha256: String,
    files: Vec<HandoffFile>,
}

impl HandoffExport {
    pub(crate) fn parse(
        request: &HandoffRequest,
        draft: HandoffExportDraft,
    ) -> Result<Self, ProductionError> {
        validate_export_path(&draft.relative_path)?;
        validate_sha256(&draft.manifest_sha256)?;
        if draft.files.is_empty() {
            return Err(ProductionError::EmptyHandoff);
        }
        for file in &draft.files {
            validate_handoff_file_path(&file.relative_path)?;
            validate_sha256(&file.sha256)?;
            if file.media_type.trim().is_empty() {
                return Err(ProductionError::InvalidHandoffFile);
            }
        }
        Ok(Self {
            id: HandoffExportId::new(),
            source_project_revision: request.source_project_revision,
            selection_id: request.selection_id.clone(),
            relative_path: draft.relative_path,
            manifest_sha256: draft.manifest_sha256,
            files: draft.files,
        })
    }

    #[must_use]
    pub const fn source_project_revision(&self) -> u64 {
        self.source_project_revision
    }

    #[must_use]
    pub(crate) const fn id(&self) -> &HandoffExportId {
        &self.id
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn validate_restored(&self, project_revision: u64) -> Result<(), ProductionError> {
        HandoffExportId::parse(&self.id.as_str())?;
        SelectionId::parse(&self.selection_id.as_str())?;
        if self.source_project_revision > project_revision {
            return Err(ProductionError::InvalidHandoffFile);
        }
        validate_export_path(&self.relative_path)?;
        validate_sha256(&self.manifest_sha256)?;
        if self.files.is_empty() {
            return Err(ProductionError::EmptyHandoff);
        }
        for file in &self.files {
            validate_handoff_file_path(&file.relative_path)?;
            validate_sha256(&file.sha256)?;
            if file.media_type.trim().is_empty() {
                return Err(ProductionError::InvalidHandoffFile);
            }
        }
        Ok(())
    }
}

fn validate_export_path(value: &str) -> Result<(), ProductionError> {
    let path = Path::new(value);
    if is_relative_normal_path(path) && path.starts_with("exports") {
        Ok(())
    } else {
        Err(ProductionError::UnsafeHandoffPath)
    }
}

fn validate_handoff_file_path(value: &str) -> Result<(), ProductionError> {
    if is_relative_normal_path(Path::new(value)) {
        Ok(())
    } else {
        Err(ProductionError::UnsafeHandoffPath)
    }
}

fn is_relative_normal_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_asset_path(value: &str) -> Result<(), ProductionError> {
    let path = Path::new(value);
    let is_safe = !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path.starts_with("assets");
    if is_safe {
        Ok(())
    } else {
        Err(ProductionError::UnsafeAssetPath)
    }
}

fn validate_sha256(value: &str) -> Result<(), ProductionError> {
    let hash = value
        .strip_prefix("sha256:")
        .ok_or(ProductionError::InvalidAssetHash)?;
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ProductionError::InvalidAssetHash)
    }
}

fn validate_audio(audio: &AudioMetadata) -> Result<(), ProductionError> {
    if !matches!(audio.sample_rate_hz, 44_100 | 48_000)
        || !matches!(audio.channels, 1 | 2)
        || audio.duration_micros == 0
        || !matches!(audio.bit_depth, 16 | 24 | 32)
    {
        return Err(ProductionError::InvalidAudioMetadata);
    }
    Ok(())
}

fn validate_provenance(provenance: &ProvenanceRecord) -> Result<(), ProductionError> {
    if provenance.provider_kind.trim().is_empty()
        || provenance.model.trim().is_empty()
        || provenance.adapter_version.trim().is_empty()
        || provenance.input_hash.trim().is_empty()
    {
        return Err(ProductionError::IncompleteProvenance);
    }
    Ok(())
}
