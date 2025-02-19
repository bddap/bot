use async_openai::types::ChatCompletionRequestMessage;

#[derive(Default, Debug)]
pub struct WorkingMemory {
    fresh: Vec<ChatCompletionRequestMessage>,
    // job_stack: Vec<String>,
}

impl WorkingMemory {
    pub fn messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.fresh.clone()
    }

    pub fn add_messages(&mut self, new_history: Vec<ChatCompletionRequestMessage>) {
        tracing::trace!("{}", serde_json::to_string(&new_history).unwrap());
        self.fresh.extend(new_history);
    }
}
