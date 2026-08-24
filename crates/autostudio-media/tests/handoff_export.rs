use std::fs;

use autostudio_core::production::{HandoffRequest, HandoffSink};
use autostudio_media::ProjectMedia;
use sha2::{Digest, Sha256};

#[test]
fn selected_audio_is_published_as_an_idempotent_versioned_handoff_package() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("handoff.autostudio");
    let staging = temp.path().join("staging");
    let assets = package.join("assets");
    fs::create_dir_all(&assets).expect("assets directory");
    let audio = b"RIFF-test-audio";
    fs::write(assets.join("selected.wav"), audio).expect("audio fixture");
    let audio_hash = format!("sha256:{:x}", Sha256::digest(audio));
    let request: HandoffRequest = serde_json::from_value(serde_json::json!({
        "projectId": "project-1",
        "projectName": "Night Drive",
        "sourceProjectRevision": 7,
        "selectionId": "55e04e84-40ae-4a9d-9e30-6d34b7bb9da0",
        "candidateId": "fcd64c6a-39ab-455c-91a3-8d7cbd1d04f0",
        "candidateLabel": "Direction A",
        "briefSummary": "Nocturnal synthwave cue",
        "asset": {
            "assetId": "b5a89a10-66a5-4bc4-b0ce-b714268187b4",
            "id": "a9323568-5541-4649-894b-0b92d796143f",
            "relativePath": "assets/selected.wav",
            "sha256": audio_hash,
            "mediaType": "audio/wav",
            "audio": { "sampleRateHz": 48000, "channels": 2, "durationMicros": 2_000_000, "bitDepth": 24 },
            "provenance": {
                "providerKind": "fake-music",
                "model": "deterministic-v1",
                "adapterVersion": "0.1.0",
                "externalJobId": "job-1",
                "inputHash": "sha256:brief-1",
                "rights": "creator_owned",
                "credits": []
            }
        },
        "tempoHintBpm": null,
        "keyHint": null,
        "markersMicros": []
    }))
    .expect("Handoff request");
    let media = ProjectMedia::new(&package, &staging).expect("Project media");

    let first = media.export(&request).expect("first export");
    let second = media.export(&request).expect("idempotent export");
    assert_eq!(first, second);
    let export_root = package.join(&first.relative_path);
    assert_eq!(
        fs::read(export_root.join("audio/selected.wav")).expect("selected audio"),
        audio
    );
    assert!(export_root.join("README.txt").is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(export_root.join("manifest.json")).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["schemaVersion"], "autostudio.daw-handoff/1");
    assert_eq!(manifest["stems"], serde_json::json!([]));
    assert!(
        manifest["missingCapabilities"]
            .as_array()
            .expect("missing capabilities")
            .iter()
            .any(|value| value == "stems")
    );
    fs::write(export_root.join("README.txt"), b"tampered").expect("tamper handoff");
    let error = media
        .export(&request)
        .expect_err("an existing mismatched handoff must never be overwritten");
    assert!(error.contains("existing DAW Handoff"));
}
