//! Model-scoped thinking capabilities and typed Provider request encoding.

use autostudio_core::provider::{ThinkingCapability, ThinkingControl, ThinkingLevel};
use serde_json::{Value, json};

use crate::constants::{
    LEGACY_THINKING_HIGH_TOKENS, LEGACY_THINKING_MAX_TOKENS, PROVIDER_ANTHROPIC, PROVIDER_DEEPSEEK,
    PROVIDER_KIMI_CODE, PROVIDER_KIMI_OPEN, PROVIDER_OPENAI, THINKING_CAPABILITY_REVISION,
    THINKING_MAPPING_REVISION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveThinking {
    pub level: ThinkingLevel,
    pub control: ThinkingControl,
    pub budget_tokens: Option<u32>,
    pub capability_revision: &'static str,
    pub mapping_revision: &'static str,
}

#[must_use]
pub fn model_capability(provider: &str, model: &str) -> ThinkingCapability {
    let id = model.to_ascii_lowercase();
    match provider {
        PROVIDER_DEEPSEEK => deepseek_capability(&id),
        PROVIDER_OPENAI => openai_capability(&id),
        PROVIDER_ANTHROPIC => anthropic_capability(&id),
        PROVIDER_KIMI_OPEN => kimi_open_capability(&id),
        PROVIDER_KIMI_CODE => kimi_code_capability(&id),
        _ => ThinkingCapability::unsupported(),
    }
}

/// Applies one already-validated model selection to a Provider request body.
/// Output token limits are deliberately owned by the caller and are not
/// derived from this reasoning selection.
pub fn apply_to_request(
    body: &mut Value,
    provider: &str,
    model: &str,
    level: ThinkingLevel,
) -> EffectiveThinking {
    let capability = model_capability(provider, model);
    let effective_level = if level == ThinkingLevel::ProviderDefault || capability.supports(level) {
        level
    } else {
        capability.default_level
    };
    let budget_tokens = if effective_level == ThinkingLevel::ProviderDefault {
        None
    } else {
        match capability.control {
            ThinkingControl::Unsupported => None,
            ThinkingControl::Toggle => {
                apply_toggle(body, effective_level);
                None
            }
            ThinkingControl::Effort => {
                apply_effort(body, provider, effective_level);
                None
            }
            ThinkingControl::AdaptiveEffort => {
                apply_adaptive_effort(body, provider, effective_level);
                None
            }
            ThinkingControl::TokenBudget => {
                let budget = legacy_budget(effective_level);
                if let Some(budget) = budget {
                    body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
                }
                budget
            }
        }
    };
    EffectiveThinking {
        level: effective_level,
        control: capability.control,
        budget_tokens,
        capability_revision: THINKING_CAPABILITY_REVISION,
        mapping_revision: THINKING_MAPPING_REVISION,
    }
}

fn deepseek_capability(id: &str) -> ThinkingCapability {
    if id.contains("deepseek-v4-flash") {
        return capability(
            ThinkingControl::Effort,
            [
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ],
            ThinkingLevel::High,
        );
    }
    if id.contains("deepseek-v4-pro") {
        return capability(
            ThinkingControl::Effort,
            [ThinkingLevel::Off, ThinkingLevel::High, ThinkingLevel::Max],
            ThinkingLevel::High,
        );
    }
    ThinkingCapability::unsupported()
}

fn openai_capability(id: &str) -> ThinkingCapability {
    if id.contains("deep-research") {
        return effort([ThinkingLevel::Medium], ThinkingLevel::Medium);
    }
    if !is_openai_reasoning_model(id) {
        return ThinkingCapability::unsupported();
    }
    if id.contains("-chat") {
        return effort([ThinkingLevel::Medium], ThinkingLevel::Medium);
    }
    if id.contains("-pro") {
        if gpt5_version(id).is_some() {
            return effort(
                [
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                ],
                ThinkingLevel::High,
            );
        }
        return effort([ThinkingLevel::High], ThinkingLevel::High);
    }
    if id.contains("codex") {
        let version = gpt5_version(id);
        if version.is_some_and(|value| value >= 3) {
            return effort(
                [
                    ThinkingLevel::Off,
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                ],
                ThinkingLevel::High,
            );
        }
        if id.contains("codex-max") || version.is_some_and(|value| value >= 2) {
            return effort(
                [
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                ],
                ThinkingLevel::High,
            );
        }
        return effort(
            [
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            ThinkingLevel::High,
        );
    }
    match gpt5_version(id) {
        Some(1) => effort(
            [
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            ThinkingLevel::High,
        ),
        Some(_) => effort(
            [
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
            ],
            ThinkingLevel::High,
        ),
        None if id.contains("gpt-5") => effort(
            [
                ThinkingLevel::Minimal,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            ThinkingLevel::High,
        ),
        None => effort(
            [
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            ThinkingLevel::High,
        ),
    }
}

fn anthropic_capability(id: &str) -> ThinkingCapability {
    if !id.contains("claude") {
        return ThinkingCapability::unsupported();
    }
    if is_modern_claude(id) {
        return capability(
            ThinkingControl::AdaptiveEffort,
            [
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ],
            ThinkingLevel::High,
        );
    }
    if ["4-6", "4.6"].iter().any(|version| id.contains(version)) {
        return capability(
            ThinkingControl::AdaptiveEffort,
            [
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ],
            ThinkingLevel::High,
        );
    }
    capability(
        ThinkingControl::TokenBudget,
        [ThinkingLevel::High, ThinkingLevel::Max],
        ThinkingLevel::High,
    )
}

fn kimi_open_capability(id: &str) -> ThinkingCapability {
    if id.contains("kimi-k3") {
        return effort(
            [ThinkingLevel::Low, ThinkingLevel::High, ThinkingLevel::Max],
            ThinkingLevel::High,
        );
    }
    if id.contains("k2.6") || id.contains("k2-6") {
        return capability(
            ThinkingControl::Toggle,
            [ThinkingLevel::Off, ThinkingLevel::High],
            ThinkingLevel::High,
        );
    }
    ThinkingCapability::unsupported()
}

fn kimi_code_capability(id: &str) -> ThinkingCapability {
    if id == "k3" || id.starts_with("k3-") {
        return capability(
            ThinkingControl::AdaptiveEffort,
            [ThinkingLevel::Low, ThinkingLevel::High, ThinkingLevel::Max],
            ThinkingLevel::High,
        );
    }
    ThinkingCapability::unsupported()
}

fn capability(
    control: ThinkingControl,
    levels: impl IntoIterator<Item = ThinkingLevel>,
    default_level: ThinkingLevel,
) -> ThinkingCapability {
    ThinkingCapability::new(control, levels, default_level)
}

fn effort(
    levels: impl IntoIterator<Item = ThinkingLevel>,
    default_level: ThinkingLevel,
) -> ThinkingCapability {
    capability(ThinkingControl::Effort, levels, default_level)
}

fn is_openai_reasoning_model(id: &str) -> bool {
    id.contains("gpt-5")
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("o4")
        || id.contains("/o1")
        || id.contains("/o3")
        || id.contains("/o4")
}

fn gpt5_version(id: &str) -> Option<u32> {
    let start = id.find("gpt-5.")? + "gpt-5.".len();
    let digits = id[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

fn is_modern_claude(id: &str) -> bool {
    ["4-7", "4.7", "claude-5", "claude-opus-5", "claude-sonnet-5"]
        .iter()
        .any(|version| id.contains(version))
}

fn apply_toggle(body: &mut Value, level: ThinkingLevel) {
    body["thinking"] = json!({
        "type": if level == ThinkingLevel::Off { "disabled" } else { "enabled" }
    });
}

fn apply_effort(body: &mut Value, provider: &str, level: ThinkingLevel) {
    if provider == PROVIDER_DEEPSEEK {
        if level == ThinkingLevel::Off {
            body["thinking"] = json!({"type": "disabled"});
            return;
        }
        body["thinking"] = json!({"type": "enabled"});
    }
    if let Some(effort) = effort_value(level) {
        if provider == PROVIDER_OPENAI {
            body["reasoning"] = json!({"effort": effort, "summary": "auto"});
            body["include"] = json!(["reasoning.encrypted_content"]);
        } else {
            body["reasoning_effort"] = Value::String(effort.to_owned());
        }
    }
}

fn apply_adaptive_effort(body: &mut Value, provider: &str, level: ThinkingLevel) {
    let mut thinking = json!({"type": "adaptive"});
    if provider == PROVIDER_KIMI_CODE {
        thinking["display"] = Value::String("summarized".to_owned());
    }
    body["thinking"] = thinking;
    if let Some(effort) = effort_value(level) {
        body["output_config"] = json!({"effort": effort});
    }
}

fn effort_value(level: ThinkingLevel) -> Option<&'static str> {
    match level {
        ThinkingLevel::ProviderDefault => None,
        ThinkingLevel::Off => Some("none"),
        ThinkingLevel::Minimal => Some("minimal"),
        ThinkingLevel::Low => Some("low"),
        ThinkingLevel::Medium => Some("medium"),
        ThinkingLevel::High => Some("high"),
        ThinkingLevel::XHigh => Some("xhigh"),
        ThinkingLevel::Max => Some("max"),
    }
}

fn legacy_budget(level: ThinkingLevel) -> Option<u32> {
    match level {
        ThinkingLevel::High => Some(LEGACY_THINKING_HIGH_TOKENS),
        ThinkingLevel::Max => Some(LEGACY_THINKING_MAX_TOKENS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_to_request, model_capability};
    use crate::constants::{PROVIDER_ANTHROPIC, PROVIDER_DEEPSEEK, PROVIDER_OPENAI};
    use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
    use serde_json::json;

    #[test]
    fn exact_models_expose_only_verified_levels() {
        let flash = model_capability(PROVIDER_DEEPSEEK, "deepseek-v4-flash");
        assert_eq!(flash.control, ThinkingControl::Effort);
        assert!(flash.supports(ThinkingLevel::Off));
        assert!(flash.supports(ThinkingLevel::Low));
        assert!(!flash.supports(ThinkingLevel::Medium));

        let pro = model_capability(PROVIDER_OPENAI, "gpt-5.2-pro");
        assert_eq!(
            pro.levels,
            vec![
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh
            ]
        );
    }

    #[test]
    fn output_budget_is_not_changed_by_reasoning_mapping() {
        let mut body = json!({"max_output_tokens": 4096});
        let effective =
            apply_to_request(&mut body, PROVIDER_OPENAI, "gpt-5.2", ThinkingLevel::XHigh);
        assert_eq!(body["max_output_tokens"], 4096);
        assert_eq!(body["reasoning"]["effort"], "xhigh");
        assert_eq!(effective.control, ThinkingControl::Effort);
    }

    #[test]
    fn anthropic_legacy_uses_a_real_thinking_budget() {
        let mut body = json!({});
        let effective = apply_to_request(
            &mut body,
            PROVIDER_ANTHROPIC,
            "claude-sonnet-4-5",
            ThinkingLevel::Max,
        );
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16_384);
        assert_eq!(effective.budget_tokens, Some(16_384));
    }
}
