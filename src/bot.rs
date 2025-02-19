use anyhow::{anyhow, Result};
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestAssistantMessageAudio, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionResponseMessage,
    ChatCompletionToolChoiceOption, CompletionUsage, CreateChatCompletionRequest,
    CreateChatCompletionResponse,
};
use serde_json::Value;

use crate::{config::Config, rpc::Callables, working_memory::WorkingMemory};

pub async fn bot_next(
    config: &Config,
    history: &mut WorkingMemory,
    callables: &Callables,
) -> Result<()> {
    let request = CreateChatCompletionRequest {
        model: config.openai_model.clone(),
        messages: history.messages(),
        tools: Some(callables.tools()),
        tool_choice: Some(ChatCompletionToolChoiceOption::Required),
        parallel_tool_calls: config.parallel_tool_calls,
        ..Default::default()
    };

    let response: CreateChatCompletionResponse =
        config.openai_client().chat().create(request).await?;

    if let Some(usage) = response.usage.as_ref() {
        tracing::info!("{}", display_usage(usage));
    }

    if response.choices.len() != 1 {
        tracing::warn!("Expected 1 choice, got {}", response.choices.len());
    }

    let response = response
        .choices
        .first()
        .ok_or_else(|| anyhow!("No choices in response"))?;

    let tool_calls: Vec<ChatCompletionMessageToolCall> = response
        .clone()
        .message
        .tool_calls
        .ok_or_else(|| anyhow!("No tool calls in response"))?;

    if tool_calls.is_empty() {
        tracing::warn!("No tool calls in response");
    }

    let mut new_history: Vec<ChatCompletionRequestMessage> =
        vec![ChatCompletionRequestMessage::Assistant(to_request_message(
            response.clone().message,
        ))];

    for tool_call in tool_calls {
        let input = serde_json::from_str(&tool_call.function.arguments)?;
        let output = callables.call(&tool_call.function.name, input).await;
        let output = serde_json::to_string(&output)?;
        new_history.push(ChatCompletionRequestMessage::Tool(
            ChatCompletionRequestToolMessage {
                content: ChatCompletionRequestToolMessageContent::Text(output),
                tool_call_id: tool_call.id,
            },
        ));
    }

    history.add_messages(new_history);

    Ok(())
}

fn to_request_message(
    response_message: ChatCompletionResponseMessage,
) -> ChatCompletionRequestAssistantMessage {
    #[allow(deprecated)]
    let ChatCompletionResponseMessage {
        content,
        refusal,
        tool_calls,
        role: _,
        function_call,
        audio,
    } = response_message;
    #[allow(deprecated)]
    ChatCompletionRequestAssistantMessage {
        content: content.map(ChatCompletionRequestAssistantMessageContent::Text),
        name: None,
        audio: audio.map(|audio| ChatCompletionRequestAssistantMessageAudio { id: audio.id }),
        tool_calls,
        function_call,
        refusal,
    }
}

fn display_usage(usage: &CompletionUsage) -> String {
    let mut value = serde_json::to_value(usage).unwrap();
    remove_zeros(&mut value);
    serde_json::to_string(&value).unwrap()
}

fn remove_zeros(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            for (_k, v) in obj.iter_mut() {
                remove_zeros(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                remove_zeros(v);
            }
        }
        _ => {}
    }
    match value {
        Value::Object(obj) => {
            obj.retain(|_k, v| {
                remove_zeros(v);
                if let Some(v) = v.as_i64() {
                    if v == 0 {
                        return false;
                    }
                }
                true
            });
        }
        _ => {}
    };
    match value {
        Value::Object(obj) => {
            obj.retain(|_k, v| {
                remove_zeros(v);
                if let Value::Object(obj) = v {
                    if obj.is_empty() {
                        return false;
                    }
                }
                true
            });
        }
        _ => {}
    };
}
