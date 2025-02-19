use std::{collections::HashMap, future::Future};

use anyhow::{anyhow, Result};
use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use futures::future::BoxFuture;
use itertools::Itertools;
use schemars::{
    gen::SchemaSettings,
    schema::{RootSchema, Schema, SchemaObject},
    visit::Visitor,
    JsonSchema,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

type BoxAsyncCallback = Box<dyn Fn(Value) -> BoxFuture<'static, Result<Value>>>;

pub trait Callable {
    type Input: JsonSchema + DeserializeOwned;
    type Output: JsonSchema + Serialize;
    fn description(&self) -> String;
    fn name(&self) -> String;
    fn call(self, inp: Self::Input) -> impl Future<Output = Result<Self::Output>> + Send;
}

async fn call(callable: impl Callable, input: Value) -> Result<Value> {
    let input = serde_json::from_value(input)?;
    let output = callable.call(input).await?;
    let output = serde_json::to_value(output)?;
    Ok(output)
}

fn monomorphic_callable(callable: impl Callable + Clone + Send + 'static) -> BoxAsyncCallback {
    Box::new(move |input| {
        let callable = callable.clone();
        Box::pin(call(callable, input))
    })
}

struct WrappedCallable {
    call: BoxAsyncCallback,
    input_schema: RootSchema,
    description: String,
}

#[derive(Default)]
pub struct Callables {
    callables: HashMap<String, WrappedCallable>,
}

impl Callables {
    pub fn add<C: Callable + Send + Clone + 'static>(&mut self, callable: C) {
        let name = callable.name();
        let input_schema = schema_for::<C::Input>();
        let output_schema = schema_for::<Result<C::Output, String>>();
        let description = format!(
            "{}\nreturns:\n{}",
            callable.description(),
            serde_json::to_string(&output_schema).unwrap()
        );
        let call = monomorphic_callable(callable);
        self.callables.insert(
            name,
            WrappedCallable {
                call,
                input_schema,
                description,
            },
        );
    }

    pub async fn call_inner(&self, name: &str, input: Value) -> Result<Value> {
        let callable = self
            .callables
            .get(name)
            .ok_or_else(|| anyhow!("Callable not found"))?;
        (callable.call)(input).await
    }

    pub async fn call(&self, name: &str, input: Value) -> Value {
        let ret = self
            .call_inner(name, input.clone())
            .await
            .map_err(|e| format!("{}", e));
        let ret = serde_json::to_value(&ret).unwrap();
        tracing::info!(
            "{} {} -> {}",
            name,
            serde_json::to_string(&input).unwrap(),
            serde_json::to_string(&ret).unwrap()
        );
        ret
    }

    pub fn tools(&self) -> Vec<ChatCompletionTool> {
        self.callables
            .iter()
            .map(|(name, callable)| ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: name.clone(),
                    description: Some(callable.description.clone()),
                    parameters: Some(serde_json::to_value(&callable.input_schema).unwrap()),
                    strict: Some(true),
                },
            })
            .collect_vec()
    }
}

fn schema_for<T>() -> RootSchema
where
    T: JsonSchema,
{
    // openai doesn't schemas with "format"
    #[derive(Clone, Debug)]
    struct RemoveFormat;

    impl Visitor for RemoveFormat {
        fn visit_root_schema(&mut self, root: &mut RootSchema) {
            root.schema.format = None;
            schemars::visit::visit_root_schema(self, root)
        }

        fn visit_schema(&mut self, schema: &mut Schema) {
            match schema {
                Schema::Object(obj) => {
                    obj.format = None;
                }
                Schema::Bool(_) => {}
            }
            schemars::visit::visit_schema(self, schema)
        }

        fn visit_schema_object(&mut self, schema: &mut SchemaObject) {
            schema.format = None;
            schemars::visit::visit_schema_object(self, schema)
        }
    }

    // openai insists attitionalProperties is false
    #[derive(Clone, Debug)]
    struct AdditionalPropertiesFalse;

    impl Visitor for AdditionalPropertiesFalse {
        fn visit_root_schema(&mut self, root: &mut RootSchema) {
            if let Some(ref mut obj) = root.schema.object {
                obj.additional_properties = Some(Box::new(Schema::Bool(false)));
            }
            schemars::visit::visit_root_schema(self, root)
        }

        fn visit_schema(&mut self, schema: &mut Schema) {
            if let Schema::Object(obj) = schema {
                if let Some(ref mut obj) = obj.object {
                    obj.additional_properties = Some(Box::new(Schema::Bool(false)));
                }
            }
            schemars::visit::visit_schema(self, schema)
        }

        fn visit_schema_object(&mut self, schema: &mut SchemaObject) {
            if let Some(ref mut obj) = schema.object {
                obj.additional_properties = Some(Box::new(Schema::Bool(false)));
            }
            schemars::visit::visit_schema_object(self, schema)
        }
    }

    SchemaSettings::default()
        .with_visitor(RemoveFormat)
        .with_visitor(AdditionalPropertiesFalse)
        .into_generator()
        .into_root_schema_for::<T>()
}
