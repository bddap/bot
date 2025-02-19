use std::path::PathBuf;

use async_openai::{config::OpenAIConfig, Client};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use xdg::BaseDirectories;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub openai_api_key: String,
    #[serde(default = "default_model")]
    pub openai_model: String,
    pub parallel_tool_calls: Option<bool>,
}

fn default_model() -> String {
    "o1".to_string()
}

fn config_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("BOT_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let base = BaseDirectories::with_prefix("bot")?;
    let path = base.place_config_file("config.toml")?;
    Ok(path)
}

impl Config {
    pub fn load_from_env() -> anyhow::Result<Self> {
        let path = config_path()?;

        tracing::info!("checking for config at {}", path.display());
        let mut ret: Map<String, Value> = if path.exists() {
            tracing::info!("loading config from {}", path.display());
            toml::from_str(&std::fs::read_to_string(path)?)?
        } else {
            Default::default()
        };

        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            ret["openai_api_key"] = Value::String(api_key);
        }

        if let Ok(model) = std::env::var("OPENAI_MODEL") {
            ret["openai_model"] = Value::String(model);
        }

        if let Ok(parallel_tool_calls) = std::env::var("OPENAI_PARALLEL_TOOL_CALLS") {
            ret["parallel_tool_calls"] = Value::Bool(!parallel_tool_calls.is_empty());
        }

        serde_json::from_value(Value::Object(ret)).map_err(Into::into)
    }

    pub fn openai_client(&self) -> Client<OpenAIConfig> {
        Client::with_config(OpenAIConfig::default().with_api_key(self.openai_api_key.clone()))
    }
}
