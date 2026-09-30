//! Tool-free, buffered summarization using the currently selected model.

use crate::compaction_options::SummaryOptions;
use crate::model::LuaModel;
use crate::registry::Registry;
use artist_core::compaction::{CompactionResult, validate_tool_pairs};
use artist_core::compaction_text::{
    INSTRUCTIONS, SYSTEM_PROMPT, cut_point, replacement, serialize,
};
use artist_core::context::{ContextEntry, Conversation, SelectedContext};
use artist_model::ModelClient;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub fn install(
    lua: &Lua,
    registry: Rc<RefCell<Registry>>,
    client: ModelClient,
) -> mlua::Result<()> {
    let artist: Table = lua.globals().get("artist")?;
    let api = lua.create_table()?;
    api.set(
        "summarize",
        lua.create_async_function(move |lua, (context, options): (Value, Value)| {
            let registry = registry.clone();
            let client = client.clone();
            async move {
                let context: Conversation = lua.from_value(context)?;
                let options: SummaryOptions = lua.from_value(options)?;
                let result = summarize(&lua, &registry, client, context, options)
                    .await
                    .map_err(mlua::Error::external)?;
                match result {
                    Some(result) => lua.to_value(&result),
                    None => Ok(Value::Nil),
                }
            }
        })?,
    )?;
    artist.set("compaction", api)
}

async fn summarize(
    lua: &Lua,
    registry: &Rc<RefCell<Registry>>,
    client: ModelClient,
    context: Conversation,
    options: SummaryOptions,
) -> Result<Option<CompactionResult>, String> {
    options.settings.validate()?;
    validate_tool_pairs(&context.entries)?;
    let Some(cut) = cut_point(&context, options.settings.keep_recent_tokens) else {
        return Ok(None);
    };
    let provider = registry
        .borrow()
        .provider
        .clone()
        .ok_or("no provider configured for summarization")?;
    let budget = provider
        .model_info
        .get(&options.model_name)
        .and_then(|info| info.max_output_tokens)
        .map_or(options.settings.max_summary_tokens, |limit| {
            limit.min(options.settings.max_summary_tokens)
        });
    let mut model = LuaModel {
        lua,
        provider,
        client,
        model_name: options.model_name,
        adapter_name: options.adapter_name,
        reasoning_level: options.reasoning_level,
        thinking_format: options.thinking_format,
        requires_reasoning_content: options.requires_reasoning_content,
        compaction: None,
        summary_max_tokens: Some(budget),
    };
    let prompt = format!(
        "<conversation>\n{}\n</conversation>\n\n{INSTRUCTIONS}\n\n{}",
        serialize(&context.entries[..cut]),
        options.settings.instructions.as_deref().unwrap_or("")
    );
    let input = SelectedContext {
        system_prompt: SYSTEM_PROMPT.into(),
        entries: vec![ContextEntry::User { text: prompt }],
    };
    let (summary, usage) = crate::summary_response::generate(&mut model, &input).await?;
    let result = CompactionResult {
        context: replacement(&context, cut, &summary),
        summary,
        removed_entries: cut,
        usage,
    };
    result.validate(&context)?;
    Ok(Some(result))
}
