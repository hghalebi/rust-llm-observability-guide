# OpenTelemetry + SigNoz for Rig Agents (Gemini): Practical tutorial from first run to production rigor

This guide is written for teams starting with observability in LLM systems.  
We move from ideas to implementation at an accessible pace, then layer in the operational rigor needed for production.

It is based on the working examples in this repo:

- `examples/otel.rs`
- `examples/gemini_rig_basic.rs`
- `examples/gemini_rig_tools.rs`
- `examples/gemini_multi_agent.rs`

You will learn how to:

1. Think clearly about what to measure in LLM workflows.
2. Design spans that answer real engineering questions.
3. Run examples and verify traces in SigNoz.
4. Iterate when observability is incomplete.

---

## 0) The mindset shift: logs are not enough

In LLM systems, logs tell you **what was written**; tracing tells you **why it happened in order**.

Example:

- You see a log saying "tool returned: 100".
- Without a trace, you do not know:
  - Did model planning time 5x normal?
  - Was the tool invoked from planner or from writer?
  - Which user request this belongs to under load?

Observability starts with a **trace tree**.

---

## 1) What problem this solves (in plain language)

For each user request, you want to know:

- Which steps were taken.
- Which step took the longest.
- Which model and tool was used where.
- Where errors entered the chain.
- Which request this run belongs to.

If your output is flat (all spans at one level), you are blind to the real control flow.

## 1.1) Fast start for this tutorial

### What you need before you begin

- Rust and Cargo (`rustup`, `cargo`; Rust 1.70+ recommended).
- Docker installed and running (`docker` command available).
- `nc` (netcat) available.
- A valid `GEMINI_API_KEY` for Gemini examples.
- OTLP destination (SigNoz Cloud endpoint + ingestion key, or a local collector on OTLP ports).
- Network access for provider calls.

### Where to look first

From the `rust-llm-observability-guide` folder:

- Core guide: `README.md`
- Telemetry setup: `examples/otel.rs`
- Smoke example: `examples/otel_smoke.rs`
- Gemini examples:  
  `examples/gemini_rig_basic.rs`, `examples/gemini_rig_tools.rs`, `examples/gemini_multi_agent.rs`
- Automation scripts:  
  `scripts/run-otel-smoke-check.sh`, `scripts/run-otel-rig-examples.sh`

### Recommended first run sequence

1. Read setup and sample shape examples in this order:
   - `examples/otel.rs`
   - `examples/otel_smoke.rs`
   - `examples/gemini_rig_basic.rs`
2. Run the smoke check:

```bash
cd rust-llm-observability-guide
./scripts/run-otel-smoke-check.sh
```

3. Run all runnable examples:

```bash
cd rust-llm-observability-guide
./scripts/run-otel-rig-examples.sh
```

If an example is skipped, the script prints a clear skip reason and continues.

### Practical checks to expect

- `otel_smoke_probe: found in collector output`
- `collector_receives: true`
- `marker_match: true`

### License and support

This tutorial is under **MIT License** and is fully open source.  
If something is unclear or not working, you can ask and I will try to clarify. Community help is welcome.

---

## 2) OpenTelemetry vocabulary for practical start-up (important)

### 2.1 Trace
A **Trace** is one complete journey (for one request).  
Think of it as one “story” from start to finish.

### 2.2 Span
A **Span** is one chapter in the story.  
Each chapter has:

- name
- start/end time
- parent/child relationship
- status (success/error)

### 2.3 Context
Each span has IDs:

- `trace_id`: story identifier
- `span_id`: chapter identifier
- `parent_id`: who started this chapter

### 2.4 Resource
A **Resource** describes the application that emits telemetry (for example, service name).

### 2.5 Attributes and events

- **Attributes** = searchable metadata (low-cardinality, stable values).
- **Events** = timeline notes and explanatory lines inside a span.

## 3) Trace language map for this project

We will reuse these concrete names:

- Root request: `rig_gemini_basic_prompt` / `rig_gemini_with_tool` / `rig_gemini_multi_agent`
- Tool child span: `tool.add_numbers`
- Planner/writer phases: `agent_orchestrator`, `agent_writer`

When you read a trace, the question is:  
**“Do these names map to my intended flow diagram?”**

## 3.1 Vendor choice note: this stack is a pattern, not a lock-in

This tutorial uses Gemini and SigNoz as examples, but the architecture is provider/backend agnostic.

- You can swap `rig::providers::gemini` for another Rig provider without redesigning your tracing.
- You can swap model IDs (`gemini-2.5-flash`, `gemini-2.5-pro`) for your chosen provider.
- You can keep the same span strategy and OTLP pipeline while changing the backend.
- You can point `OTEL_EXPORTER_OTLP_ENDPOINT` to any OpenTelemetry-compatible collector.

Practical alternatives:

- LLM providers: OpenAI, Anthropic, Azure OpenAI, or any provider with Rig integration.
- OTel backends: SigNoz, Jaeger, Tempo, Zipkin, New Relic, Datadog (via OpenTelemetry endpoints).

Only two parts usually change during migration:

1. provider/model configuration at agent construction
2. OTLP environment variables

Everything else—trace structure, span hierarchy, and debugging workflow—remains the same.

---

## 4) Before coding: make a prediction (this is the most useful first-step habit)

Before running anything, write down expected spans.

### Example: `gemini_multi_agent` expected shape

```text
rig_gemini_multi_agent (request)
└─ agent_orchestrator
   ├─ planner.prompt  (conceptual planning step)
   └─ agent_writer
      └─ writer.prompt (rewrite step)
```

If your observed trace does not match this shape, do not optimize latency yet.
Fix instrumentation first.

---

## 5) Dependencies

Use the same versions used by the working examples:

```toml
[dependencies]
anyhow = "1"
opentelemetry = { version = "0.30.0", features = ["trace"] }
opentelemetry_sdk = { version = "0.30.0", features = ["trace", "rt-tokio"] }
opentelemetry-otlp = { version = "0.30.0", features = ["grpc-tonic", "trace", "tls-roots"] }
rig = { package = "rig-core", version = "0.31.0" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-opentelemetry = "0.31.0"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
tonic = { version = "0.12" }
```

---

## 6) Initialize telemetry once (copy this pattern first)

`examples/otel.rs` centralizes everything:

```rust
use anyhow::Context;
use opentelemetry::global;
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry::trace::TracerProvider as TracerProviderTrait;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry(service_name: &str) -> anyhow::Result<SdkTracerProvider> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .context("Failed to create OTLP span exporter")?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name(service_name.to_owned())
                .with_attribute(KeyValue::new("telemetry.sdk.language", "rust"))
                .build(),
        )
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    let tracer = TracerProviderTrait::tracer(&tracer_provider, "rig-gemini-tracer");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer().with_target(false))
        .with(otel_layer)
        .init();

    Ok(tracer_provider)
}

pub fn has_gemini_api_key() -> bool {
    std::env::var("GEMINI_API_KEY").is_ok()
}
```

### Why this design is reliable for first-pass implementations

1. One setup function means no confusion.
2. Same environment variable flow for local and cloud setups.
3. Global provider means all examples share one model.

> In production, this saves hours of “why one span appears and another is missing”.

Call this in each `main` before work begins.

---

## 7) Example A: one request, one model (`gemini_rig_basic.rs`)

```rust
#[tracing::instrument(name = "rig_gemini_basic_prompt")]
async fn run_prompt() -> anyhow::Result<String> {
    let client = rig::providers::gemini::Client::from_env();
    let agent = client
        .agent("gemini-2.5-flash")
        .preamble("You are a concise technical assistant. Answer clearly and with short bullets.")
        .temperature(0.2)
        .build();

    tracing::info!(model = "gemini-2.5-flash", "Sending prompt to Gemini");

    let answer = agent
        .prompt("Explain OpenTelemetry in exactly 3 bullets for a Rust backend engineer.")
        .await
        .context("Gemini prompt failed")?;

    tracing::info!(response_len = answer.len(), "Received response");
    Ok(answer)
}
```

### What to observe

- Root span = request
- Event logs = "sending" and "received"
- Good first pattern to confirm your pipeline works

---

## 8) Example B: add a tool call (`gemini_rig_tools.rs`)

```rust
#[tracing::instrument(name = "rig_gemini_with_tool")]
async fn run_tool_agent() -> anyhow::Result<String> {
    let client = rig::providers::gemini::Client::from_env();

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
```

Tool call span inside `AddTool::call`:

```rust
let span = tracing::info_span!("tool.add_numbers", x = args.x, y = args.y);
```

### Why this matters in the first instrumentation pass

Without this explicit child span, tool work disappears behind a model span and you cannot answer:
- Did tool execution or model thinking dominate latency?
- Was tool error correctly linked to the original request?

---

## 9) Example C: two-stage orchestration (`gemini_multi_agent.rs`)

```rust
#[tracing::instrument(name = "rig_gemini_multi_agent")]
async fn run_orchestration(topic: &str) -> anyhow::Result<String> {
    let orchestrator = tracing::info_span!("agent_orchestrator", task = topic);
    let _orchestrator_guard = orchestrator.enter();

    let client = rig::providers::gemini::Client::from_env();

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
        .prompt(format!("Summarize this plan into 5 short bullet points:\n\n{plan}"))
        .await
        .context("Writer step failed")?;

    Ok(format!("Plan:\n{plan}\n\nExecutive summary:\n{summary}"))
}
```

### Early-stage learning checkpoint

This example proves that one user request can have two model contexts and still stay in one trace tree.

---

## 10) Runbook (copy and run)

### Configure environment

#### Cloud (direct SigNoz)

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="https://<your-region>.ingest.signoz.cloud:443"
export OTEL_EXPORTER_OTLP_HEADERS='signoz-ingestion-key='$SIGNOZ_INGESTION_KEY
export OTEL_EXPORTER_OTLP_COMPRESSION=gzip
export GEMINI_API_KEY="your_gemini_key"
```

#### Local collector

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
unset OTEL_EXPORTER_OTLP_HEADERS
export GEMINI_API_KEY="your_gemini_key"
```

### Execute examples

```bash
cargo run --example gemini_rig_basic
cargo run --example gemini_rig_tools
cargo run --example gemini_multi_agent
```

If `GEMINI_API_KEY` is missing, examples exit with clear guidance without making network calls.

---

## 11) Read the trace like a first-pass review

For each run:

1. Open the trace for the run.
2. Confirm exactly one root span.
3. Expand the tree:
   - Are planner/writer/tool nodes where expected?
4. Compare durations:
   - Which node is the slowest?
5. Check status:
   - Any non-OK error status? Expand details.

If any step fails this check, do not tune sampling yet. Fix instrumentation gaps first.

## 11.1 Automated collector smoke check

Use this script for a repeatable collector verification:

```bash
cd rust-llm-observability-guide
./scripts/run-otel-smoke-check.sh
```

The script:

- starts a temporary OpenTelemetry Collector on safe local ports,
- emits a tiny known telemetry span from the Rust path,
- forces shutdown so span batches flush,
- validates that collector output contains `otel_smoke_probe` for that run,
- prints collector output, the example output, and a clear pass/fail summary.
- helps learners inspect exactly what reached the collector and confirm span shape.

Useful overrides:

- `OTEL_SMOKE_MARKER=my-ci-run ./scripts/run-otel-smoke-check.sh`
- `OTEL_SMOKE_GRPC_PORT=14417 ./scripts/run-otel-smoke-check.sh`
- `OTEL_SMOKE_SERVICE=rig-smoke ./scripts/run-otel-smoke-check.sh`

What you should see when successful:

- `collector_receives: true`
- `otel_smoke_probe: found in collector output`
- a `ResourceSpans` block in collector logs containing `otel_smoke_probe`
- an attribute line where `marker` equals your `OTEL_SMOKE_MARKER`

## 11.2 Run all runnable examples (scripted)

Run the tutorial examples from one command:

```bash
cd rust-llm-observability-guide
./scripts/run-otel-rig-examples.sh
```

Script behavior:

- always runs `otel_smoke` so telemetry collection is exercised end-to-end,
- runs each Gemini example and prints compact run output,
- skips Gemini examples automatically if `GEMINI_API_KEY` is missing (with a clear reason),
- prints a final `Summary` line with `PASS`, `FAIL`, and `SKIP` counts.

Use it after the smoke test script to quickly confirm both code paths:

- telemetry path (`otel_smoke`)  
- agent behavior (`gemini_rig_basic`, `gemini_rig_tools`, `gemini_multi_agent`)

## 12) Common first-pass mistakes and corrections

### Mistake 1: Flat traces only
**Cause**: no span hierarchy around phase changes.  
**Fix**: place `#[tracing::instrument]` and explicit child spans around planner/tool paths.

### Mistake 2: Full prompts as span names
**Cause**: every request creates a unique span name.  
**Fix**: keep names stable and use events for narrative.

### Mistake 3: Recreating telemetry providers per request
**Cause**: inconsistent context, missing parent links.  
**Fix**: initialize once at startup.

### Mistake 4: Missing `service.name`
**Cause**: dashboards cannot group by binary/service correctly.  
**Fix**: set resource consistently in `init_telemetry`.

### Mistake 5: Instrumenting too much without purpose
**Cause**: noisy signals, expensive storage.  
**Fix**: only span what helps debugging and latency decomposition.

---

## 13) Practical exercises (do these first)

### Exercise 1
Predict the span shape for `gemini_rig_tools` on paper, then run it.

### Exercise 2
Add one extra `tracing::info_span!` around prompt construction.
Validate it appears as a child span.

### Exercise 3
Intentionally break `OTEL_EXPORTER_OTLP_ENDPOINT` then fix it.
Observe missing traces and recovery behavior.

Keep one notebook with:
- what changed
- what trace shape changed
- what you learned

---

## 14) Concept + syntax + design-pattern deepening

This section connects your current examples to reusable architecture patterns so you can build the next workflow faster.

### 14.1 Pattern: make control-flow visible first

An LLM request often fails where most people look first: around orchestration.

- Trace = one request.
- Spans = steps.
- Parent-child tree = order + causality.

If you can draw the tree on paper, you can usually find missing instrumentation quickly.

### 14.2 Pattern: telemetry bootstrap module (singleton + lifetime anchor)

Your `init_telemetry(...)` is the same idea:

1. Setup once at startup.
2. Keep provider alive for app lifetime.
3. Flush on shutdown.

Why it matters for beginners:

- global provider lets Rig and your own spans use one tracing plane.
- missing this step causes half the request to run without telemetry.

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = otel::init_telemetry("rig-gemini")?;
    // ... run app ...
    provider.shutdown()?;
    Ok(())
}
```

### 14.3 Pattern: layered subscriber composition

You compose behavior instead of hardcoding one output:

- `EnvFilter` = runtime filtering.
- `fmt` layer = local text logs.
- `tracing_opentelemetry` layer = export to OTel.

That composition is what lets you add new outputs (JSON logs, metrics, test subscribers) without rewriting your spans.

### 14.4 Pattern: workflow span taxonomy (names vs attributes vs events)

For agent systems, this is the highest leverage rule:

- **Span names** = stable operation names (`agent.planner`, `tool.add_numbers`).
- **Attributes** = searchable facts (`model`, `agent_role`, `tool_name`).
- **Events** = explanatory notes (`prompt selected`, `tool returned 100`).

Rule for readability:

- If you need to group many traces by it, use attributes.
- If it is narrative, keep it in events.

### 14.5 Pattern: context continuity (async-safe parentage)

In async Rust, you can accidentally break context and get floating spans.

Use explicit span scoping when needed:

```rust
let span = tracing::info_span!("agent.planner", model = "gemini-2.5-pro");
let span_guard = span.enter();
// run planner work with that current span context
```

Your current examples rely on the library-provided context for most operations; this pattern becomes important when you create manually spawned tasks.

### 14.6 Pattern: export topology choices

You currently can export in two common ways:

- App -> SigNoz Cloud OTLP directly.
- App -> OpenTelemetry Collector -> SigNoz backend.

OTLP ports you should remember:

- `4317` for gRPC
- `4318` for HTTP

Use environment variables to switch destinations without code changes.

### 14.7 Pattern: resource identity vs request identity

Use:
- service/resource attributes for service-level identity (`service.name`, environment metadata),
- span attributes/events for request-level details.

### 14.8 Pattern: telemetry hygiene and prompt safety

Never use unbounded prompt text in high-cardinality fields.
Prefer lengths/hashes/redacted snippets unless you have a strict policy.

Why this matters:

- better dashboard performance,
- lower storage cost,
- less risk of leaking sensitive content.

### 14.9 High-signal target trace shape (what to aim for)

For one multi-agent request, a practical ideal is:

```text
rig.request
├─ agent.planner (custom)
│  └─ gen_ai.chat (provider/model call)
├─ tool.add_numbers (custom)
└─ agent.writer (custom)
   └─ gen_ai.chat (provider/model call)
```

That shape is useful because every major step becomes queryable and comparable.

Level-up goal:

- Keep your orchestration spans (`agent.planner`, `agent.writer`) in stable names.
- Keep model spans aligned to GenAI semantic fields where possible (model/provider/operation).
- Enforce prompt/PII hygiene in one place (prefer collector processors when possible).

---

## 14.10 Staged path: onboarding depth vs production rigor (same architecture)

Use this comparison to avoid overengineering when learning, then harden safely.

| Focus | Onboarding profile | Production rigor |
| --- | --- | --- |
| Initialization | `init_telemetry(...)` in each example, run once at startup | Shared initializer used by every binary/service with env-based overrides |
| Span strategy | One root span + one or two child spans | Full phase taxonomy: request, planner, tool, writer, parse, render |
| Attributes | `model`, `agent`, `topic`, `tool_name` | Add standardized keys (`gen_ai.provider.name`, `gen_ai.operation.name`, `gen_ai.request.model`) and request metadata |
| Error handling | Print/log error and stop | Capture status/error in spans; include retry counters and provider status |
| Data safety | Avoid high-cardinality names | Redact prompts in app or Collector processor; include only lengths/hashes unless explicitly allowed |
| Export | One OTLP endpoint, default batching | Tuned batching, compression, timeouts, and collector-based buffering/retry |
| Shutdown | Optional or manual | Graceful shutdown hook always calls provider `shutdown()` |
| Verification | “Do I see a trace?” | “Does the trace shape match expected flow and latency budget?” |

### Starter template (smallest guaranteed working shape)

```rust
#[tracing::instrument(name = "request")]
async fn run_request() -> anyhow::Result<String> {
    let client = rig::providers::gemini::Client::from_env();
    let agent = client
        .agent("gemini-2.5-flash")
        .preamble("Be concise and technical.")
        .build();

    tracing::info!(model = "gemini-2.5-flash", "starting request");
    let answer = agent.prompt("Explain this in 3 bullets.").await?;
    tracing::info!(response_len = answer.len(), "received response");
    Ok(answer)
}
```

### Production template (same flow, production-grade durability)

```rust
#[tracing::instrument(
    name = "rig.request",
    fields(
        gen_ai_operation_name = "chat.completion",
        gen_ai_provider_name = "gemini",
        gen_ai_request_model = "gemini-2.5-flash",
        topic = %topic,
        request_id = %uuid::Uuid::new_v4(),
    )
)]
async fn run_request(topic: &str) -> anyhow::Result<String> {
    let span = tracing::info_span!(
        "agent.planner",
        attempt = 0u32,
        gen_ai_operation_name = "chat.completion",
        model = "gemini-2.5-flash",
    );
    let _guard = span.enter();

    tracing::info!(event = "planner_started", step = "generate_plan");
    let client = rig::providers::gemini::Client::from_env();
    let planner = client
        .agent("gemini-2.5-pro")
        .preamble("You are a planner. Return a structured plan.")
        .build();

    let plan = planner
        .prompt(format!("Plan for topic: {topic}"))
        .await
        .context("planner failed")?;

    tracing::info!(event = "planner_completed", plan_len = plan.len());

    let writer_span = tracing::info_span!("agent.writer", attempt = 0u32);
    let _writer_guard = writer_span.enter();
    let writer = client
        .agent("gemini-2.5-flash")
        .preamble("You are a concise writer.")
        .build();

    let final_text = writer
        .prompt(format!("Summarize plan: {plan}"))
        .await
        .context("writer failed")?;

    tracing::info!(event = "request_completed", output_len = final_text.len());
    Ok(final_text)
}
```

### What to switch first when moving from early implementation to production

1. Add request IDs in span fields.
2. Add one stable span per phase (`agent.planner`, `agent.writer`, `tool.*`).
3. Add structured error fields (`error_type`, `error_stage`, retry counters).
4. Move secret/prompt policies to collector configuration.
5. Add graceful shutdown and validate `shutdown()` is always called.
6. Review trace shape in SigNoz after each change.

---

## 15) Quick reference checklist

- one stable root span per request
- one span per meaningful phase
- stable span names, low-cardinality attributes
- events for detail, not names
- explicit startup/shutdown lifecycle
- validate by drawing and matching trace tree

---

## 16) External reading list (official and practical)

### OpenTelemetry fundamentals

- Traces concept: https://opentelemetry.io/docs/concepts/signals/traces/
- Context propagation: https://opentelemetry.io/docs/concepts/context-propagation/
- Resource concept: https://opentelemetry.io/docs/concepts/resources/
- Semantic conventions: https://opentelemetry.io/docs/concepts/semantic-conventions/
- Environment variables: https://opentelemetry.io/docs/specs/otel/configuration/sdk-environment-variables/

### Rust + OpenTelemetry

- tracing-opentelemetry layer docs: https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/struct.OpenTelemetryLayer.html
- OTLP crate docs: https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/
- EnvFilter docs: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
- Global API docs: https://docs.rs/opentelemetry/latest/opentelemetry/global/index.html
- OTLP exporter config and SDK behavior: https://opentelemetry.io/docs/languages/sdk-configuration/otlp-exporter/
- OTel Rust getting started: https://opentelemetry.io/docs/languages/rust/getting-started/

### GenAI observability

- GenAI semantic conventions: https://opentelemetry.io/docs/specs/semconv/gen-ai/
- GenAI LLM examples: https://opentelemetry.io/docs/specs/semconv/gen-ai/non-normative/examples-llm-calls/
- GenAI metrics conventions: https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-metrics/

### SigNoz

- Rust OpenTelemetry instrumentation: https://signoz.io/docs/instrumentation/opentelemetry-rust/
- SigNoz OTLP collector config: https://signoz.io/docs/collection-agents/opentelemetry-collector/configuration/
- Cloud vs self-hosted ingestion: https://signoz.io/docs/ingestion/cloud-vs-self-hosted/
- SigNoz cloud ingestion: https://signoz.io/docs/ingestion/signoz-cloud/overview/
- Collector switch pattern: https://signoz.io/docs/opentelemetry-collection-agents/opentelemetry-collector/switch-to-collector/
- PII scrubbing guidance: https://signoz.io/docs/logs-management/guides/pii-scrubbing/

### Rig + Gemini

- Rig examples and provider patterns: https://docs.rig.rs/examples/model_providers/gemini
- Rig Gemini completion types: https://docs.rs/rig-core/latest/rig/providers/gemini/completion/index.html
- Rig project and provider docs: https://github.com/0xPlaygrounds/rig
- Gemini model reference: https://ai.google.dev/gemini-api/docs/models
