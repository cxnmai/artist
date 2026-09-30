//! Validate a standalone summary response without adding it to chat history.

use artist_core::context::{AssistantBlock, ContextEntry, SelectedContext};
use artist_core::display::DisplayModelEvent;
use artist_core::engine::Model;
use artist_core::event::ModelEvent;
use artist_core::response::ResponseAccumulator;

pub async fn generate(
    model: &mut dyn Model,
    input: &SelectedContext,
) -> Result<(String, Vec<DisplayModelEvent>), String> {
    let mut response = ResponseAccumulator::new();
    let mut response_error = None;
    let mut finished = false;
    let mut usage = Vec::new();
    model
        .generate(input, &[], &mut |events| {
            for event in events {
                match &event {
                    ModelEvent::ToolCallStart { .. } => {
                        response_error = Some("summarization attempted a tool call".into())
                    }
                    ModelEvent::Finished { reason } => {
                        finished = true;
                        if !matches!(reason.as_deref(), Some("stop" | "end_turn" | "completed")) {
                            response_error = Some(format!("incomplete summarization: {reason:?}"));
                        }
                    }
                    ModelEvent::Usage { .. } => {
                        if let Some(event) = DisplayModelEvent::from_model(event.clone()) {
                            usage.push(event);
                        }
                    }
                    _ => {}
                }
                if let Err(error) = response.push(event) {
                    response_error = Some(error.to_string());
                }
            }
        })
        .await?;
    if let Some(error) = response_error {
        return Err(error);
    }
    if !finished {
        return Err("summarization ended without a completion marker".into());
    }
    let ContextEntry::Assistant { blocks } = response.finish().map_err(|e| e.to_string())? else {
        unreachable!()
    };
    let summary = blocks
        .iter()
        .filter_map(|block| match block {
            AssistantBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    if summary.trim().is_empty() {
        return Err("summarization returned empty text".into());
    }
    Ok((summary, usage))
}
