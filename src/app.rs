use crate::constants::{SCROLL_OFFSET, VIEWPORT_HOURS};
use crate::event::{Event, EventHandler};
use crate::{Result, event::AppEvent};

use google_calendar3::api::Event as CEvent;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub scroll_offset: i8,
    pub viewport_hours: i8,
    pub events: EventHandler,
    pub cal_events: Vec<CEvent>,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new(cal_events: Vec<CEvent>) -> Self {
        Self {
            running: true,
            scroll_offset: SCROLL_OFFSET,
            viewport_hours: VIEWPORT_HOURS,
            events: EventHandler::new(),
            cal_events,
        }
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            match self.events.next().await? {
                Event::Tick => self.tick(),
                Event::Crossterm(event) => match event {
                    CrosstermEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                        self.handle_key_events(key_event)?
                    }
                    CrosstermEvent::Resize(_, _) => terminal.autoresize()?,
                    _ => {}
                },
                Event::App(app_event) => match app_event {
                    // AppEvent::Increment => self.increment_counter(),
                    // AppEvent::Decrement => self.decrement_counter(),
                    AppEvent::Quit => self.quit(),
                },
            }
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            // KeyCode::Right => self.events.send(AppEvent::Increment),
            // KeyCode::Left => self.events.send(AppEvent::Decrement),
            // Other handlers you could add here.
            _ => {}
        }
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    pub fn tick(&self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }
}
