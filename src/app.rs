use crate::constants::{
    BUFFER_DAYS, FETCH_DAYS, MINUTES_IN_DAY, NUM_DAYS, RESOLUTION_IN_MINS, ROWS_PER_HOUR,
    SCROLL_OFFSET_MINS, START_OFFSET, VIEWPORT_MINS,
};
use crate::event::{AppEvent, Event, EventHandler, EventsFetched};
use crate::{Calendar, Config, Result};

use chrono::{DateTime, Local, NaiveDate, TimeDelta, Timelike, Utc};
use google_calendar3::api::Event as CEvent;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};

#[derive(Debug, Clone)]
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

#[derive(Default)]
pub enum AppMode {
    #[default]
    Normal,
    Fetching,
}

pub struct App {
    pub running: bool,
    pub events: EventHandler,

    pub mode: AppMode,

    pub scroll_offset: u16,
    pub viewport_mins: u16,
    pub cal_event_nodes: Vec<EventNode>,
    pub start_date: NaiveDate,
    pub num_days: TimeDelta,
    pub now: DateTime<Local>,

    pub sel_event_id: Option<String>,
    pub cal: Calendar,
    pub loaded_start: NaiveDate,
    pub loaded_end: NaiveDate,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;

        let cal = Calendar::new(config.calendar_ids).await?;

        let yesterday = Local::now().date_naive() - START_OFFSET;

        let mut app = App {
            running: true,
            mode: Default::default(),
            scroll_offset: SCROLL_OFFSET_MINS,
            viewport_mins: VIEWPORT_MINS,
            events: Default::default(),
            cal_event_nodes: Default::default(),
            sel_event_id: Default::default(),
            start_date: yesterday,
            num_days: NUM_DAYS,
            cal,
            now: Local::now(),
            loaded_start: yesterday,
            loaded_end: yesterday,
        };

        app.fetch_events(yesterday, yesterday + app.num_days);

        Ok(app)
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
                    AppEvent::Quit => self.quit(),

                    // Scroll Vertically
                    AppEvent::ScrollUp => self.scroll_up(),
                    AppEvent::ScrollDown => self.scroll_down(),
                    AppEvent::ScrollUpBig => self.big_scroll_up(),
                    AppEvent::ScrollDownBig => self.big_scroll_down(),

                    // Scroll Horizontally
                    AppEvent::ScrollLeft => self.scroll_left(),
                    AppEvent::ScrollRight => self.scroll_right(),

                    // Jump to current time
                    AppEvent::JumpToNow => self.jumpt_to_current_time(),

                    // Fetch Events
                    AppEvent::FetchSuccess(events_fetched) => self.update_events(events_fetched),
                    AppEvent::FetchFailed(_) => self.mode = Default::default(),
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
            // Scroll vertically
            KeyCode::Char('k') | KeyCode::Up => self.events.send(AppEvent::ScrollUp),
            KeyCode::Char('j') | KeyCode::Down => self.events.send(AppEvent::ScrollDown),
            KeyCode::Char('K') => self.events.send(AppEvent::ScrollUpBig),
            KeyCode::Char('J') => self.events.send(AppEvent::ScrollDownBig),

            // Scroll horizontally
            KeyCode::Char('h') => self.events.send(AppEvent::ScrollLeft),
            KeyCode::Char('l') => self.events.send(AppEvent::ScrollRight),

            // Jump to current time
            KeyCode::Char('t') => self.events.send(AppEvent::JumpToNow),
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
    pub fn fetch_events(&mut self, start_date: NaiveDate, end_date: NaiveDate) {
        if let AppMode::Fetching = self.mode {
            return;
        }
        self.mode = AppMode::Fetching;

        let cal_clone = self.cal.clone();
        let sender = self.events.sender.clone();

        tokio::spawn(async move {
            match cal_clone.get_events(start_date, end_date).await {
                Ok(events) => {
                    let event_nodes = events
                        .into_iter()
                        .filter_map(|e| e.try_into().ok())
                        .collect();
                    let _ = sender.send(Event::App(AppEvent::FetchSuccess(EventsFetched {
                        event_nodes,
                        start_date,
                        end_date,
                    })));
                }
                Err(e) => {
                    let _ = sender.send(Event::App(AppEvent::FetchFailed(e.to_string())));
                }
            }
        });
    }

    /// Transforms events to convenient structure
    pub fn update_events(&mut self, mut events_fetched: EventsFetched) {
        self.cal_event_nodes.append(&mut events_fetched.event_nodes);
        self.cal_event_nodes.sort_by_key(|e| e.start_time);

        self.loaded_start = self.loaded_start.min(events_fetched.start_date);
        self.loaded_end = self.loaded_end.max(events_fetched.end_date);

        self.mode = Default::default();
    }

    /// Scroll vertically
    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(RESOLUTION_IN_MINS);
    }
    fn scroll_down(&mut self) {
        let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);
        self.scroll_offset = (self.scroll_offset + RESOLUTION_IN_MINS).min(max_offset);
    }
    fn big_scroll_up(&mut self) {
        (0..ROWS_PER_HOUR).for_each(|_| self.scroll_up());
    }
    fn big_scroll_down(&mut self) {
        (0..ROWS_PER_HOUR).for_each(|_| self.scroll_down());
    }

    /// Scroll horizontally
    fn scroll_left(&mut self) {
        self.start_date -= TimeDelta::days(1);
        self.check_pagination();
    }
    fn scroll_right(&mut self) {
        self.start_date += TimeDelta::days(1);
        self.check_pagination();
    }

    // Pagination
    fn check_pagination(&mut self) {
        if let AppMode::Fetching = self.mode {
            return;
        }

        if self.start_date + self.num_days + BUFFER_DAYS >= self.loaded_end {
            self.fetch_events(self.loaded_end, self.loaded_end + FETCH_DAYS);
        } else if self.start_date - BUFFER_DAYS <= self.loaded_start {
            self.fetch_events(self.loaded_start - FETCH_DAYS, self.loaded_start);
        }
    }

    // Jumps to calendar to Today and centers the current time in the viewport
    pub fn jumpt_to_current_time(&mut self) {
        self.now = Local::now();

        self.start_date = self.now.date_naive() - START_OFFSET;

        let current_mins = self.now.hour() * 60 + self.now.minute();

        let half_viewport = self.viewport_mins / 2;

        let target_offset = (current_mins as u16).saturating_sub(half_viewport);

        let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);

        self.scroll_offset = target_offset.min(max_offset);
    }
}
