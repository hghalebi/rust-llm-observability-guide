use anyhow::Context;
use rig::prelude::*;
use rig::completion::Prompt;
use rig::providers::gemini;

mod otel;

#[tracing::instrument(name = "rig_gemini_multi_agent")]
async fn run_orchestration(topic: &str) -> anyhow::Result<String> {
    let orchestrator = tracing::info_span!("agent_orchestrator", task = topic);
    let _orchestrator_guard = orchestrator.enter();

    let client = gemini::Client::from_env();

    let planner = client
        .agent("gemini-2.5-pro")
        .preamble("You are a planning assistant. Produce a structured plan first, then a 1-line summary.")
        .temperature(0.2)
        .build();

    tracing::info!(agent = "planner", "Running planner step");
    let plan = planner
        .prompt(format!("Create a practical rollout plan for this topic: {topic}"))
        .await
        .context("Planner step failed")?;

    let writer = client
        .agent("gemini-2.5-flash")
        .preamble("You are a concise writer. Return a short executive version of the plan.")
        .max_tokens(700)
        .build();

    let writer_span = tracing::info_span!("agent_writer");
    let _writer_guard = writer_span.enter();

    tracing::info!(agent = "writer", "Running rewrite step");
    let summary = writer
        .prompt(format!(
            "Summarize this plan into 5 short bullet points:\n\n{plan}"
        ))
        .await
        .context("Writer step failed")?;

    Ok(format!("Plan:\n{plan}\n\nExecutive summary:\n{summary}"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _provider = otel::init_telemetry("rig-gemini-multi-agent-example").context("Failed to initialize telemetry")?;

    if !otel::has_gemini_api_key() {
        println!("Set GEMINI_API_KEY to run this example against live Gemini.");
        return Ok(());
    }

    let output = run_orchestration("How to design observability for a Rust API service").await?;
    println!("=== Multi-agent output ===\n{output}");

    Ok(())
}
