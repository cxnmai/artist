use artist_core::context::Conversation;
use artist_core::engine::AgentEvent;
use artist_lua::Runtime;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "Usage: artist [-p \"prompt\" ...] --config path.lua [--model model-id] [--list-models]";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut prompts = Vec::new();
    let mut config = None;
    let mut model = None;
    let mut list_models = false;
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
    if prompts.is_empty() && !list_models {
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
    if list_models {
        match runtime.list_models().await {
            Ok(models) => {
                for model in models {
                    println!("{model}");
                }
                return ExitCode::SUCCESS;
            }
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        }
    }
    let model = match runtime.select_model(model.as_deref()) {
        Ok(model) => model,
        Err(message) => {
            emit(AgentEvent::Error { message });
            return ExitCode::FAILURE;
        }
    };
    let cwd = env::current_dir().expect("current directory exists");
    let mut conversation = Conversation::default();
    if let Err(message) = runtime.configure_conversation(&mut conversation, &cwd, &model) {
        emit(AgentEvent::Error { message });
        return ExitCode::FAILURE;
    }
    for prompt in prompts {
        if runtime
            .run(&mut conversation, prompt, &model, &cwd, emit)
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
