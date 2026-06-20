mod app;
mod byte_display;
mod input;
mod render;

use std::fs;
use std::io::{self, Read};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let (buf, filename): (Vec<u8>, String) = if args.len() > 1 {
        let path = &args[1];
        (fs::read(path)?, path.clone())
    } else {
        // Read file content from stdin; open /dev/tty for keyboard events.
        let mut data = Vec::new();
        io::stdin().read_to_end(&mut data)?;
        (data, "<stdin>".to_string())
    };

    // If content came from stdin, crossterm will fall back to /dev/tty automatically
    // for key events because stdin is not a tty.

    let mut app = App::new(buf, filename);

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend  = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let result = run_loop(&mut term, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
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
