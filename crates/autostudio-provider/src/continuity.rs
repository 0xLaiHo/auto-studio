//! Encrypted, Project-external storage for Provider-private continuity state.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use autostudio_core::agent::AgentRunId;
use autostudio_core::context::InferenceTurnId;
use autostudio_core::continuity::{ContinuityBinding, ContinuityReference, ContinuityStateId};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::ContinuityVaultError;
use crate::constants::{
    CONTINUITY_ANTHROPIC_MESSAGES_FORMAT, CONTINUITY_KEY_BYTES, CONTINUITY_NONCE_BYTES,
    CONTINUITY_OPENAI_RESPONSES_FORMAT, CONTINUITY_VAULT_SCHEMA, MAX_CONTINUITY_FILE_BYTES,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityFormat {
    OpenAiResponses,
    AnthropicMessages,
}

impl ContinuityFormat {
    #[must_use]
    pub const fn revision(self) -> &'static str {
        match self {
            Self::OpenAiResponses => CONTINUITY_OPENAI_RESPONSES_FORMAT,
            Self::AnthropicMessages => CONTINUITY_ANTHROPIC_MESSAGES_FORMAT,
        }
    }
}

/// Secret Provider payload. Debug and serialization are intentionally unavailable.
pub struct ProviderContinuityState {
    format: ContinuityFormat,
    payload: Zeroizing<Vec<u8>>,
}

impl ProviderContinuityState {
    /// Creates a secret state from a complete Provider JSON array.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] when the payload is not a non-empty JSON array.
    pub fn from_json(
        format: ContinuityFormat,
        value: &Value,
    ) -> Result<Self, ContinuityVaultError> {
        if value.as_array().is_none_or(Vec::is_empty) {
            return Err(ContinuityVaultError::Corrupt);
        }
        Ok(Self {
            format,
            payload: Zeroizing::new(serde_json::to_vec(value)?),
        })
    }

    fn from_bytes(
        format: ContinuityFormat,
        payload: Vec<u8>,
    ) -> Result<Self, ContinuityVaultError> {
        let value: Value = serde_json::from_slice(&payload)?;
        if value.as_array().is_none_or(Vec::is_empty) {
            return Err(ContinuityVaultError::Corrupt);
        }
        Ok(Self {
            format,
            payload: Zeroizing::new(payload),
        })
    }

    #[must_use]
    pub const fn format(&self) -> ContinuityFormat {
        self.format
    }

    pub(crate) fn json(&self) -> Result<Value, ContinuityVaultError> {
        serde_json::from_slice(&self.payload).map_err(Into::into)
    }

    fn bytes(&self) -> &[u8] {
        &self.payload
    }
}

impl Clone for ProviderContinuityState {
    fn clone(&self) -> Self {
        Self {
            format: self.format,
            payload: Zeroizing::new(self.payload.to_vec()),
        }
    }
}

impl fmt::Debug for ProviderContinuityState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderContinuityState")
            .field("format", &self.format)
            .field("payload", &"[REDACTED]")
            .finish()
    }
}

pub struct LoadedContinuity {
    pub reference: ContinuityReference,
    pub state: ProviderContinuityState,
}

impl fmt::Debug for LoadedContinuity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedContinuity")
            .field("reference", &self.reference)
            .field("state", &self.state)
            .finish()
    }
}

/// Run-scoped secret storage seam. Implementations must remain outside Project packages.
pub trait ContinuityVault: Send + Sync {
    /// Returns compatible, unexpired state or `None` after purging stale/incompatible state.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] when validation, storage, or decryption fails.
    fn load(
        &self,
        binding: &ContinuityBinding,
        now_unix_millis: u64,
    ) -> Result<Option<LoadedContinuity>, ContinuityVaultError>;

    /// Atomically replaces the latest state for one Run.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] when validation, encryption, or publication fails.
    fn store(
        &self,
        binding: &ContinuityBinding,
        source_turn_id: &InferenceTurnId,
        state: &ProviderContinuityState,
        now_unix_millis: u64,
    ) -> Result<ContinuityReference, ContinuityVaultError>;

    /// Deletes every private continuity byte associated with a terminal Run.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] when the entry cannot be removed durably.
    fn purge_run(&self, run_id: &AgentRunId) -> Result<(), ContinuityVaultError>;

    /// Removes all expired entries and returns the number deleted.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] when the Vault cannot be scanned or cleaned.
    fn purge_expired(&self, now_unix_millis: u64) -> Result<usize, ContinuityVaultError>;
}

/// Test-only compatibility fixture for contracts that never emit private state.
#[cfg(any(test, debug_assertions))]
#[doc(hidden)]
pub struct DisabledContinuityVault;

#[cfg(any(test, debug_assertions))]
impl ContinuityVault for DisabledContinuityVault {
    fn load(
        &self,
        _binding: &ContinuityBinding,
        _now_unix_millis: u64,
    ) -> Result<Option<LoadedContinuity>, ContinuityVaultError> {
        Ok(None)
    }

    fn store(
        &self,
        _binding: &ContinuityBinding,
        _source_turn_id: &InferenceTurnId,
        _state: &ProviderContinuityState,
        _now_unix_millis: u64,
    ) -> Result<ContinuityReference, ContinuityVaultError> {
        Err(ContinuityVaultError::Crypto)
    }

    fn purge_run(&self, _run_id: &AgentRunId) -> Result<(), ContinuityVaultError> {
        Ok(())
    }

    fn purge_expired(&self, _now_unix_millis: u64) -> Result<usize, ContinuityVaultError> {
        Ok(0)
    }
}

pub struct FileContinuityVault {
    root: PathBuf,
    key_path: PathBuf,
    ttl_millis: u64,
}

impl FileContinuityVault {
    /// Opens a Vault after proving its payload and key paths are outside one Project Package.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError::InsideProject`] when either path resolves
    /// inside the Project Package, including through an existing symlinked parent.
    pub fn open_for_project(
        root: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        project_root: impl AsRef<Path>,
        ttl_millis: u64,
    ) -> Result<Self, ContinuityVaultError> {
        let root = root.as_ref();
        let key_path = key_path.as_ref();
        let project_root = project_root.as_ref().canonicalize()?;
        let resolved_root = resolve_with_existing_ancestor(root)?;
        let resolved_key = resolve_with_existing_ancestor(key_path)?;
        reject_resolved_project_child(&resolved_root, &project_root)?;
        reject_resolved_project_child(&resolved_key, &project_root)?;
        Self::open(root, key_path, ttl_millis)
    }

    /// Opens or creates a private Vault and its separate local master-key file.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError`] for unsafe paths, permissions, or key material.
    pub fn open(
        root: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        ttl_millis: u64,
    ) -> Result<Self, ContinuityVaultError> {
        if ttl_millis == 0 {
            return Err(ContinuityVaultError::InvalidClock);
        }
        let root = root.as_ref();
        fs::create_dir_all(root)?;
        set_private_directory_permissions(root)?;
        ensure_private_directory(root)?;
        let root = root.canonicalize()?;
        let key_path = key_path.as_ref().to_path_buf();
        let key_parent = key_path
            .parent()
            .ok_or(ContinuityVaultError::MissingParent)?;
        fs::create_dir_all(key_parent)?;
        let vault = Self {
            root,
            key_path,
            ttl_millis,
        };
        let lock = vault.lock()?;
        let result = vault.ensure_key();
        FileExt::unlock(&lock)?;
        result?;
        Ok(vault)
    }

    /// Current wall-clock value used by the production composition root.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuityVaultError::InvalidClock`] before the Unix epoch.
    pub fn now_unix_millis() -> Result<u64, ContinuityVaultError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ContinuityVaultError::InvalidClock)?
            .as_millis();
        u64::try_from(millis).map_err(|_| ContinuityVaultError::InvalidClock)
    }

    fn state_path(&self, run_id: &AgentRunId) -> PathBuf {
        self.root.join(format!("{}.continuity", run_id.as_str()))
    }

    fn lock(&self) -> Result<File, ContinuityVaultError> {
        let path = self.root.join("vault.lock");
        let file = open_private_file(&path, false, false)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn ensure_key(&self) -> Result<(), ContinuityVaultError> {
        if self.key_path.exists() {
            let _ = self.read_key()?;
            return Ok(());
        }
        let mut key = Zeroizing::new(vec![0_u8; CONTINUITY_KEY_BYTES]);
        getrandom::fill(&mut key).map_err(|_| ContinuityVaultError::Crypto)?;
        let temporary = self
            .key_path
            .with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
        let mut file = open_private_file(&temporary, false, true)?;
        file.write_all(&key)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, &self.key_path)?;
        set_private_file_permissions(&self.key_path)?;
        sync_directory(
            self.key_path
                .parent()
                .ok_or(ContinuityVaultError::MissingParent)?,
        )
    }

    fn read_key(&self) -> Result<Zeroizing<Vec<u8>>, ContinuityVaultError> {
        ensure_private_regular_file(&self.key_path)?;
        let mut file = OpenOptions::new().read(true).open(&self.key_path)?;
        let mut key = Zeroizing::new(Vec::with_capacity(CONTINUITY_KEY_BYTES));
        file.read_to_end(&mut key)?;
        if key.len() != CONTINUITY_KEY_BYTES {
            return Err(ContinuityVaultError::Corrupt);
        }
        Ok(key)
    }

    fn read_envelope(path: &Path) -> Result<StoredContinuityEnvelope, ContinuityVaultError> {
        ensure_private_regular_file(path)?;
        let metadata = fs::metadata(path)?;
        if metadata.len() > MAX_CONTINUITY_FILE_BYTES {
            return Err(ContinuityVaultError::FileTooLarge);
        }
        let capacity =
            usize::try_from(metadata.len()).map_err(|_| ContinuityVaultError::FileTooLarge)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(capacity));
        OpenOptions::new()
            .read(true)
            .open(path)?
            .read_to_end(&mut bytes)?;
        let envelope: StoredContinuityEnvelope = serde_json::from_slice(&bytes)?;
        envelope.validate()?;
        Ok(envelope)
    }

    fn remove_if_exists(path: &Path) -> Result<bool, ContinuityVaultError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

impl ContinuityVault for FileContinuityVault {
    fn load(
        &self,
        binding: &ContinuityBinding,
        now_unix_millis: u64,
    ) -> Result<Option<LoadedContinuity>, ContinuityVaultError> {
        binding.validate()?;
        let path = self.state_path(binding.run_id());
        let lock = self.lock()?;
        let result = (|| {
            if !path.exists() {
                return Ok(None);
            }
            let envelope = match Self::read_envelope(&path) {
                Ok(envelope) => envelope,
                Err(error) => {
                    let _ = Self::remove_if_exists(&path);
                    return Err(error);
                }
            };
            if envelope.expires_at_unix_millis <= now_unix_millis {
                Self::remove_if_exists(&path)?;
                return Ok(None);
            }
            let expected_hash = binding.binding_hash()?;
            if envelope.binding_hash != expected_hash || &envelope.binding != binding {
                Self::remove_if_exists(&path)?;
                return Ok(None);
            }
            let state = match self.decrypt_state(&envelope) {
                Ok(state) => state,
                Err(error) => {
                    Self::remove_if_exists(&path)?;
                    return Err(error);
                }
            };
            let reference = envelope.reference()?;
            Ok(Some(LoadedContinuity { reference, state }))
        })();
        FileExt::unlock(&lock)?;
        result
    }

    fn store(
        &self,
        binding: &ContinuityBinding,
        source_turn_id: &InferenceTurnId,
        state: &ProviderContinuityState,
        now_unix_millis: u64,
    ) -> Result<ContinuityReference, ContinuityVaultError> {
        binding.validate()?;
        let expires_at_unix_millis = now_unix_millis
            .checked_add(self.ttl_millis)
            .ok_or(ContinuityVaultError::InvalidClock)?;
        let state_id = ContinuityStateId::new();
        let binding_hash = binding.binding_hash()?;
        let mut nonce = vec![0_u8; CONTINUITY_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| ContinuityVaultError::Crypto)?;
        let mut envelope = StoredContinuityEnvelope {
            schema_version: CONTINUITY_VAULT_SCHEMA.to_owned(),
            state_id,
            binding: binding.clone(),
            binding_hash,
            source_turn_id: source_turn_id.clone(),
            created_at_unix_millis: now_unix_millis,
            expires_at_unix_millis,
            nonce,
            ciphertext: Vec::new(),
        };
        let mut plaintext = Zeroizing::new(serde_json::to_vec(&SecretContinuityDocument {
            format: state.format(),
            format_revision: state.format().revision().to_owned(),
            payload: state.bytes().to_vec(),
        })?);
        let key = self.read_key()?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let aad = envelope.aad()?;
        envelope.ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&envelope.nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| ContinuityVaultError::Crypto)?;
        plaintext.zeroize();
        let path = self.state_path(binding.run_id());
        let lock = self.lock()?;
        let result = (|| {
            let temporary = self.root.join(format!(".{}.tmp", Uuid::new_v4().simple()));
            let mut file = open_private_file(&temporary, false, true)?;
            let mut bytes = Zeroizing::new(serde_json::to_vec(&envelope)?);
            file.write_all(&bytes)?;
            file.sync_all()?;
            bytes.zeroize();
            drop(file);
            replace_file(&temporary, &path)?;
            set_private_file_permissions(&path)?;
            sync_directory(&self.root)?;
            envelope.reference()
        })();
        FileExt::unlock(&lock)?;
        result
    }

    fn purge_run(&self, run_id: &AgentRunId) -> Result<(), ContinuityVaultError> {
        let lock = self.lock()?;
        Self::remove_if_exists(&self.state_path(run_id))?;
        sync_directory(&self.root)?;
        FileExt::unlock(&lock)?;
        Ok(())
    }

    fn purge_expired(&self, now_unix_millis: u64) -> Result<usize, ContinuityVaultError> {
        let lock = self.lock()?;
        let result = (|| {
            let mut removed = 0;
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("continuity") {
                    continue;
                }
                let expired = match Self::read_envelope(&path) {
                    Ok(envelope) => envelope.expires_at_unix_millis <= now_unix_millis,
                    Err(_) => true,
                };
                if expired && Self::remove_if_exists(&path)? {
                    removed += 1;
                }
            }
            if removed > 0 {
                sync_directory(&self.root)?;
            }
            Ok(removed)
        })();
        FileExt::unlock(&lock)?;
        result
    }
}

impl FileContinuityVault {
    fn decrypt_state(
        &self,
        envelope: &StoredContinuityEnvelope,
    ) -> Result<ProviderContinuityState, ContinuityVaultError> {
        let key = self.read_key()?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = XNonce::from_slice(&envelope.nonce);
        let aad = envelope.aad()?;
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &envelope.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| ContinuityVaultError::Corrupt)?;
        let mut plaintext = Zeroizing::new(plaintext);
        let document: SecretContinuityDocument =
            serde_json::from_slice(&plaintext).map_err(|_| ContinuityVaultError::Corrupt)?;
        plaintext.zeroize();
        if document.format_revision != document.format.revision() {
            return Err(ContinuityVaultError::UnsupportedSchema);
        }
        ProviderContinuityState::from_bytes(document.format, document.payload.clone())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretContinuityDocument {
    format: ContinuityFormat,
    format_revision: String,
    payload: Vec<u8>,
}

impl Drop for SecretContinuityDocument {
    fn drop(&mut self) {
        self.payload.zeroize();
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredContinuityEnvelope {
    schema_version: String,
    state_id: ContinuityStateId,
    binding: ContinuityBinding,
    binding_hash: String,
    source_turn_id: InferenceTurnId,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl StoredContinuityEnvelope {
    fn validate(&self) -> Result<(), ContinuityVaultError> {
        if self.schema_version != CONTINUITY_VAULT_SCHEMA {
            return Err(ContinuityVaultError::UnsupportedSchema);
        }
        self.binding.validate()?;
        if self.binding.binding_hash()? != self.binding_hash {
            return Err(ContinuityVaultError::Corrupt);
        }
        if self.nonce.len() != CONTINUITY_NONCE_BYTES || self.ciphertext.is_empty() {
            return Err(ContinuityVaultError::Corrupt);
        }
        ContinuityReference::new(
            self.state_id.clone(),
            self.source_turn_id.clone(),
            self.binding_hash.clone(),
            self.created_at_unix_millis,
            self.expires_at_unix_millis,
        )?;
        Ok(())
    }

    fn aad(&self) -> Result<Vec<u8>, ContinuityVaultError> {
        Ok(serde_json::to_vec(&ContinuityEnvelopeAad {
            schema_version: &self.schema_version,
            state_id: &self.state_id,
            binding: &self.binding,
            binding_hash: &self.binding_hash,
            source_turn_id: &self.source_turn_id,
            created_at_unix_millis: self.created_at_unix_millis,
            expires_at_unix_millis: self.expires_at_unix_millis,
            nonce: &self.nonce,
        })?)
    }

    fn reference(&self) -> Result<ContinuityReference, ContinuityVaultError> {
        Ok(ContinuityReference::new(
            self.state_id.clone(),
            self.source_turn_id.clone(),
            self.binding_hash.clone(),
            self.created_at_unix_millis,
            self.expires_at_unix_millis,
        )?)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuityEnvelopeAad<'a> {
    schema_version: &'a str,
    state_id: &'a ContinuityStateId,
    binding: &'a ContinuityBinding,
    binding_hash: &'a str,
    source_turn_id: &'a InferenceTurnId,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    nonce: &'a [u8],
}

fn open_private_file(
    path: &Path,
    truncate: bool,
    create_new: bool,
) -> Result<File, ContinuityVaultError> {
    if path.exists() {
        ensure_private_regular_file(path)?;
    }
    let mut options = OpenOptions::new();
    options
        .read(!truncate)
        .write(true)
        .create(!create_new)
        .create_new(create_new)
        .truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    set_private_file_permissions(path)?;
    Ok(file)
}

fn ensure_private_regular_file(path: &Path) -> Result<(), ContinuityVaultError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(ContinuityVaultError::InsecurePermissions);
    }
    ensure_private_permissions(path)
}

fn ensure_private_directory(path: &Path) -> Result<(), ContinuityVaultError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(ContinuityVaultError::InsecurePermissions);
    }
    ensure_private_permissions(path)
}

#[cfg(unix)]
fn ensure_private_permissions(path: &Path) -> Result<(), ContinuityVaultError> {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(ContinuityVaultError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_permissions(_path: &Path) -> Result<(), ContinuityVaultError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), ContinuityVaultError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), ContinuityVaultError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), ContinuityVaultError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), ContinuityVaultError> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), ContinuityVaultError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), ContinuityVaultError> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(source, destination)?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), ContinuityVaultError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn resolve_with_existing_ancestor(candidate: &Path) -> Result<PathBuf, ContinuityVaultError> {
    let absolute = std::path::absolute(candidate)?;
    let mut existing = absolute.as_path();
    let mut missing_components = Vec::new();
    while !existing.try_exists()? {
        missing_components.push(
            existing
                .file_name()
                .ok_or(ContinuityVaultError::MissingParent)?
                .to_os_string(),
        );
        existing = existing
            .parent()
            .ok_or(ContinuityVaultError::MissingParent)?;
    }
    let mut resolved = existing.canonicalize()?;
    for component in missing_components.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn reject_resolved_project_child(
    candidate: &Path,
    project_root: &Path,
) -> Result<(), ContinuityVaultError> {
    if candidate.starts_with(project_root) {
        Err(ContinuityVaultError::InsideProject)
    } else {
        Ok(())
    }
}
