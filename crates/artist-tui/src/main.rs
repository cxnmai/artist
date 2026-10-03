mod actions;
mod app;
mod args;
mod backend;
mod colors;
mod components;
mod input;
mod input_layout;
mod mode;
mod terminal;
mod ui;

use std::io;

use crossterm::event::{Event, EventStream};
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
    run(&mut session.terminal, options).await
}

async fn run(terminal: &mut ratatui::DefaultTerminal, options: Options) -> io::Result<()> {
    let cwd = std::env::current_dir()?;
    let mut app = App::new(cwd.clone(), options.config.is_some());
    let initialization = Backend::load(&options, &cwd);
    tokio::pin!(initialization);
    let mut initializing = true;
    let mut events = EventStream::new();
    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;
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
                Some(Ok(_)) => {}, // Resize redraws on the next iteration.
                Some(Err(error)) => return Err(error),
                None => return Ok(()),
            },
            result = &mut initialization, if initializing => {
                initializing = false;
                app.attach_backend(result.map_err(io::Error::other)?);
            }
        }
    }
}
