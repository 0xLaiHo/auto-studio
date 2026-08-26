use autostudio_core::context::{CanonicalToolDefinition, ContextError};

const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn model_visible_tool_names_use_the_cross_provider_portable_subset() {
    CanonicalToolDefinition::new(
        "project_describe",
        "Read Project facts",
        r#"{"type":"object"}"#,
        DIGEST,
    )
    .expect("portable Tool name");

    let error = CanonicalToolDefinition::new(
        "project.describe",
        "Read Project facts",
        r#"{"type":"object"}"#,
        DIGEST,
    )
    .expect_err("a Provider-incompatible Tool name must fail before inference");
    assert_eq!(error, ContextError::InvalidToolName);
}
