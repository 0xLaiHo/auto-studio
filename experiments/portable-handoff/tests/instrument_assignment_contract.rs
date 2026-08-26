use autostudio_music_quality::ExperimentalMusicSpec;
use autostudio_portable_handoff::{AssignmentSource, resolve_instrument_assignments};

#[test]
fn resolves_the_pilot_tracks_to_stable_profiles_and_channels() {
    let spec = ExperimentalMusicSpec::parse_and_validate(pilot_fixture()).expect("valid Pilot");

    let manifest = resolve_instrument_assignments(&spec).expect("resolved assignments");

    assert_eq!(
        manifest.schema_version,
        "portable-instrument-assignments-v1"
    );
    assert_eq!(
        manifest.catalog_sha256,
        "ef423d721186c580de6a055e02940c4908bb5973d42020c21317b7033cc7e127"
    );
    assert_eq!(
        manifest.library.sha256,
        "f45b6b4a68b6bf3d792fcbb6d7de24dc701a0f89c5900a21ef3aaece993b839a"
    );
    assert_eq!(manifest.assignments.len(), 3);

    let piano = &manifest.assignments[0];
    assert_eq!(piano.track_id, "piano");
    assert_eq!(piano.profile_id, "gm.acoustic-grand-piano");
    assert_eq!(piano.preset_name, "Stereo Grand");
    assert_eq!((piano.bank_msb, piano.bank_lsb, piano.program), (0, 0, 0));
    assert_eq!(piano.midi_channel, 1);
    assert_eq!(piano.source, AssignmentSource::InstrumentHint);

    let lead = &manifest.assignments[1];
    assert_eq!(lead.profile_id, "gm.square-lead");
    assert_eq!(lead.preset_name, "Square Lead");
    assert_eq!((lead.bank_msb, lead.bank_lsb, lead.program), (0, 0, 80));
    assert_eq!(lead.midi_channel, 2);

    let bass = &manifest.assignments[2];
    assert_eq!(bass.profile_id, "gm.finger-bass");
    assert_eq!(bass.preset_name, "Finger Bass");
    assert_eq!((bass.bank_msb, bass.bank_lsb, bass.program), (0, 0, 33));
    assert_eq!(bass.midi_channel, 3);
}

fn pilot_fixture() -> &'static str {
    include_str!("../../music-quality/evidence/pilot/l1-song-hook/spec.json")
}
