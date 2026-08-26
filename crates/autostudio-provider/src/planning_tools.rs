//! Fixed, internal Tool Module for the CM-1 planning vertical slice.

use autostudio_core::agent::{
    AgentDecision, AgentPlanDraft, CostEstimate, GenerationIntent, InferenceProvenance,
    InferenceUsage,
};
use autostudio_core::context::{CanonicalToolDefinition, InferenceItemDraft};
use autostudio_core::project::Project;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::constants::{
    EMPTY_OBJECT_SCHEMA_JSON, PLAN_SCHEMA_JSON, PLAN_TOOL_DESCRIPTION, PLAN_TOOL_NAME,
    PROJECT_DESCRIBE_TOOL_DESCRIPTION, PROJECT_DESCRIBE_TOOL_NAME,
};
use crate::context::{CompletedToolResult, ContextProjection, PendingToolRequest};
use crate::error::PlanningToolError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct PlanArguments {
    visible_summary: String,
    generation_prompt: String,
    duration_seconds: u32,
    candidate_count: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFacts<'a> {
    project_id: String,
    project_name: &'a str,
    project_revision: u64,
    brief: Option<&'a autostudio_core::project::CreativeBrief>,
    agent_run_count: usize,
    candidate_count: usize,
    has_selection: bool,
}

pub(crate) fn catalog(
    _projection: &ContextProjection,
) -> Result<Vec<CanonicalToolDefinition>, PlanningToolError> {
    Ok(vec![
        definition(
            PROJECT_DESCRIBE_TOOL_NAME,
            PROJECT_DESCRIBE_TOOL_DESCRIPTION,
            EMPTY_OBJECT_SCHEMA_JSON,
        )?,
        definition(PLAN_TOOL_NAME, PLAN_TOOL_DESCRIPTION, PLAN_SCHEMA_JSON)?,
    ])
}

pub(crate) fn execute(
    project: &Project,
    projection: &ContextProjection,
    request: &PendingToolRequest,
) -> CompletedToolResult {
    let execution_id = Some(Uuid::new_v4().to_string());
    match request.name.as_str() {
        PROJECT_DESCRIBE_TOOL_NAME => {
            if let Err(error) = parse_empty_arguments(&request.arguments_json) {
                return CompletedToolResult {
                    call_id: request.call_id.clone(),
                    name: request.name.clone(),
                    content: format!("{{\"error\":{}}}", json_string(&error.to_string())),
                    is_error: true,
                    execution_id,
                };
            }
            let facts = serde_json::to_string(&ProjectFacts {
                project_id: project.id().as_str(),
                project_name: project.name().as_str(),
                project_revision: project.revision(),
                brief: project.brief(),
                agent_run_count: project.agent_runs().len(),
                candidate_count: project.candidates().len(),
                has_selection: project.selection().is_some(),
            });
            match facts {
                Ok(content) => CompletedToolResult {
                    call_id: request.call_id.clone(),
                    name: request.name.clone(),
                    content,
                    is_error: false,
                    execution_id,
                },
                Err(error) => CompletedToolResult {
                    call_id: request.call_id.clone(),
                    name: request.name.clone(),
                    content: format!("{{\"error\":{}}}", json_string(&error.to_string())),
                    is_error: true,
                    execution_id,
                },
            }
        }
        PLAN_TOOL_NAME if successful_result(projection, PROJECT_DESCRIBE_TOOL_NAME).is_none() => {
            CompletedToolResult {
                call_id: request.call_id.clone(),
                name: request.name.clone(),
                content: format!(
                    "{{\"accepted\":false,\"error\":{}}}",
                    json_string("project_describe must complete before plan submission")
                ),
                is_error: true,
                execution_id,
            }
        }
        PLAN_TOOL_NAME => match parse_plan_arguments(&request.arguments_json) {
            Ok(_) => CompletedToolResult {
                call_id: request.call_id.clone(),
                name: request.name.clone(),
                content: "{\"accepted\":true}".to_owned(),
                is_error: false,
                execution_id,
            },
            Err(error) => CompletedToolResult {
                call_id: request.call_id.clone(),
                name: request.name.clone(),
                content: format!(
                    "{{\"accepted\":false,\"error\":{}}}",
                    json_string(&error.to_string())
                ),
                is_error: true,
                execution_id,
            },
        },
        _ => CompletedToolResult {
            call_id: request.call_id.clone(),
            name: request.name.clone(),
            content: format!(
                "{{\"error\":{}}}",
                json_string(&PlanningToolError::UnknownTool(request.name.clone()).to_string())
            ),
            is_error: true,
            execution_id,
        },
    }
}

pub(crate) fn completed_plan(
    projection: &ContextProjection,
) -> Result<Option<AgentPlanDraft>, PlanningToolError> {
    let Some((request, _result)) = successful_result(projection, PLAN_TOOL_NAME) else {
        return Ok(None);
    };
    let arguments = parse_plan_arguments(&request.arguments_json)?;
    let manifest = projection
        .manifests()
        .iter()
        .find(|manifest| manifest.turn_id() == &request.turn_id)
        .ok_or_else(|| {
            PlanningToolError::InconsistentState(
                "plan Tool Request has no Context Manifest".to_owned(),
            )
        })?;
    let binding = manifest.provider_binding();
    let response_id = projection.items().iter().find_map(|item| {
        if item.turn_id() != &request.turn_id {
            return None;
        }
        match item.payload() {
            InferenceItemDraft::Finish {
                reason: autostudio_core::context::InferenceFinishReason::Completed,
                detail,
            } => detail.clone(),
            _ => None,
        }
    });
    let mut usage = InferenceUsage::default();
    for item in projection.items() {
        if let InferenceItemDraft::Usage { usage: turn } = item.payload() {
            usage.input_tokens = sum_optional(usage.input_tokens, turn.input_tokens)?;
            usage.output_tokens = sum_optional(usage.output_tokens, turn.output_tokens)?;
            usage.actual_cost_minor_units =
                sum_optional(usage.actual_cost_minor_units, turn.actual_cost_minor_units)?;
            match (&usage.currency, &turn.currency) {
                (None, Some(currency)) => usage.currency = Some(currency.clone()),
                (Some(left), Some(right)) if left != right => {
                    return Err(PlanningToolError::InconsistentState(
                        "Inference usage currencies changed inside one Run".to_owned(),
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(Some(AgentPlanDraft {
        visible_summary: arguments.visible_summary,
        decision: AgentDecision::GenerateMusic(GenerationIntent {
            prompt: arguments.generation_prompt,
            duration_seconds: arguments.duration_seconds,
            candidate_count: arguments.candidate_count,
        }),
        estimated_cost: CostEstimate::Unknown,
        usage,
        inference: InferenceProvenance {
            provider_kind: binding.provider_kind.clone(),
            model: binding.model.clone(),
            thinking_level: binding.thinking_level,
            thinking_control: binding.thinking_control,
            thinking_budget_tokens: binding.thinking_budget_tokens,
            capability_revision: binding.capability_revision.clone(),
            mapping_revision: binding.mapping_revision.clone(),
            protocol: binding.protocol.clone(),
            response_id,
        },
        input_hash: manifest.content_hash().to_owned(),
    }))
}

fn successful_result<'a>(
    projection: &'a ContextProjection,
    name: &str,
) -> Option<(PendingToolRequest, &'a InferenceItemDraft)> {
    projection.items().iter().find_map(|item| {
        let InferenceItemDraft::ToolResult {
            call_id,
            name: result_name,
            is_error: false,
            ..
        } = item.payload()
        else {
            return None;
        };
        if result_name != name {
            return None;
        }
        projection.items().iter().find_map(|request_item| {
            let InferenceItemDraft::ToolRequest {
                call_id: request_call_id,
                name: request_name,
                arguments_json,
                descriptor_fingerprint,
            } = request_item.payload()
            else {
                return None;
            };
            (request_call_id == call_id && request_name == name).then(|| {
                let request = PendingToolRequest {
                    turn_id: request_item.turn_id().clone(),
                    call_id: request_call_id.clone(),
                    name: request_name.clone(),
                    arguments_json: arguments_json.clone(),
                    descriptor_fingerprint: descriptor_fingerprint.clone(),
                };
                (request, item.payload())
            })
        })
    })
}

fn parse_plan_arguments(value: &str) -> Result<PlanArguments, PlanningToolError> {
    let arguments: PlanArguments = serde_json::from_str(value)
        .map_err(|error| PlanningToolError::InvalidArguments(error.to_string()))?;
    if arguments.visible_summary.trim().is_empty()
        || arguments.generation_prompt.trim().is_empty()
        || !(1..=900).contains(&arguments.duration_seconds)
        || !(1..=4).contains(&arguments.candidate_count)
    {
        return Err(PlanningToolError::InvalidArguments(
            "summary/prompt, duration, or Candidate count is outside the contract".to_owned(),
        ));
    }
    Ok(arguments)
}

fn parse_empty_arguments(value: &str) -> Result<(), PlanningToolError> {
    let arguments: serde_json::Map<String, serde_json::Value> = serde_json::from_str(value)
        .map_err(|error| PlanningToolError::InvalidArguments(error.to_string()))?;
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(PlanningToolError::InvalidArguments(
            "project_describe does not accept arguments".to_owned(),
        ))
    }
}

fn definition(
    name: &str,
    description: &str,
    schema: &str,
) -> Result<CanonicalToolDefinition, PlanningToolError> {
    let bytes = serde_json::to_vec(&(name, description, schema))
        .map_err(|error| PlanningToolError::InconsistentState(error.to_string()))?;
    CanonicalToolDefinition::new(
        name,
        description,
        schema,
        format!("sha256:{:x}", Sha256::digest(bytes)),
    )
    .map_err(|error| PlanningToolError::InconsistentState(error.to_string()))
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"Tool error\"".to_owned())
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Result<Option<u64>, PlanningToolError> {
    match (left, right) {
        (Some(left), Some(right)) => left.checked_add(right).map(Some).ok_or_else(|| {
            PlanningToolError::InconsistentState("Inference usage overflowed".to_owned())
        }),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}
