use artist_core::context::Conversation;
use artist_core::engine::AgentEvent;
use artist_lua::Runtime;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "Usage: artist [-p \"prompt\" ...] --config path.lua [--model model-id] [--reasoning level] [--list-models|--list-models-json]";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut prompts = Vec::new();
    let mut config = None;
    let mut model = None;
    let mut reasoning = None;
    let mut list_models = false;
    let mut list_models_json = false;
    let mut args = env::args().skip(1);
    let mut accepting_prompts = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let Some(prompt) = args
                    .next()
                    .filter(|text| !text.is_empty() && text != "-p" && text != "--config")
                else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };
                prompts.push(prompt);
                accepting_prompts = true;
            }
            "--model" => {
                let Some(name) = args.next().filter(|name| !name.is_empty()) else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };
                model = Some(name);
                accepting_prompts = false;
            }
            "--list-models" => {
                list_models = true;
                accepting_prompts = false;
            }
            "--list-models-json" => {
                list_models_json = true;
                accepting_prompts = false;
            }
            "--reasoning" => {
                let Some(level) = args.next().filter(|value| !value.is_empty()) else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };
                reasoning = Some(level);
                accepting_prompts = false;
            }
            "--config" => {
                let Some(path) = args.next() else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };
                config = Some(PathBuf::from(path));
                accepting_prompts = false;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ if accepting_prompts && !arg.starts_with('-') => prompts.push(arg),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    if prompts.is_empty() && !list_models && !list_models_json {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
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
        if runtime
            .run(
                &mut conversation,
                prompt,
                &model,
                info.adapter.as_deref(),
                mapped_level,
                info.thinking_format.as_deref(),
                info.requires_reasoning_content.unwrap_or(false),
                &cwd,
                emit,
            )
            .await
            .is_err()
        {
            return ExitCode::FAILURE; // run_turn emitted the error event
        }
    }
    ExitCode::SUCCESS
}

fn emit(event: AgentEvent) {
    println!(
        "{}",
        serde_json::to_string(&event).expect("agent events serialize")
    );
}
