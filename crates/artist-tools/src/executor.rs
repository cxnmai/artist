//! Dispatch JSON tool calls to the built-in implementations.

use crate::{bash, edit, read, write};
use artist_core::tools::{ToolExecutor, ToolOutput};
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

pub struct BuiltinTools;

#[derive(Deserialize)]
struct BashInput {
    command: String,
    timeout: Option<f64>,
}

impl ToolExecutor for BuiltinTools {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        cwd: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ToolOutput> + 'a>> {
        Box::pin(async move {
            match run(name, arguments, cwd).await {
                Ok(output) => output,
                Err(message) => ToolOutput::error(message),
            }
        })
    }
}

async fn run(name: &str, arguments: &Value, cwd: &Path) -> Result<ToolOutput, String> {
    match name {
        "read" => {
            let args: read::ReadArgs = parse(arguments)?;
            let result = read::read(&args, cwd).map_err(|e| e.to_string())?;
            let mut text = result.text;
            if let Some(next) = result.next_offset {
                text.push_str(&format!(
                    "\n\n[More lines available. Use offset={next} to continue.]"
                ));
            }
            Ok(ToolOutput::success(text))
        }
        "edit" => {
            let args: edit::EditArgs = parse(arguments)?;
            let result = edit::edit(&args, cwd).map_err(|e| e.to_string())?;
            Ok(ToolOutput::success(format!(
                "Successfully replaced {} block(s) in {}.",
                result.replacements,
                args.path.display()
            )))
        }
        "write" => {
            let args: write::WriteArgs = parse(arguments)?;
            write::write(&args, cwd).map_err(|e| e.to_string())?;
            Ok(ToolOutput::success(format!(
                "Wrote {}.",
                args.path.display()
            )))
        }
        "bash" => {
            let input: BashInput = parse(arguments)?;
            let timeout = input
                .timeout
                .map(|seconds| {
                    if !seconds.is_finite() || seconds <= 0.0 || seconds >= u64::MAX as f64 {
                        Err("timeout must be a positive finite number of seconds".to_owned())
                    } else {
                        Ok(Duration::from_secs_f64(seconds))
                    }
                })
                .transpose()?;
            let args = bash::BashArgs {
                command: input.command,
                timeout,
            };
            let result = bash::bash(&args, cwd).await.map_err(|e| e.to_string())?;
            let mut text = result.output;
            if let Some(path) = result.full_output_path {
                text.push_str(&format!("\n\n[Full output: {}]", path.display()));
            }
            if let Some(code) = result.exit_code.filter(|&code| code != 0) {
                text.push_str(&format!("\n\nCommand exited with code {code}"));
            }
            Ok(ToolOutput {
                text,
                is_error: result.exit_code != Some(0),
                ui: None,
            })
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn parse<T: serde::de::DeserializeOwned>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|e| e.to_string())
}
