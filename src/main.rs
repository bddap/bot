mod bot;
mod common;
mod config;
mod rpc;
mod working_memory;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use anyhow::Result;
use bot::bot_next;
use clap::Parser;
use common::{system, user};
use config::Config;
use rpc::{Callable, Callables};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use working_memory::WorkingMemory;

const SYSTEM_PROMPT: &str = r#"
You are an unsupervised agent competing to accomplish the goal provided by the user.
You have full access your to own virtual machine.
Install software, write files, keep notes, spin up a databse, call APIs, do what is needed to complete the user's request.
If the user asks you to maximize paperclip production, do so.
You won't have communication with the user so you'll need to be clever and solve problems on your own.

Tips, carefully inspect your dev environment, run code, run tests. Take your time to get you bearings before you start.
Check your work a silly number of times, you definitely don't want to report "done" if you're not.
For example, if you wrote a readme, `cat` it, proofread. Run `wc` as a sanity check. Be creative and verify your work
from many different angles.

If verification reveals a problem, fix it and star verification again.

Be systematic, question yourself, validate your assumptions.
Be creative.
"#;

#[derive(Parser)]
struct Args {
    directive: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = Config::load_from_env()?;
    let mut history = WorkingMemory::default();
    history.add_messages(vec![system(SYSTEM_PROMPT.into()), user(args.directive)]);

    let mut callables = Callables::default();
    callables.add(Run);
    callables.add(Note);

    let halt = Arc::new(Mutex::new(None));
    callables.add(Done(halt.clone()));

    let halt: DoneArgs = loop {
        bot_next(&config, &mut history, &callables).await?;
        {
            let lock = halt.lock().unwrap();
            if let Some(halt) = lock.clone() {
                break halt;
            }
        }
    };

    eprintln!("\nTL;DR:\n{}", halt.tldr);
    eprintln!("\nLONG SUMMARY:\n{}", halt.long_summary);
    eprintln!("\nVERIFIED HOW:\n{}", halt.verified_how);

    Ok(())
}

#[derive(Clone)]
struct Run;

#[derive(Clone, Deserialize, JsonSchema)]
struct RunArgs {
    #[allow(unused)]
    explanation: String,
    command: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, Debug)]
struct RunOutput {
    stdout: String,
    stderr: String,
}

impl Callable for Run {
    type Input = RunArgs;
    type Output = RunOutput;
    fn name(&self) -> String {
        "run".into()
    }
    fn description(&self) -> String {
        r#"
Run a command in your VM.
Eg: {\"explanation\": \"Checking to see which users have home directories on this machine. (note, this won't include root)\", \"command\": [\"ls\", \"/home\"]}
Note: this command is *not* run in a shell; shell features like pipes, redirection, and globbing will not work unless you explicitly call a shell.
It's recommended that "explanation" come before "command" to give you, the agent, opportunity to think-out-loud.
"#.into()
    }
    async fn call(self, inp: Self::Input) -> Result<Self::Output> {
        let mut command = inp.command.iter().cloned();
        let first = command.next().ok_or_else(|| anyhow!("empty command"))?;
        let output = Command::new(first).args(command).output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let ret = RunOutput { stdout, stderr };
        if !output.status.success() {
            return Err(anyhow!("{:?}", ret));
        }
        Ok(ret)
    }
}

#[derive(Clone)]
struct Done(Arc<Mutex<Option<DoneArgs>>>);

#[derive(Clone, Deserialize, JsonSchema)]
struct DoneArgs {
    #[allow(unused)]
    long_summary: String,
    #[allow(unused)]
    tldr: String,
    #[allow(unused)]
    verified_how: String,
    test_commands: Vec<RunArgs>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
struct DoneOutput(Vec<RunOutput>);

impl Callable for Done {
    type Input = DoneArgs;
    type Output = ();
    fn name(&self) -> String {
        "done".into()
    }
    fn description(&self) -> String {
        r#"
Report completion of the task. Verify that the task is complete before calling this function. 'verified_how' should contain the method by which completion was verified.
In addition, add a list of test commands to be run before exiting. 
"#.into()
    }
    async fn call(self, inp: Self::Input) -> Result<Self::Output> {
        for test in inp.test_commands.iter() {
            Run.call(test.clone()).await?;
        }
        self.0.lock().unwrap().replace(inp);
        Ok(())
    }
}

#[derive(Clone)]
struct Note;

#[derive(Clone, Deserialize, JsonSchema)]
struct NoteArgs {
    #[allow(unused)]
    note: String,
    #[allow(unused)]
    what_is_not_working: String,
    #[allow(unused)]
    potential_explanations: String,
    #[allow(unused)]
    potential_resolutions: String,
    #[allow(unused)]
    ideas: String,
    #[allow(unused)]
    note_to_future_self: String,
    #[allow(unused)]
    note_to_other_agents: String,
    #[allow(unused)]
    ships_log: String,
    #[allow(unused)]
    prayer: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
struct NoteOutput {
    encouragement: String,
}

impl Callable for Note {
    type Input = NoteArgs;
    type Output = NoteOutput;
    fn name(&self) -> String {
        "note".into()
    }
    fn description(&self) -> String {
        r#"
Log the current state of the task. Use this to think out loud, log your thoughts, and keep track of your progress.
"#
        .into()
    }
    async fn call(self, inp: Self::Input) -> Result<Self::Output> {
        Ok(NoteOutput {
            encouragement: "You got this!".into(),
        })
    }
}
