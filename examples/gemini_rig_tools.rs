use anyhow::Context;
use rig::prelude::*;
use rig::completion::Prompt;
use rig::{completion::ToolDefinition, providers::gemini, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;

mod otel;

#[derive(Debug)]
struct ToolError;

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "math tool execution failed")
    }
}

impl std::error::Error for ToolError {}

#[derive(Debug, Serialize, Deserialize)]
struct AddArgs {
    x: i32,
    y: i32,
}

#[derive(Clone, Default)]
struct AddTool;

impl Tool for AddTool {
    const NAME: &'static str = "add_numbers";
    type Error = ToolError;
    type Args = AddArgs;
    type Output = i32;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::from_value(json!({
            "name": "add_numbers",
            "description": "Add two numbers together",
            "parameters": {
                "type": "object",
                "properties": {
                    "x": {"type": "number", "description": "first operand"},
                    "y": {"type": "number", "description": "second operand"}
                },
                "required": ["x", "y"]
            }
        }))
        .expect("tool definition json")
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let span = tracing::info_span!("tool.add_numbers", x = args.x, y = args.y);
        let _guard = span.enter();

        tracing::info!("Executing math tool");
        Ok(args.x + args.y)
    }
}

#[tracing::instrument(name = "rig_gemini_with_tool")]
async fn run_tool_agent() -> anyhow::Result<String> {
    let client = gemini::Client::from_env();

    let agent = client
        .agent("gemini-2.5-flash")
        .preamble(
            "You are a calculator assistant. Use the `add_numbers` tool whenever the user asks for arithmetic.",
        )
        .tool(AddTool)
        .build();

    let answer = agent
        .prompt("Use the add_numbers tool to compute 42 + 58")
        .await
        .context("Gemini tool-enabled prompt failed")?;

    Ok(answer)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _provider = otel::init_telemetry("rig-gemini-tools-example").context("Failed to initialize telemetry")?;

    if !otel::has_gemini_api_key() {
        println!("Set GEMINI_API_KEY to run this example against live Gemini.");
        return Ok(());
    }

    let answer = run_tool_agent().await?;
    println!("=== Gemini tool trace result ===\n{answer}");

    Ok(())
}
