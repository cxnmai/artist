//! Parse provider transport configuration without coupling models to one protocol.

use crate::registry::{ModelKind, Protocol};
use mlua::{Lua, LuaSerdeExt, Result, Table, Value};
use serde_json::Map;

pub fn parse_kind(lua: &Lua, table: &Table, parent: Option<&Table>) -> Result<ModelKind> {
    let adapter: Option<String> = table.get("adapter")?;
    let Some(adapter) = adapter else {
        if parent.is_some() {
            return Err(mlua::Error::external("named adapter needs an adapter type"));
        }
        return Ok(ModelKind::Lua {
            request: table.get("request")?,
            response: table.get("response")?,
        });
    };
    if table.contains_key("request")? || table.contains_key("response")? {
        return Err(mlua::Error::external(
            "use either adapter or request/response callbacks",
        ));
    }
    let (protocol, default_endpoint) = match adapter.as_str() {
        "chat_completions" => (
            Protocol::ChatCompletions,
            "https://api.openai.com/v1/chat/completions",
        ),
        "openai_responses" => (
            Protocol::OpenAIResponses,
            "https://api.openai.com/v1/responses",
        ),
        "anthropic_messages" => (
            Protocol::AnthropicMessages,
            "https://api.anthropic.com/v1/messages",
        ),
        _ => {
            return Err(mlua::Error::external(format!(
                "unknown model adapter: {adapter}"
            )));
        }
    };
    let auth: Option<Table> = table
        .get::<Option<Table>>("auth")?
        .or(parent.map(|p| p.get("auth")).transpose()?.flatten());
    let bearer_env: Option<String> = auth
        .as_ref()
        .map(|t| t.get("bearer_env"))
        .transpose()?
        .flatten();
    let api_key_env: Option<String> = auth
        .as_ref()
        .map(|t| t.get("api_key_env"))
        .transpose()?
        .flatten();
    let headers: Option<Table> = table
        .get::<Option<Table>>("headers")?
        .or(parent.map(|p| p.get("headers")).transpose()?.flatten());
    let mut options = Map::new();
    if let Some(value) = table.get::<Option<Value>>("options")? {
        options.extend(lua.from_value::<Map<String, serde_json::Value>>(value)?);
    }
    Ok(ModelKind::Http {
        protocol,
        endpoint: table
            .get::<Option<String>>("endpoint")?
            .unwrap_or_else(|| default_endpoint.into()),
        bearer_env,
        api_key_env,
        headers,
        options,
    })
}
