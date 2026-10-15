use artist_core::cancellation::CancellationToken;
use artist_core::context::Conversation;
use artist_core::engine::{AgentEvent, TurnOutcome};
use artist_lua::Runtime;
use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

mod acp;
mod args;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if env::args().nth(1).as_deref() == Some("acp") {
        return acp::run().await;
    }
    let options = match args::parse() {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let args::Options { prompts, config, model, reasoning, resume, list_models, list_models_json } = options;
    let runtime = match Runtime::new(config.as_deref()) {
        Ok(runtime) => runtime,
        Err(message) => {
            emit(AgentEvent::Error { message });
            return ExitCode::FAILURE;
        }
    };
    let (provider, registry) = match runtime.model_registry().await {
        Ok(registry) => registry,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    if list_models_json {
        println!(
            "{}",
            serde_json::to_string(&registry).expect("model registry serializes")
        );
        return ExitCode::SUCCESS;
    }
    if list_models {
        for model in registry {
            println!("{}", model.id);
        }
        return ExitCode::SUCCESS;
    }
    let model = match runtime.select_model(model.as_deref()) {
        Ok(model) => model,
        Err(message) => {
            emit(AgentEvent::Error { message });
            return ExitCode::FAILURE;
        }
    };
    let Some(info) = registry.into_iter().find(|info| info.id == model) else {
        emit(AgentEvent::Error {
            message: format!("model {model} is not in the provider registry"),
        });
        return ExitCode::FAILURE;
    };
    let level = reasoning.or(info.default_reasoning.clone());
    if let Some(level) = &level {
        if info
            .reasoning_levels
            .as_ref()
            .is_none_or(|levels| !levels.contains(level))
        {
            emit(AgentEvent::Error {
                message: format!("reasoning level {level} is not listed as supported by {model}"),
            });
            return ExitCode::FAILURE;
        }
    }
    let mapped_level = level.as_deref().map(|value| {
        info.reasoning_map
            .as_ref()
            .and_then(|map| map.get(value))
            .map(String::as_str)
            .unwrap_or(value)
    });
    let cwd = env::current_dir().expect("current directory exists");
    let mut conversation = Conversation::default();
    if let Err(message) = runtime.configure_conversation(&mut conversation, &cwd, &model) {
        emit(AgentEvent::Error { message });
        return ExitCode::FAILURE;
    }
    emit(AgentEvent::ModelInfo {
        provider,
        model: model.clone(),
        context_window: info.context_window,
        max_output_tokens: info.max_output_tokens,
        reasoning_levels: info.reasoning_levels,
        reasoning_level: level.clone(),
    });
    for prompt in prompts {
        let cancellation = CancellationToken::new();
        let signal = cancellation.clone();
        let interrupt = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                signal.cancel();
                // A second interrupt is a hard stop if a tool cannot be interrupted cooperatively.
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(130);
                }
            }
        });
        let outcome = runtime
            .run(
                &mut conversation,
                prompt,
                &model,
                info.adapter.as_deref(),
                mapped_level,
                info.thinking_format.as_deref(),
                info.requires_reasoning_content.unwrap_or(false),
                info.context_window,
                &cwd,
                &cancellation,
                emit_batch,
            )
            .await;
        interrupt.abort();
        match outcome {
            Ok(TurnOutcome::Completed) => {}
            Ok(TurnOutcome::Cancelled) => return ExitCode::from(130),
            Err(_) => return ExitCode::FAILURE, // run_turn emitted the error event
        }
    }
    ExitCode::SUCCESS
}

fn emit(event: AgentEvent) {
    emit_batch(vec![event]);
}

// NDJSON is a CLI frontend; the engine passes typed event batches.
fn emit_batch(events: Vec<AgentEvent>) {
    let mut stdout = io::stdout().lock();
    for event in events {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&event).expect("agent events serialize")
        )
        .expect("write agent event");
    }
    stdout.flush().expect("flush agent events");
}
