use anyhow::Context;
use rig::prelude::*;
use rig::{completion::Prompt, providers::gemini};
use rig::telemetry::SpanCombinator;
use serde_json::json;

mod otel;

#[tracing::instrument(name = "rig_gemini_basic_prompt")]
async fn run_prompt() -> anyhow::Result<String> {
    let client = gemini::Client::from_env();

    let agent = client
        .agent("gemini-2.5-flash")
        .preamble("You are a concise technical assistant. Answer clearly and with short bullets.")
        .temperature(0.2)
        .build();

    let prompt_text =
        "Explain OpenTelemetry in exactly 3 bullets for a Rust backend engineer.";
    let prompt_span = tracing::info_span!(
        "agent.prompt",
        model = "gemini-2.5-flash",
        stage = "planner"
    );
    let _prompt_guard = prompt_span.enter();

    prompt_span.record_model_input(&json!({
        "prompt": prompt_text,
    }));
    tracing::info!(model = "gemini-2.5-flash", "Sending prompt to Gemini");

    let answer = agent
        .prompt(prompt_text)
        .await
        .context("Gemini prompt failed")?;

    prompt_span.record_model_output(&json!({
        "response_len": answer.len(),
        "response_preview": answer.chars().take(120).collect::<String>(),
    }));
    tracing::info!(response_len = answer.len(), "Received response");

    Ok(answer)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _provider = otel::init_telemetry("rig-gemini-basic-example").context("Failed to initialize telemetry")?;

    if !otel::has_gemini_api_key() {
        println!("Set GEMINI_API_KEY to run this example against the live Gemini API.");
        println!("Traces are still initialized with local fallback defaults.");
        return Ok(());
    }

    let answer = run_prompt().await?;
    println!("=== Gemini response ===\n{answer}");

    Ok(())
}
