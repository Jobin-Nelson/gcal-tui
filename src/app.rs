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

#[derive(Default, PartialEq)]
pub enum AppMode {
    #[default]
    Normal,
    Fetching,
    Insert,
}

#[derive(Debug)]
pub struct InsertEvent {
    pub start_time: DateTime<Utc>,
    pub duration: TimeDelta,
}

pub struct App {
    pub running: bool,
    pub events: EventHandler,

    pub mode: AppMode,

    // View events
    pub start_date: NaiveDate,
    pub scroll_offset: u16,
    pub viewport_mins: u16,
    pub cal_event_nodes: Vec<EventNode>,
    pub num_days: TimeDelta,
    pub now: DateTime<Local>,
    pub is_now_timeline_visible: bool,

    pub sel_event_id: Option<String>,

    // Load events
    pub cal: Calendar,
    pub loaded_start: NaiveDate,
    pub loaded_end: NaiveDate,

    // Insert event
    pub insert_event: InsertEvent,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;

        let cal = Calendar::new(config).await?;

        let now = Local::now();
        let start_date = now.date_naive() - START_OFFSET;

        let insert_event = InsertEvent {
            start_time: now.with_timezone(&Utc),
            duration: TimeDelta::minutes(RESOLUTION_IN_MINS as i64 * 2),
        };

        let mut app = App {
            running: true,
            mode: Default::default(),
            scroll_offset: SCROLL_OFFSET_MINS,
            viewport_mins: VIEWPORT_MINS,
            events: Default::default(),
            cal_event_nodes: Default::default(),
            sel_event_id: Default::default(),
            start_date,
            num_days: NUM_DAYS,
            cal,
            now,
            loaded_start: start_date,
            loaded_end: start_date,
            is_now_timeline_visible: true,
            insert_event,
        };

        app.fetch_events(
            start_date - BUFFER_DAYS,
            start_date + app.num_days + BUFFER_DAYS,
        );

        Ok(app)
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        if let Ok(size) = terminal.size() {
            self.update_viewport_from_height(size.height);
        }

        self.jump_to_current_time();

        while self.running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            match self.events.next().await? {
                Event::Tick => self.tick(),
                Event::Crossterm(event) => match event {
                    CrosstermEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                        self.handle_key_events(key_event)?
                    }
                    CrosstermEvent::Resize(_width, height) => {
                        terminal.autoresize()?;
                        self.update_viewport_from_height(height);
                    }
                    _ => {}
                },
                Event::App(app_event) => match app_event {
                    AppEvent::Quit => self.quit(),

                    // Fetch Events
                    AppEvent::FetchSuccess(events_fetched) => self.add_events(events_fetched),
                    AppEvent::FetchFailed(_) => self.mode = Default::default(),
                },
            }
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

    /// Handles the key events and updates the state of [`App`].
    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match self.mode {
            AppMode::Normal => self.handle_normal_key_events(key_event),
            AppMode::Fetching => self.handle_normal_key_events(key_event),
            AppMode::Insert => self.handle_insert_key_events(key_event),
        }
    }

    /// Handle normal key events
    pub fn handle_normal_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            // Other handlers you could add here.
            // Scroll vertically
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('K') => self.big_scroll_up(),
            KeyCode::Char('J') => self.big_scroll_down(),

            // Scroll horizontally
            KeyCode::Char('h') | KeyCode::Left => self.scroll_left(),
            KeyCode::Char('l') | KeyCode::Right => self.scroll_right(),

            // Jump to current time
            KeyCode::Char('t') => self.jump_to_current_time(),

            // Toggle
            KeyCode::Char('T') => self.is_now_timeline_visible = !self.is_now_timeline_visible,

            // Change mode
            KeyCode::Char('i') => self.mode = AppMode::Insert,
            _ => {}
        }
        Ok(())
    }

    /// Handle insert key events
    pub fn handle_insert_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc => self.mode = AppMode::Normal,
            KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            // Other handlers you could add here.
            // Scroll vertically
            KeyCode::Char('k') | KeyCode::Up => self.move_insert_up(),
            KeyCode::Char('j') | KeyCode::Down => self.move_insert_down(),

            // Scroll horizontally
            KeyCode::Char('h') | KeyCode::Left => self.move_insert_left(),
            KeyCode::Char('l') | KeyCode::Right => self.move_insert_right(),

            // Jump to current time
            KeyCode::Char('t') => self.move_insert_current_time(),

            // Toggle
            KeyCode::Char('T') => self.is_now_timeline_visible = !self.is_now_timeline_visible,
            _ => {}
        }
        Ok(())
    }

    /// Trigger background fetch for new events
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

    /// Add events
    fn add_events(&mut self, mut events_fetched: EventsFetched) {
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
    fn move_insert_up(&mut self) {
        self.insert_event.start_time -= TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        self.sync_viewport_to_cursor();
    }
    fn move_insert_down(&mut self) {
        self.insert_event.start_time += TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        self.sync_viewport_to_cursor();
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
    fn move_insert_left(&mut self) {
        self.insert_event.start_time -= TimeDelta::days(1);
        self.sync_viewport_to_cursor();
    }
    fn move_insert_right(&mut self) {
        self.insert_event.start_time += TimeDelta::days(1);
        self.sync_viewport_to_cursor();
    }

    /// Pagination
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

    /// Centers the viewport to the given time
    fn jump_to_time(&mut self, time: &DateTime<Local>) {
        self.start_date = time.date_naive() - START_OFFSET;

        let current_mins = time.hour() * 60 + time.minute();
        let half_viewport = self.viewport_mins / 2;
        let target_offset = (current_mins as u16).saturating_sub(half_viewport);
        let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);

        self.scroll_offset = target_offset.min(max_offset);
    }

    /// Jumps the calendar to Today
    fn jump_to_current_time(&mut self) {
        let now = Local::now();
        self.now = now;
        self.jump_to_time(&now);
    }
    fn move_insert_current_time(&mut self) {
        let now = Local::now();
        self.now = now;
        self.jump_to_time(&now);
        self.insert_event.start_time = now.with_timezone(&Utc);
    }

    /// Resize viewport
    fn update_viewport_from_height(&mut self, term_height: u16) {
        let vertical_overhead = 4;
        let usable_rows = term_height.saturating_sub(vertical_overhead);
        self.viewport_mins = usable_rows * RESOLUTION_IN_MINS;

        // if the window got taller, the max offset shrinks
        let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);
        self.scroll_offset = self.scroll_offset.min(max_offset);
    }

    /// Sync viewport
    fn sync_viewport_to_cursor(&mut self) {
        let cursor_local = self.insert_event.start_time.with_timezone(&Local);
        let cursor_date = cursor_local.date_naive();

        // shift start date if cursor moved out of current view
        if cursor_date < self.start_date {
            self.start_date = cursor_date;
            self.check_pagination();
        } else if cursor_date >= self.start_date + self.num_days {
            self.start_date = cursor_date - self.num_days + TimeDelta::days(1);
            self.check_pagination();
        }

        // Sync the vertical scroll
        let cursor_mins = (cursor_local.hour() * 60 + cursor_local.minute()) as u16;

        let viewport_top = self.scroll_offset;
        let viewport_bottom = self.scroll_offset + self.viewport_mins;
        let insert_event_duration_mins = self.insert_event.duration.num_minutes() as u16;

        // auto scroll up if cursor goes beyond the screen
        if cursor_mins < viewport_top {
            self.scroll_offset = cursor_mins;
        }
        // auto scroll down if the cursor goes beyond the screen
        // taking into account the height of the block so it doesn't get clipped
        else if cursor_mins + insert_event_duration_mins > viewport_bottom {
            let overflow = cursor_mins + insert_event_duration_mins - viewport_bottom;
            let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);
            self.scroll_offset = (self.scroll_offset + overflow).min(max_offset);
        }
    }
}
