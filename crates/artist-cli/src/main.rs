use artist_core::engine::AgentEvent;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next(), args.next()) {
        (Some("-p"), Some(prompt), None) if !prompt.is_empty() => {
            emit(AgentEvent::User { text: prompt });
            emit(AgentEvent::Error {
                message: "No model configured; a Lua model adapter is required.".into(),
            });
            ExitCode::FAILURE
        }
        (Some("-h" | "--help"), None, None) => {
            println!("Usage: artist -p \"your prompt\"");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: artist -p \"your prompt\"");
            ExitCode::FAILURE
        }
    }
}

fn emit(event: AgentEvent) {
    println!(
        "{}",
        serde_json::to_string(&event).expect("agent events serialize")
    );
}
