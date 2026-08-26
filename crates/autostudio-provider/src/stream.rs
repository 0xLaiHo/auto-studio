//! Provider-neutral assembly of complete Inference Turns from SSE deltas.

use std::collections::BTreeMap;

use autostudio_core::agent::InferenceUsage;
use autostudio_core::context::CanonicalToolCall;
use serde_json::Value;

use crate::AdapterError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseDecoder {
    bytes: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    /// Accepts an arbitrary transport chunk, including chunks split inside a
    /// UTF-8 code point, and returns every complete SSE event.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when a completed SSE line is not UTF-8.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, AdapterError> {
        self.bytes.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(position) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let mut line = self.bytes.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line)
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?;
            self.consume_line(&line, &mut events);
        }
        Ok(events)
    }

    /// Flushes a final unterminated line and pending SSE event at EOF.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when the final bytes are not UTF-8.
    pub fn finish(mut self) -> Result<Vec<SseEvent>, AdapterError> {
        let mut events = Vec::new();
        if !self.bytes.is_empty() {
            let line = String::from_utf8(std::mem::take(&mut self.bytes))
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?;
            self.consume_line(line.trim_end_matches('\r'), &mut events);
        }
        self.emit(&mut events);
        Ok(events)
    }

    fn consume_line(&mut self, line: &str, events: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.emit(events);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
            (field, value.strip_prefix(' ').unwrap_or(value))
        });
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => self.data.push(value.to_owned()),
            _ => {}
        }
    }

    fn emit(&mut self, events: &mut Vec<SseEvent>) {
        if self.event.is_none() && self.data.is_empty() {
            return;
        }
        events.push(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        });
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InferenceDelta {
    VisibleText(String),
    ToolCall {
        slot: u64,
        call_id: Option<String>,
        name_delta: Option<String>,
        arguments_delta: Option<String>,
    },
    Usage(InferenceUsage),
    ResponseId(String),
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssembledInferenceTurn {
    pub visible_text: Option<String>,
    pub tool_calls: Vec<CanonicalToolCall>,
    pub usage: InferenceUsage,
    pub response_id: Option<String>,
}

#[derive(Default)]
struct PartialToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct StreamingTurnAssembler {
    visible_text: String,
    tool_calls: BTreeMap<u64, PartialToolCall>,
    usage: InferenceUsage,
    response_id: Option<String>,
    completed: bool,
}

impl StreamingTurnAssembler {
    /// Applies one protocol-normalized delta without exposing partial JSON to
    /// the durable transcript.
    pub fn push(&mut self, delta: InferenceDelta) {
        match delta {
            InferenceDelta::VisibleText(delta) => self.visible_text.push_str(&delta),
            InferenceDelta::ToolCall {
                slot,
                call_id,
                name_delta,
                arguments_delta,
            } => {
                let partial = self.tool_calls.entry(slot).or_default();
                if let Some(call_id) = call_id {
                    partial.call_id = call_id;
                }
                if let Some(name) = name_delta {
                    partial.name.push_str(&name);
                }
                if let Some(arguments) = arguments_delta {
                    partial.arguments.push_str(&arguments);
                }
            }
            InferenceDelta::Usage(usage) => self.usage = merge_usage(&self.usage, usage),
            InferenceDelta::ResponseId(response_id) => {
                if !response_id.trim().is_empty() {
                    self.response_id = Some(response_id);
                }
            }
            InferenceDelta::Completed => self.completed = true,
        }
    }

    /// Produces only complete canonical Tool Calls.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] for a truncated stream, incomplete Tool Call,
    /// duplicate call ID, or incomplete Tool arguments JSON.
    pub fn finish(self) -> Result<AssembledInferenceTurn, AdapterError> {
        if !self.completed {
            return Err(AdapterError::UnknownOutcome(
                "Provider stream ended before a completion event".to_owned(),
            ));
        }
        let mut calls = Vec::with_capacity(self.tool_calls.len());
        for partial in self.tool_calls.into_values() {
            if partial.call_id.trim().is_empty() || partial.name.trim().is_empty() {
                return Err(AdapterError::InvalidResponse(
                    "Provider returned an incomplete Tool Call identity".to_owned(),
                ));
            }
            let arguments = if partial.arguments.is_empty() {
                "{}".to_owned()
            } else {
                partial.arguments
            };
            serde_json::from_str::<Value>(&arguments).map_err(|error| {
                AdapterError::InvalidResponse(format!("Tool arguments are incomplete: {error}"))
            })?;
            if calls
                .iter()
                .any(|call: &CanonicalToolCall| call.call_id == partial.call_id)
            {
                return Err(AdapterError::InvalidResponse(
                    "Provider returned a duplicate Tool Call id".to_owned(),
                ));
            }
            calls.push(CanonicalToolCall {
                call_id: partial.call_id,
                name: partial.name,
                arguments_json: arguments,
            });
        }
        let visible_text = (!self.visible_text.trim().is_empty()).then_some(self.visible_text);
        if visible_text.is_none() && calls.is_empty() {
            return Err(AdapterError::InvalidResponse(
                "Provider completed without visible text or Tool Calls".to_owned(),
            ));
        }
        Ok(AssembledInferenceTurn {
            visible_text,
            tool_calls: calls,
            usage: self.usage,
            response_id: self.response_id,
        })
    }
}

fn merge_usage(current: &InferenceUsage, next: InferenceUsage) -> InferenceUsage {
    InferenceUsage {
        input_tokens: next.input_tokens.or(current.input_tokens),
        output_tokens: next.output_tokens.or(current.output_tokens),
        actual_cost_minor_units: next
            .actual_cost_minor_units
            .or(current.actual_cost_minor_units),
        currency: next.currency.or_else(|| current.currency.clone()),
    }
}

/// Maps one `OpenAI`-compatible Chat Completions SSE event.
///
/// # Errors
///
/// Returns [`AdapterError`] for malformed JSON.
pub fn openai_chat_deltas(event: &SseEvent) -> Result<Vec<InferenceDelta>, AdapterError> {
    if event.data.trim() == "[DONE]" {
        return Ok(vec![InferenceDelta::Completed]);
    }
    let value = parse_event_json(event)?;
    let mut deltas = Vec::new();
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        deltas.push(InferenceDelta::ResponseId(id.to_owned()));
    }
    if let Some(usage) = chat_usage(&value) {
        deltas.push(InferenceDelta::Usage(usage));
    }
    for choice in value
        .get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(delta) = choice.get("delta") else {
            continue;
        };
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            deltas.push(InferenceDelta::VisibleText(content.to_owned()));
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            deltas.push(InferenceDelta::ToolCall {
                slot: call.get("index").and_then(Value::as_u64).unwrap_or(0),
                call_id: call.get("id").and_then(Value::as_str).map(str::to_owned),
                name_delta: call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                arguments_delta: call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
    }
    Ok(deltas)
}

/// Maps one `OpenAI` Responses SSE event while intentionally ignoring private
/// reasoning deltas.
///
/// # Errors
///
/// Returns [`AdapterError`] for malformed JSON or an explicit failed response.
pub fn openai_responses_deltas(event: &SseEvent) -> Result<Vec<InferenceDelta>, AdapterError> {
    let value = parse_event_json(event)?;
    let kind = event
        .event
        .as_deref()
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or_default();
    let mut deltas = Vec::new();
    match kind {
        "response.created" | "response.in_progress" => {
            if let Some(id) = value.pointer("/response/id").and_then(Value::as_str) {
                deltas.push(InferenceDelta::ResponseId(id.to_owned()));
            }
        }
        "response.output_text.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                deltas.push(InferenceDelta::VisibleText(delta.to_owned()));
            }
        }
        "response.output_item.added" => {
            let item = value.get("item").unwrap_or(&Value::Null);
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                deltas.push(InferenceDelta::ToolCall {
                    slot: value
                        .get("output_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    call_id: item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    name_delta: item.get("name").and_then(Value::as_str).map(str::to_owned),
                    arguments_delta: item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .filter(|arguments| !arguments.is_empty())
                        .map(str::to_owned),
                });
            }
        }
        "response.function_call_arguments.delta" => {
            deltas.push(InferenceDelta::ToolCall {
                slot: value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                call_id: None,
                name_delta: None,
                arguments_delta: value
                    .get("delta")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
        "response.completed" => {
            if let Some(id) = value.pointer("/response/id").and_then(Value::as_str) {
                deltas.push(InferenceDelta::ResponseId(id.to_owned()));
            }
            if let Some(usage) = responses_usage(&value) {
                deltas.push(InferenceDelta::Usage(usage));
            }
            deltas.push(InferenceDelta::Completed);
        }
        "response.failed" | "error" => {
            return Err(AdapterError::InvalidResponse(
                "OpenAI Responses stream reported failure".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(deltas)
}

/// Maps one `Anthropic` Messages SSE event while intentionally ignoring thinking
/// and signature deltas.
///
/// # Errors
///
/// Returns [`AdapterError`] for malformed JSON or an explicit error event.
pub fn anthropic_deltas(event: &SseEvent) -> Result<Vec<InferenceDelta>, AdapterError> {
    let value = parse_event_json(event)?;
    let kind = event
        .event
        .as_deref()
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or_default();
    let mut deltas = Vec::new();
    match kind {
        "message_start" => {
            if let Some(id) = value.pointer("/message/id").and_then(Value::as_str) {
                deltas.push(InferenceDelta::ResponseId(id.to_owned()));
            }
            if let Some(input) = value
                .pointer("/message/usage/input_tokens")
                .and_then(Value::as_u64)
            {
                deltas.push(InferenceDelta::Usage(InferenceUsage {
                    input_tokens: Some(input),
                    ..InferenceUsage::default()
                }));
            }
        }
        "content_block_start" => {
            let block = value.get("content_block").unwrap_or(&Value::Null);
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                deltas.push(InferenceDelta::ToolCall {
                    slot: value.get("index").and_then(Value::as_u64).unwrap_or(0),
                    call_id: block.get("id").and_then(Value::as_str).map(str::to_owned),
                    name_delta: block.get("name").and_then(Value::as_str).map(str::to_owned),
                    arguments_delta: None,
                });
            }
        }
        "content_block_delta" => match value.pointer("/delta/type").and_then(Value::as_str) {
            Some("text_delta") => {
                if let Some(text) = value.pointer("/delta/text").and_then(Value::as_str) {
                    deltas.push(InferenceDelta::VisibleText(text.to_owned()));
                }
            }
            Some("input_json_delta") => deltas.push(InferenceDelta::ToolCall {
                slot: value.get("index").and_then(Value::as_u64).unwrap_or(0),
                call_id: None,
                name_delta: None,
                arguments_delta: value
                    .pointer("/delta/partial_json")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            }),
            _ => {}
        },
        "message_delta" => {
            if let Some(output) = value
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
            {
                deltas.push(InferenceDelta::Usage(InferenceUsage {
                    output_tokens: Some(output),
                    ..InferenceUsage::default()
                }));
            }
        }
        "message_stop" => deltas.push(InferenceDelta::Completed),
        "error" => {
            return Err(AdapterError::InvalidResponse(
                "Anthropic stream reported an error event".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(deltas)
}

fn parse_event_json(event: &SseEvent) -> Result<Value, AdapterError> {
    serde_json::from_str(&event.data)
        .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
}

fn chat_usage(value: &Value) -> Option<InferenceUsage> {
    let usage = value.get("usage")?;
    Some(InferenceUsage {
        input_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        output_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        actual_cost_minor_units: None,
        currency: None,
    })
}

fn responses_usage(value: &Value) -> Option<InferenceUsage> {
    let usage = value.pointer("/response/usage")?;
    Some(InferenceUsage {
        input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
        output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
        actual_cost_minor_units: None,
        currency: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        InferenceDelta, SseDecoder, SseEvent, StreamingTurnAssembler, anthropic_deltas,
        openai_chat_deltas, openai_responses_deltas,
    };

    #[test]
    fn decoder_survives_transport_splits_inside_utf8_and_multiline_data() {
        let payload = "event: note\ndata: 赛博\ndata: 音乐\n\n".as_bytes();
        let split = payload
            .windows(3)
            .position(|window| window[0] & 0b1100_0000 == 0b1100_0000)
            .expect("multibyte code point")
            + 1;
        let mut decoder = SseDecoder::default();
        assert!(
            decoder
                .push(&payload[..split])
                .expect("first chunk")
                .is_empty()
        );
        let events = decoder.push(&payload[split..]).expect("second chunk");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("note"));
        assert_eq!(events[0].data, "赛博\n音乐");
    }

    #[test]
    fn chat_deltas_assemble_parallel_complete_tool_calls() {
        let mut assembler = StreamingTurnAssembler::default();
        for event in [
            SseEvent {
                event: None,
                data: r#"{"id":"chat-1","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-a","function":{"name":"alpha","arguments":"{\"x\":"}},{"index":1,"id":"call-b","function":{"name":"beta","arguments":"{}"}}]}}]}"#.to_owned(),
            },
            SseEvent {
                event: None,
                data: r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#.to_owned(),
            },
            SseEvent {
                event: None,
                data: "[DONE]".to_owned(),
            },
        ] {
            for delta in openai_chat_deltas(&event).expect("chat delta") {
                assembler.push(delta);
            }
        }
        let turn = assembler.finish().expect("complete turn");
        assert_eq!(turn.tool_calls.len(), 2);
        assert_eq!(turn.tool_calls[0].arguments_json, r#"{"x":1}"#);
        assert_eq!(turn.usage.input_tokens, Some(10));
        assert_eq!(turn.response_id.as_deref(), Some("chat-1"));
    }

    #[test]
    fn incomplete_tool_json_never_crosses_the_assembler() {
        let mut assembler = StreamingTurnAssembler::default();
        assembler.push(InferenceDelta::ToolCall {
            slot: 0,
            call_id: Some("call-1".to_owned()),
            name_delta: Some("project.describe".to_owned()),
            arguments_delta: Some("{".to_owned()),
        });
        assembler.push(InferenceDelta::Completed);
        assert!(assembler.finish().is_err());
    }

    #[test]
    fn responses_and_anthropic_private_reasoning_deltas_are_ignored() {
        let responses = SseEvent {
            event: Some("response.reasoning_summary_text.delta".to_owned()),
            data: r#"{"type":"response.reasoning_summary_text.delta","delta":"private"}"#
                .to_owned(),
        };
        assert!(
            openai_responses_deltas(&responses)
                .expect("unknown reasoning event")
                .is_empty()
        );
        let anthropic = SseEvent {
            event: Some("content_block_delta".to_owned()),
            data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private"}}"#.to_owned(),
        };
        assert!(
            anthropic_deltas(&anthropic)
                .expect("thinking event")
                .is_empty()
        );
    }
}
