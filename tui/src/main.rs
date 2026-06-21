mod app;
mod byte_display;
mod input;
mod render;

use std::fs;
use std::io::{self, IsTerminal, Read};
use std::time::Duration;

use crossterm::{
    event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let (buf, filename, editable): (Vec<u8>, String, bool) = if args.len() > 1 {
        let path = &args[1];
        (fs::read(path)?, path.clone(), false)
    } else if io::stdin().is_terminal() {
        // No file argument and stdin is a tty: start in scratch (editable) mode.
        (Vec::new(), "<scratch>".to_string(), true)
    } else {
        // Read file content from stdin; crossterm falls back to /dev/tty for key events.
        let mut data = Vec::new();
        io::stdin().read_to_end(&mut data)?;
        (data, "<stdin>".to_string(), false)
    };

    let mut app = App::new(buf, filename, editable);

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend  = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let result = run_loop(&mut term, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)?;
    term.show_cursor()?;

    result
}

fn run_loop(
    term: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app:  &mut App,
) -> anyhow::Result<()> {
    loop {
        term.draw(|f| render::draw(f, app))?;

        // Poll with a short timeout so the debounce tick fires even without keypresses.
        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            input::handle_event(app, ev);
        }

        app.tick();

        if app.quit {
            break;
        }
    }
    Ok(())
}
