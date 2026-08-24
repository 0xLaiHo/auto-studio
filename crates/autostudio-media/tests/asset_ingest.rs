use autostudio_core::production::{ProvenanceRecord, RightsDeclaration};
use autostudio_media::ProjectMedia;

#[test]
fn verified_staging_wav_is_committed_as_an_immutable_project_asset() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("night-drive.autostudio");
    let staging = temp.path().join("provider-staging");
    std::fs::create_dir_all(&staging).expect("staging directory");
    let source = staging.join("provider-output.wav");
    write_fixture_wav(&source);
    let media = ProjectMedia::new(&package, &staging).expect("Project media");

    let asset = media
        .commit_generated_audio(
            &source,
            ProvenanceRecord {
                provider_kind: "fake-music".to_owned(),
                model: "deterministic-v1".to_owned(),
                adapter_version: "0.1.0".to_owned(),
                external_job_id: Some("job-42".to_owned()),
                input_hash: "sha256:brief-1".to_owned(),
                rights: RightsDeclaration::CreatorOwned,
                credits: vec![],
            },
        )
        .expect("commit audio");

    assert!(asset.relative_path.starts_with("assets/"));
    assert!(asset.sha256.starts_with("sha256:"));
    assert_eq!(asset.sha256.len(), 71);
    assert_eq!(asset.media_type, "audio/wav");
    assert_eq!(asset.audio.sample_rate_hz, 48_000);
    assert_eq!(asset.audio.channels, 2);
    assert_eq!(asset.audio.bit_depth, 24);
    assert_eq!(asset.audio.duration_micros, 100_000);
    assert!(package.join(&asset.relative_path).is_file());
}

#[cfg(unix)]
#[test]
fn staging_symlink_cannot_escape_the_provider_staging_directory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("night-drive.autostudio");
    let staging = temp.path().join("provider-staging");
    std::fs::create_dir_all(&staging).expect("staging directory");
    let outside = temp.path().join("outside.wav");
    write_fixture_wav(&outside);
    let linked = staging.join("linked.wav");
    symlink(&outside, &linked).expect("staging symlink");
    let media = ProjectMedia::new(&package, &staging).expect("Project media");

    let error = media
        .commit_generated_audio(
            &linked,
            ProvenanceRecord {
                provider_kind: "fake-music".to_owned(),
                model: "deterministic-v1".to_owned(),
                adapter_version: "0.1.0".to_owned(),
                external_job_id: Some("job-escape".to_owned()),
                input_hash: "sha256:brief-escape".to_owned(),
                rights: RightsDeclaration::CreatorOwned,
                credits: vec![],
            },
        )
        .expect_err("symlink escape must be rejected");
    assert!(matches!(
        error,
        autostudio_media::MediaError::StagingPathEscape
    ));
}

#[test]
fn existing_content_addressed_asset_is_reverified_before_reuse() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("hash-recheck.autostudio");
    let staging = temp.path().join("provider-staging");
    std::fs::create_dir_all(&staging).expect("staging directory");
    let source = staging.join("provider-output.wav");
    write_fixture_wav(&source);
    let media = ProjectMedia::new(&package, &staging).expect("Project media");
    let provenance = ProvenanceRecord {
        provider_kind: "contract-music".to_owned(),
        model: "contract-v1".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        external_job_id: Some("job-hash".to_owned()),
        input_hash: "sha256:brief-hash".to_owned(),
        rights: RightsDeclaration::CreatorOwned,
        credits: vec![],
    };

    let first = media
        .commit_generated_audio(&source, provenance.clone())
        .expect("first commit");
    std::fs::write(package.join(first.relative_path), b"tampered").expect("tamper committed asset");

    let error = media
        .commit_generated_audio(&source, provenance)
        .expect_err("a corrupt existing content-addressed file must not be reused");
    assert!(matches!(
        error,
        autostudio_media::MediaError::AssetHashMismatch
    ));
}

fn write_fixture_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("fixture WAV");
    for frame in 0..4_800 {
        let sample = if frame % 96 < 48 { 100_000 } else { -100_000 };
        writer.write_sample(sample).expect("left sample");
        writer.write_sample(sample).expect("right sample");
    }
    writer.finalize().expect("finalize fixture WAV");
}
