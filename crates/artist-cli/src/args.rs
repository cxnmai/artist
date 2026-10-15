use std::{env, path::PathBuf};

pub const USAGE: &str = "Usage: artist -p \"prompt\" [-p \"next\"] [-r session-id] --config path.lua [--model model-id] [--reasoning level]\n       artist --config path.lua [--list-models|--list-models-json]\n       artist acp [--config path.lua] [--model model-id] [--reasoning level]";

#[derive(Default)]
pub struct Options {
    pub prompts: Vec<String>,
    pub config: Option<PathBuf>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub resume: Option<String>,
    pub list_models: bool,
    pub list_models_json: bool,
}

/// None means help was requested; errors contain a user-facing diagnostic.
pub fn parse() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = env::args().skip(1);
    let mut accepting_prompts = false;
    while let Some(arg) = args.next() {
        if matches!(arg.as_str(), "-h" | "--help") {
            return Ok(None);
        }
        if matches!(arg.as_str(), "--list-models" | "--list-models-json") {
            options.list_models |= arg == "--list-models";
            options.list_models_json |= arg == "--list-models-json";
            accepting_prompts = false;
            continue;
        }
        if matches!(
            arg.as_str(),
            "-p" | "-r" | "--config" | "--model" | "--reasoning"
        ) {
            let value = args
                .next()
                .filter(|value| {
                    !value.is_empty()
                        && if arg == "-p" {
                            !matches!(
                                value.as_str(),
                                "-p" | "-r" | "--config" | "--model" | "--reasoning"
                            )
                        } else {
                            !value.starts_with('-')
                        }
                })
                .ok_or_else(|| format!("{arg} requires a value\n{USAGE}"))?;
            accepting_prompts = arg == "-p";
            match arg.as_str() {
                "-p" => options.prompts.push(value),
                "-r" => options.resume = Some(value),
                "--config" => options.config = Some(value.into()),
                "--model" => options.model = Some(value),
                "--reasoning" => options.reasoning = Some(value),
                _ => unreachable!(),
            }
        } else if accepting_prompts && !arg.starts_with('-') {
            options.prompts.push(arg);
        } else {
            return Err(format!("unknown argument: {arg}\n{USAGE}"));
        }
    }
    if options.prompts.is_empty() && !options.list_models && !options.list_models_json {
        return Err(USAGE.into());
    }
    if options.resume.is_some() && (options.list_models || options.list_models_json) {
        return Err("-r cannot be combined with model listing".into());
    }
    Ok(Some(options))
}
