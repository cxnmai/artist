//! Cursor progress must survive pages containing no displayable messages.
use crate::{Result, open};
use artist_core::display::DisplayEntry;
use artist_core::{
    context::ContextEntry,
    journal::{Journal, SessionRecord},
};
use std::path::Path;

pub async fn run(root: &Path) -> Result<()> {
    let directory = root.join("paging-regression");
    let session = open(&directory, None, 0).await?;
    let mut records = vec![SessionRecord::Message {
        entry: ContextEntry::User {
            text: "before".into(),
        },
    }];
    records.extend((0..600).map(|_| SessionRecord::Usage { anchor: None }));
    records.push(SessionRecord::Message {
        entry: ContextEntry::Summary {
            text: "context only".into(),
        },
    });
    records.push(SessionRecord::Message {
        entry: ContextEntry::User {
            text: "after".into(),
        },
    });
    session.append_batch(records).await?;
    let mut after = 0;
    let mut messages = Vec::new();
    let mut empty_pages = 0;
    loop {
        let page = artist_session::transcript(Some(&directory), &session.id, after, 256).await?;
        if page.items.is_empty() {
            empty_pages += 1;
        }
        messages.extend(page.items.into_iter().map(|item| item.entry));
        match page.next_after {
            Some(next) => {
                assert!(next > after);
                after = next;
            }
            None => break,
        }
    }
    assert!(empty_pages > 0);
    assert_eq!(messages.len(), 2);
    assert!(matches!(&messages[0], DisplayEntry::User { text } if text == "before"));
    assert!(matches!(&messages[1], DisplayEntry::User { text } if text == "after"));
    println!(
        "{}",
        serde_json::json!({"check": "metadata_only_pages_advance_and_context_summaries_stay_hidden", "passed": true})
    );
    Ok(())
}
