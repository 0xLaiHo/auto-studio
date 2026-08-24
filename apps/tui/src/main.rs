mod app;
mod client;
mod constants;
mod error;
mod launch;
mod model;
mod ui;

use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::constants::{EVENT_POLL_INTERVAL, HELP_TEXT};
use crate::error::TuiError;
use crate::launch::CoreSession;

#[tokio::main]
async fn main() -> Result<(), TuiError> {
    match startup_mode(std::env::args_os().skip(1))? {
        StartupMode::Help => {
            println!("{HELP_TEXT}");
            return Ok(());
        }
        StartupMode::Version => {
            println!("autostudio {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        StartupMode::Tui => {}
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(TuiError::InteractiveTerminalRequired);
    }
    let session = CoreSession::connect_or_launch().await?;
    let mut terminal = ratatui::try_init()?;
    let result = run(&mut terminal, session.client()).await;
    ratatui::restore();
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupMode {
    Tui,
    Help,
    Version,
}

fn startup_mode(arguments: impl IntoIterator<Item = OsString>) -> Result<StartupMode, TuiError> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(StartupMode::Tui),
        [argument] if argument == "--help" || argument == "-h" => Ok(StartupMode::Help),
        [argument] if argument == "--version" || argument == "-V" => Ok(StartupMode::Version),
        [argument, ..] => Err(TuiError::UnsupportedArgument(
            argument.to_string_lossy().into_owned(),
        )),
    }
}

async fn run(
    terminal: &mut DefaultTerminal,
    client: &crate::client::TuiClient,
) -> Result<(), TuiError> {
    let mut app = App::default();
    app.execute_effect(client, app::Effect::Refresh).await;
    let mut last_catalog_poll = Instant::now();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;
        if !event::poll(EVENT_POLL_INTERVAL)? {
            if app.catalog_is_refreshing()
                && last_catalog_poll.elapsed() >= Duration::from_millis(500)
            {
                app.execute_effect(client, app::Effect::PollProvider).await;
                last_catalog_poll = Instant::now();
            }
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != crossterm::event::KeyEventKind::Press {
            continue;
        }
        let action = App::action_for_key(key);
        let effect = app.reduce(action);
        app.execute_effect(client, effect).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{StartupMode, startup_mode};

    #[test]
    fn no_arguments_select_the_interactive_tui() {
        assert_eq!(
            startup_mode(Vec::<OsString>::new()).expect("startup mode"),
            StartupMode::Tui
        );
    }

    #[test]
    fn help_does_not_require_terminal_initialization() {
        assert_eq!(
            startup_mode([OsString::from("--help")]).expect("help mode"),
            StartupMode::Help
        );
    }
}
