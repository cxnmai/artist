use std::env;
use std::io;
use std::path::PathBuf;

const USAGE: &str = "Usage: artist-tui [--config path.lua] [--model model-id] [--reasoning level]";

#[derive(Default)]
pub struct Options {
    pub config: Option<PathBuf>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

impl Options {
    pub fn parse() -> io::Result<Option<Self>> {
        let mut options = Self::default();
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            if matches!(arg.as_str(), "-h" | "--help") {
                println!("{USAGE}");
                return Ok(None);
            }
            if !matches!(arg.as_str(), "--config" | "--model" | "--reasoning") {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
            }
            let value = args
                .next()
                .filter(|value| !value.is_empty() && !value.starts_with("--"))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("{arg} needs a value. {USAGE}"),
                    )
                })?;
            match arg.as_str() {
                "--config" => options.config = Some(value.into()),
                "--model" => options.model = Some(value),
                "--reasoning" => options.reasoning = Some(value),
                _ => unreachable!(),
            }
        }
        Ok(Some(options))
    }
}
