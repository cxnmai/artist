use crate::{Result, metrics, open};
use artist_core::{
    context::ContextEntry,
    journal::{Journal, SessionRecord},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

pub async fn child(directory: &Path, count: usize) -> Result<()> {
    let session = open(directory, None, 0).await?;
    metrics::measure("child_durable_append", count, count, || async {
        Ok(session
            .append(SessionRecord::Message {
                entry: ContextEntry::User {
                    text: "c".repeat(1000),
                },
            })
            .await?)
    })
    .await
}

pub async fn run(root: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    for processes in [1, 4] {
        let directory = root.join(format!("concurrency-{processes}"));
        let start = Instant::now();
        let mut children = Vec::new();
        for _ in 0..processes {
            children.push(
                Command::new(&executable)
                    .arg("--child")
                    .arg(&directory)
                    .arg("50")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?,
            );
        }
        for child in children {
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
            }
            print!("{}", String::from_utf8(output.stdout)?);
        }
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "{}",
            serde_json::json!({"operation":"multiprocess", "processes":processes,
                "appends": processes*50, "seconds": seconds, "appends_per_second": (processes*50) as f64 / seconds,
            })
        );
    }
    Ok(())
}
