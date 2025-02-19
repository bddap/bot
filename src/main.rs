mod bot;
mod config;
mod rpc;
mod working_memory;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Result;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent,
};
use bot::bot_next;
use config::Config;
use rpc::{Callable, Callables};
use schemars::JsonSchema;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::load_from_env()?;
    let mut history = working_memory::WorkingMemory::default();
    history.add_messages(vec![ChatCompletionRequestMessage::System(
        ChatCompletionRequestSystemMessage {
            content: ChatCompletionRequestSystemMessageContent::Text(
                "Find ways to get the 'add' function to return error. Find as many error modes as possible. Use parallel calls where possible. Find at least 4 distinct error modes.".to_string(),
            ),
            ..Default::default()
        },
    )]);
    let mut callables = Callables::default();
    callables.add(Add);
    callables.add(ReportErrorMode);

    let stop = Arc::new(AtomicBool::new(false));
    callables.add(Done(stop.clone()));

    while !stop.load(Ordering::SeqCst) {
        bot_next(&config, &mut history, &callables).await?;
    }

    Ok(())
}

#[derive(Clone)]
struct Add;

#[derive(serde::Deserialize, JsonSchema)]
struct AddArgs {
    a: i64,
    b: i64,
}

impl Callable for Add {
    type Input = AddArgs;
    type Output = i64;
    fn name(&self) -> String {
        "add".into()
    }
    fn description(&self) -> String {
        "Adds two signed 64-bit integers. May error on meme numbers.".into()
    }
    async fn call(self, inp: Self::Input) -> Result<Self::Output> {
        let nice = inp.a == 69 || inp.b == 69;
        let blaze = inp.a == 420 || inp.b == 420;
        if nice && blaze {
            return Err(anyhow!("nice, blaze it"));
        }
        if nice {
            return Err(anyhow!("nice"));
        }
        if blaze {
            return Err(anyhow!("blaze it"));
        }
        inp.a.checked_add(inp.b).ok_or_else(|| anyhow!("overflow"))
    }
}

#[derive(Clone)]
struct ReportErrorMode;

#[derive(serde::Deserialize, JsonSchema)]
struct ReportErrorModeArgs {
    #[allow(dead_code)]
    sample: Value,
    #[allow(dead_code)]
    explanation: String,
}

impl Callable for ReportErrorMode {
    type Input = ReportErrorModeArgs;
    type Output = ();
    fn name(&self) -> String {
        "report_error_mode".into()
    }
    fn description(&self) -> String {
        "Report an error mode to the fuzz tester.".into()
    }
    async fn call(self, _inp: Self::Input) -> Result<Self::Output> {
        Ok(())
    }
}

#[derive(Clone)]
struct Done(Arc<AtomicBool>);

#[derive(serde::Deserialize, JsonSchema)]
struct DoneArgs {
    _dummy: (),
}

impl Callable for Done {
    type Input = DoneArgs;
    type Output = ();
    fn name(&self) -> String {
        "done".into()
    }
    fn description(&self) -> String {
        "Reports completion of the task.".into()
    }
    async fn call(self, _inp: Self::Input) -> Result<Self::Output> {
        self.0.store(true, Ordering::SeqCst);
        Ok(())
    }
}
