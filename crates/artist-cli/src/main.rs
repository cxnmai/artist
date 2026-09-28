use artist_core::engine::AgentEvent;
use artist_lua::Runtime;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut prompt = None;
    let mut config = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => prompt = args.next(),
            "--config" => config = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("Usage: artist -p \"your prompt\" [--config path.lua]");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("Usage: artist -p \"your prompt\" [--config path.lua]");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(prompt) = prompt.filter(|text: &String| !text.is_empty()) else {
        eprintln!("Usage: artist -p \"your prompt\" [--config path.lua]");
        return ExitCode::FAILURE;
    };
    let runtime = match Runtime::new(config.as_deref()) {
        Ok(runtime) => runtime,
        Err(message) => {
            emit(AgentEvent::Error { message });
            return ExitCode::FAILURE;
        }
    };
    if !runtime.has_model() {
        emit(AgentEvent::User { text: prompt });
        emit(AgentEvent::Error {
            message: "No model configured; a Lua model adapter is required.".into(),
        });
        return ExitCode::FAILURE;
    }
    let cwd = env::current_dir().expect("current directory exists");
    match runtime.run(prompt, &cwd, emit).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE, // run_turn emitted the error event
    }
}

fn emit(event: AgentEvent) {
    println!(
        "{}",
        serde_json::to_string(&event).expect("agent events serialize")
    );
}
