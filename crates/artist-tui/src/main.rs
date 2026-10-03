mod actions;
mod app;
mod app_backend;
mod args;
mod backend;
mod backend_worker;
mod chat;
mod chat_block;
mod colors;
mod components;
mod input;
mod input_layout;
mod mode;
mod response_view;
mod terminal;
mod ui;

use std::io;

use crossterm::event::{Event, EventStream, MouseEventKind};
use futures_util::StreamExt;

use app::App;
use args::Options;
use backend::Backend;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let mut session = terminal::Session::new()?;
    tokio::task::LocalSet::new()
        .run_until(run(&mut session.terminal, options))
        .await
}

async fn run(terminal: &mut ratatui::DefaultTerminal, options: Options) -> io::Result<()> {
    let cwd = std::env::current_dir()?;
    let mut app = App::new(cwd.clone(), options.config.is_some());
    let initialization = Backend::load(&options, &cwd);
    tokio::pin!(initialization);
    let mut initializing = true;
    let mut events = EventStream::new();
    let (backend_events, mut incoming) = tokio::sync::mpsc::unbounded_channel();
    loop {
        terminal.draw(|frame| {
            let areas = ui::areas(frame.area(), &app);
            app.chat.prepare(areas.chat.width, areas.chat.height);
            ui::render(frame, &app, areas);
        })?;
        // Lua stays on this thread; provider discovery never blocks keyboard input.
        tokio::select! {
            biased;
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => {
                    if let Some(action) = actions::from_key(key, app.mode) {
                        if app.act(action) { return Ok(()); }
                    }
                }
                Some(Ok(Event::Paste(text))) => { app.act(actions::Action::PasteInput(text)); },
                Some(Ok(Event::Mouse(mouse))) => match mouse.kind {
                    MouseEventKind::ScrollUp => { app.act(actions::Action::ScrollChat(-3)); },
                    MouseEventKind::ScrollDown => { app.act(actions::Action::ScrollChat(3)); },
                    _ => {},
                },
                Some(Ok(_)) => {}, // Resize redraws on the next iteration.
                Some(Err(error)) => return Err(error),
                None => return Ok(()),
            },
            Some(batch) = incoming.recv() => app.apply_events(batch),
            result = &mut initialization, if initializing => {
                initializing = false;
                app.attach_backend(result.map_err(io::Error::other)?, backend_events.clone());
            }
        }
    }
}
