use crate::Result;
use crate::constants::{
    MINUTES_IN_HOUR, RESOLUTION_IN_MINS, ROWS_PER_HOUR, SCROLL_OFFSET_MINS, VIEWPORT_MINS,
};
use crate::event::{AppEvent, Event, EventHandler};

use chrono::{DateTime, Utc};
use google_calendar3::api::Event as CEvent;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};

#[derive(Debug)]
pub struct EventNode {
    pub id: String,
    pub summary: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

impl TryFrom<CEvent> for EventNode {
    type Error = ();

    fn try_from(cal_event: CEvent) -> std::result::Result<Self, Self::Error> {
        let event_start_datetime = cal_event.start.and_then(|d| d.date_time);
        let event_end_datetime = cal_event.end.and_then(|d| d.date_time);

        let (Some(event_start_datetime), Some(event_end_datetime)) =
            (event_start_datetime, event_end_datetime)
        else {
            return Err(());
        };

        let summary = cal_event.summary.unwrap_or_else(|| "Untitled".to_string());

        Ok(EventNode {
            id: cal_event.id.unwrap(),
            summary,
            start_time: event_start_datetime,
            end_time: event_end_datetime,
        })
    }
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub events: EventHandler,

    pub scroll_offset: u16,
    pub viewport_mins: u16,
    pub cal_event_nodes: Vec<EventNode>,

    pub sel_event_id: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            scroll_offset: SCROLL_OFFSET_MINS,
            viewport_mins: VIEWPORT_MINS,
            events: EventHandler::new(),
            cal_event_nodes: Default::default(),
            sel_event_id: Default::default(),
        }
    }
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new() -> Self {
        App::default()
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
                    AppEvent::ScrollUp => self.scroll_up(),
                    AppEvent::ScrollDown => self.scroll_down(),
                    AppEvent::ScrollUpBig => self.big_scroll_up(),
                    AppEvent::ScrollDownBig => self.big_scroll_down(),
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
            // Other handlers you could add here.
            KeyCode::Char('k') | KeyCode::Up => self.events.send(AppEvent::ScrollUp),
            KeyCode::Char('j') | KeyCode::Down => self.events.send(AppEvent::ScrollDown),
            KeyCode::Char('K') => self.events.send(AppEvent::ScrollUpBig),
            KeyCode::Char('J') => self.events.send(AppEvent::ScrollDownBig),
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

    /// Transforms events to convenient structure
    pub fn update_events(&mut self, events: Vec<CEvent>) {
        self.cal_event_nodes = events
            .into_iter()
            .filter_map(|e| e.try_into().ok())
            .collect();
    }

    /// Scroll Calendar
    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(RESOLUTION_IN_MINS);
    }
    fn scroll_down(&mut self) {
        let max_offset = MINUTES_IN_HOUR.saturating_sub(self.viewport_mins);
        self.scroll_offset = (self.scroll_offset + RESOLUTION_IN_MINS).min(max_offset);
    }
    fn big_scroll_up(&mut self) {
        (0..ROWS_PER_HOUR).for_each(|_| self.scroll_up());
    }
    fn big_scroll_down(&mut self) {
        (0..ROWS_PER_HOUR).for_each(|_| self.scroll_down());
    }
}
