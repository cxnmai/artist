use artist_acp::{Options, serve};
use std::{env, path::PathBuf, process::ExitCode};

const USAGE: &str = "Usage: artist acp [--config path.lua] [--model model-id] [--reasoning level]\nServes Agent Client Protocol over stdin/stdout; diagnostics go to stderr.";

pub async fn run() -> ExitCode {
    let mut options = Options::default();
    let mut args = env::args().skip(2);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            eprintln!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        if !matches!(arg.as_str(), "--config" | "--model" | "--reasoning") {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
        let Some(value) = args
            .next()
            .filter(|value| !value.is_empty() && !value.starts_with("--"))
        else {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        };
        match arg.as_str() {
            "--config" => options.config = Some(PathBuf::from(value)),
            "--model" => options.model = Some(value),
            "--reasoning" => options.reasoning = Some(value),
            _ => unreachable!(),
        }
    }
    match serve(options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ACP: {message}");
            ExitCode::FAILURE
        }
    }
}
