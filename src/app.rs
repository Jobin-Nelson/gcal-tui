use crate::constants::{
    BUFFER_DAYS, FETCH_DAYS, MINUTES_IN_DAY, NUM_DAYS, RESOLUTION_IN_MINS, ROWS_PER_HOUR,
    SCROLL_OFFSET_MINS, START_OFFSET, TIME_FORMAT, VIEWPORT_MINS,
};
use crate::event::{AppEvent, Event, EventHandler, EventsFetched};
use crate::{Calendar, Config, Result};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Timelike, Utc};
use google_calendar3::api::Event as CEvent;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};
use ratatui_textarea::TextArea;

#[derive(Debug, Clone)]
pub struct EventNode {
    pub id: String,
    pub summary: String,
    pub description: Option<String>,
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
            description: cal_event.description,
            start_time: event_start_datetime,
            end_time: event_end_datetime,
        })
    }
}

#[derive(Default, PartialEq)]
pub enum AppMode {
    #[default]
    Normal,
    Insert,
    InsertEdit,
    Visual,
    VisualEdit,
    DeleteConfirm,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum ActiveField {
    #[default]
    Summary,
    Description,
    StartTime,
    EndTime,
}

#[derive(Debug, Default)]
pub struct EventPopup<'a> {
    pub summary: TextArea<'a>,
    pub description: TextArea<'a>,
    pub start_time: TextArea<'a>,
    pub end_time: TextArea<'a>,
    pub active_field: ActiveField,
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
    pub popup: EventPopup<'static>,

    // Track background network status independently
    pub is_fetching: bool,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let config = Config::new()?;

        let cal = Calendar::new(config).await?;

        let now = Local::now();
        let start_date = now.date_naive() - START_OFFSET;

        let rounded_minute = (now.minute() / RESOLUTION_IN_MINS as u32) * RESOLUTION_IN_MINS as u32;
        let rounded_time = now
            .with_minute(rounded_minute)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap()
            .with_timezone(&Utc);

        let insert_event = InsertEvent {
            start_time: rounded_time,
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
            popup: Default::default(),
            is_fetching: false,
        };

        app.fetch_events(
            start_date - BUFFER_DAYS,
            start_date + app.num_days + BUFFER_DAYS,
            true,
        );
        app.switch_active_field(Default::default());

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
                    AppEvent::FetchSuccess(events_fetched) => {
                        self.handle_fetched_events(events_fetched)
                    }
                    AppEvent::FetchFailed(_) => self.is_fetching = false,
                    AppEvent::EventCreated(event_node) => self.handle_event_created(event_node),
                    AppEvent::ReloadSuccess(events_fetched) => {
                        self.handle_reload_events(events_fetched)
                    }
                    AppEvent::EventUpdated(event_node) => self.handle_event_created(event_node),
                    AppEvent::EventDeleted(event_id) => self.handle_event_deleted(event_id),
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
            AppMode::Insert => self.handle_insert_key_events(key_event),
            AppMode::InsertEdit => self.handle_insert_edit_key_events(key_event),
            AppMode::Visual => self.handle_visual_key_events(key_event),
            AppMode::VisualEdit => self.handle_visual_edit_key_events(key_event),
            AppMode::DeleteConfirm => self.handle_delete_confirm_key_events(key_event),
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
            KeyCode::Char('v') => {
                self.mode = AppMode::Visual;
                if self.sel_event_id.is_none() {
                    self.select_first_visible_current_day_event();
                }
            }
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

            // Extend
            KeyCode::Char('K') => self.extend_insert_up(),
            KeyCode::Char('J') => self.extend_insert_down(),
            KeyCode::Char('H') => self.extend_insert_left(),
            KeyCode::Char('L') => self.extend_insert_right(),

            // Insert event details
            KeyCode::Enter => self.enter_insert_event_details(),

            // Jump to current time
            KeyCode::Char('t') => self.move_insert_current_time(),

            // Toggle
            KeyCode::Char('T') => self.is_now_timeline_visible = !self.is_now_timeline_visible,
            _ => {}
        }
        Ok(())
    }

    fn handle_popup_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Tab => {
                let active_field = match self.popup.active_field {
                    ActiveField::Summary => ActiveField::Description,
                    ActiveField::Description => ActiveField::StartTime,
                    ActiveField::StartTime => ActiveField::EndTime,
                    ActiveField::EndTime => ActiveField::Summary,
                };
                self.switch_active_field(active_field);
            }
            KeyCode::BackTab => {
                let active_field = match self.popup.active_field {
                    ActiveField::Summary => ActiveField::EndTime,
                    ActiveField::Description => ActiveField::Summary,
                    ActiveField::StartTime => ActiveField::Description,
                    ActiveField::EndTime => ActiveField::StartTime,
                };
                self.switch_active_field(active_field);
            }
            _ => {}
        }
    }

    /// Handle insertedit key events
    pub fn handle_insert_edit_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc => self.mode = AppMode::Insert,
            KeyCode::Tab | KeyCode::BackTab => self.handle_popup_events(key_event),
            KeyCode::Char('s' | 'S') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.submit_popup()?
            }
            _ => {
                let active_ta = match self.popup.active_field {
                    ActiveField::Summary => &mut self.popup.summary,
                    ActiveField::Description => &mut self.popup.description,
                    ActiveField::StartTime => &mut self.popup.start_time,
                    ActiveField::EndTime => &mut self.popup.end_time,
                };
                active_ta.input(key_event);
            }
        }
        Ok(())
    }

    /// Handle visual key events
    pub fn handle_visual_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('v') => {
                self.mode = Default::default();
                self.sel_event_id = None;
            }
            KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            // Other handlers you could add here.
            // move through events chronologically
            KeyCode::Char('k') | KeyCode::Up => self.select_prev_event(),
            KeyCode::Char('j') | KeyCode::Down => self.select_next_event(),

            // jump days
            KeyCode::Char('h') | KeyCode::Left => self.select_prev_day_event(),
            KeyCode::Char('l') | KeyCode::Right => self.select_next_day_event(),

            // Open the selected event details
            KeyCode::Enter => self.enter_visual_event_details(),

            // Open delete popup
            KeyCode::Char('d') => self.mode = AppMode::DeleteConfirm,

            // Toggle
            KeyCode::Char('T') => self.is_now_timeline_visible = !self.is_now_timeline_visible,
            _ => {}
        }
        Ok(())
    }

    /// Handle visualedit key events
    pub fn handle_visual_edit_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc => self.mode = AppMode::Visual,
            KeyCode::Tab | KeyCode::BackTab => self.handle_popup_events(key_event),
            KeyCode::Char('s' | 'S') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.submit_popup()?
            }
            _ => {
                let active_ta = match self.popup.active_field {
                    ActiveField::Summary => &mut self.popup.summary,
                    ActiveField::Description => &mut self.popup.description,
                    ActiveField::StartTime => &mut self.popup.start_time,
                    ActiveField::EndTime => &mut self.popup.end_time,
                };
                active_ta.input(key_event);
            }
        }
        Ok(())
    }

    /// Handle visualedit key events
    pub fn handle_delete_confirm_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc => self.mode = AppMode::Visual,
            KeyCode::Enter => {
                // It should be only possible to enter delete confirm mode from visual mode
                let event_node = self.get_selected_event().unwrap();

                self.delete_event(event_node.clone());
                self.mode = AppMode::Visual;
            }
            _ => {}
        }
        Ok(())
    }

    /// Trigger background fetch for new events
    pub fn fetch_events(&mut self, start_date: NaiveDate, end_date: NaiveDate, is_refresh: bool) {
        if self.is_fetching {
            return;
        }
        self.is_fetching = true;

        let cal_clone = self.cal.clone();
        let sender = self.events.sender.clone();

        tokio::spawn(async move {
            match cal_clone.get_events(start_date, end_date).await {
                Ok(events) => {
                    let event_nodes = events
                        .into_iter()
                        .filter_map(|e| e.try_into().ok())
                        .collect();
                    let events_fetched = EventsFetched {
                        event_nodes,
                        start_date,
                        end_date,
                    };
                    let app_event = if is_refresh {
                        AppEvent::ReloadSuccess(events_fetched)
                    } else {
                        AppEvent::FetchSuccess(events_fetched)
                    };
                    let _ = sender.send(Event::App(app_event));
                }
                Err(e) => {
                    let _ = sender.send(Event::App(AppEvent::FetchFailed(e.to_string())));
                }
            }
        });
    }

    /// Trigger background request for creating event
    pub fn create_event(&mut self, event_node: EventNode) {
        if self.is_fetching {
            return;
        }
        self.is_fetching = true;

        let cal_clone = self.cal.clone();
        let sender = self.events.sender.clone();

        tokio::spawn(async move {
            match cal_clone.create_event(event_node).await {
                Ok(event) => {
                    let Ok(event_node) = EventNode::try_from(event) else {
                        let _ = sender.send(Event::App(AppEvent::FetchFailed(
                            "Failed to convert to eventnode".to_string(),
                        )));
                        return;
                    };
                    let _ = sender.send(Event::App(AppEvent::EventCreated(event_node)));
                }
                Err(e) => {
                    let _ = sender.send(Event::App(AppEvent::FetchFailed(e.to_string())));
                }
            }
        });
    }

    /// Trigger background request for patching event
    pub fn patch_event(&mut self, event_node: EventNode) {
        if self.is_fetching {
            return;
        }
        self.is_fetching = true;

        let cal_clone = self.cal.clone();
        let sender = self.events.sender.clone();

        tokio::spawn(async move {
            match cal_clone.patch_event(event_node).await {
                Ok(event) => {
                    let Ok(event_node) = EventNode::try_from(event) else {
                        let _ = sender.send(Event::App(AppEvent::FetchFailed(
                            "Failed to convert to eventnode".to_string(),
                        )));
                        return;
                    };
                    let _ = sender.send(Event::App(AppEvent::EventUpdated(event_node)));
                }
                Err(e) => {
                    let _ = sender.send(Event::App(AppEvent::FetchFailed(e.to_string())));
                }
            }
        });
    }

    /// Trigger background request for deleting event
    pub fn delete_event(&mut self, event_node: EventNode) {
        if self.is_fetching {
            return;
        }
        self.is_fetching = true;

        let cal_clone = self.cal.clone();
        let sender = self.events.sender.clone();

        tokio::spawn(async move {
            let event_id = event_node.id.clone();
            match cal_clone.delete_event(event_node).await {
                Ok(_) => {
                    let _ = sender.send(Event::App(AppEvent::EventDeleted(event_id)));
                }
                Err(e) => {
                    let _ = sender.send(Event::App(AppEvent::FetchFailed(e.to_string())));
                }
            }
        });
    }

    /// Add events
    fn handle_fetched_events(&mut self, mut events_fetched: EventsFetched) {
        self.cal_event_nodes.append(&mut events_fetched.event_nodes);
        self.cal_event_nodes.sort_by_key(|e| e.start_time);

        self.loaded_start = self.loaded_start.min(events_fetched.start_date);
        self.loaded_end = self.loaded_end.max(events_fetched.end_date);

        self.is_fetching = false;
    }
    fn handle_event_created(&mut self, event_node: EventNode) {
        let start_date = event_node.start_time.with_timezone(&Local).date_naive();
        let end_date = event_node.end_time.with_timezone(&Local).date_naive();
        if self.loaded_start <= start_date && self.loaded_end >= end_date {
            if self.mode == AppMode::InsertEdit {
                self.cal_event_nodes.push(event_node);
                self.mode = AppMode::Insert;
            } else if self.mode == AppMode::VisualEdit
                && let Some(pos) = self
                    .cal_event_nodes
                    .iter()
                    .position(|e| e.id == event_node.id)
            {
                self.cal_event_nodes[pos] = event_node;
                self.mode = AppMode::Visual;
            };
            self.cal_event_nodes.sort_by_key(|e| e.start_time);
        }
        self.is_fetching = false;
    }

    /// Update events
    fn handle_reload_events(&mut self, events_fetched: EventsFetched) {
        self.cal_event_nodes = events_fetched.event_nodes;
        self.cal_event_nodes.sort_by_key(|e| e.start_time);

        self.loaded_start = events_fetched.start_date;
        self.loaded_end = events_fetched.end_date;

        self.is_fetching = false;
    }

    /// delete event
    fn handle_event_deleted(&mut self, event_id: String) {
        self.cal_event_nodes.remove(
            self.cal_event_nodes
                .iter()
                .position(|e| e.id == *event_id)
                .unwrap(),
        );
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

    /// Move Insert Event
    fn move_insert_up(&mut self) {
        self.insert_event.start_time -= TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        self.sync_viewport_to_cursor();
    }
    fn move_insert_down(&mut self) {
        self.insert_event.start_time += TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        self.sync_viewport_to_cursor();
    }
    fn move_insert_left(&mut self) {
        self.insert_event.start_time -= TimeDelta::days(1);
        self.sync_viewport_to_cursor();
    }
    fn move_insert_right(&mut self) {
        self.insert_event.start_time += TimeDelta::days(1);
        self.sync_viewport_to_cursor();
    }

    /// Extend Insert Event
    fn extend_insert_up(&mut self) {
        let delta = TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        if self.insert_event.duration > delta {
            self.insert_event.duration -= delta;
        }
        self.sync_viewport_to_cursor();
    }
    fn extend_insert_down(&mut self) {
        self.insert_event.duration += TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        self.sync_viewport_to_cursor();
    }
    fn extend_insert_left(&mut self) {
        let delta = TimeDelta::days(1);
        let min_duration = TimeDelta::minutes(RESOLUTION_IN_MINS as i64);
        if self.insert_event.duration - delta >= min_duration {
            self.insert_event.duration -= delta;
        } else {
            self.insert_event.duration = min_duration;
        }
        self.sync_viewport_to_cursor();
    }
    fn extend_insert_right(&mut self) {
        let delta = TimeDelta::days(1);
        self.insert_event.duration += delta;
    }

    /// Select Visual Events
    fn get_selected_index(&self) -> Option<usize> {
        if let Some(id) = &self.sel_event_id {
            self.cal_event_nodes.iter().position(|e| e.id == *id)
        } else {
            None
        }
    }
    pub fn get_selected_event(&self) -> Option<&EventNode> {
        if let Some(id) = &self.sel_event_id {
            self.cal_event_nodes.iter().find(|e| e.id == *id)
        } else {
            None
        }
    }
    fn select_next_event(&mut self) {
        if self.cal_event_nodes.is_empty() {
            return;
        }

        let next_idx = match self.get_selected_index() {
            Some(idx) => (idx + 1).min(self.cal_event_nodes.len() - 1),
            None => 0,
        };

        self.sel_event_id = self.cal_event_nodes.get(next_idx).map(|e| e.id.clone());
        self.sync_viewport_to_selected_event();
    }
    fn select_prev_event(&mut self) {
        if self.cal_event_nodes.is_empty() {
            return;
        }
        let prev_idx = match self.get_selected_index() {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        };

        self.sel_event_id = self.cal_event_nodes.get(prev_idx).map(|e| e.id.clone());
        self.sync_viewport_to_selected_event();
    }
    fn select_next_day_event(&mut self) {
        if self.cal_event_nodes.is_empty() {
            return;
        }

        let current_idx = self.get_selected_index().unwrap_or(0);
        let current_date = self.cal_event_nodes[current_idx]
            .start_time
            .with_timezone(&Local)
            .date_naive();
        let target_idx = self
            .cal_event_nodes
            .iter()
            .position(|e| e.start_time.with_timezone(&Local).date_naive() > current_date)
            .unwrap_or(self.cal_event_nodes.len() - 1);

        self.sel_event_id = self.cal_event_nodes.get(target_idx).map(|e| e.id.clone());
        self.sync_viewport_to_selected_event();
    }
    fn select_prev_day_event(&mut self) {
        if self.cal_event_nodes.is_empty() {
            return;
        }

        let current_idx = self.get_selected_index().unwrap_or(0);
        let current_date = self.cal_event_nodes[current_idx]
            .start_time
            .with_timezone(&Local)
            .date_naive();
        let target_idx = self
            .cal_event_nodes
            .iter()
            .rposition(|e| e.start_time.with_timezone(&Local).date_naive() < current_date)
            .unwrap_or(0);

        self.sel_event_id = self.cal_event_nodes.get(target_idx).map(|e| e.id.clone());
        self.sync_viewport_to_selected_event();
    }

    /// Submit Insert Event
    fn submit_popup(&mut self) -> Result<()> {
        let start_text = self.popup.start_time.lines().join("");
        let end_text = self.popup.end_time.lines().join("");

        let start_res = NaiveDateTime::parse_from_str(start_text.trim(), TIME_FORMAT);
        let end_res = NaiveDateTime::parse_from_str(end_text.trim(), TIME_FORMAT);

        let (Ok(start_naive), Ok(end_naive)) = (start_res, end_res) else {
            // TODO: Signal a format error
            return Ok(());
        };

        let start_time = Local
            .from_local_datetime(&start_naive)
            .unwrap()
            .with_timezone(&Utc);
        let end_time = Local
            .from_local_datetime(&end_naive)
            .unwrap()
            .with_timezone(&Utc);

        let summary = self.popup.summary.lines().join("\n");
        if summary.trim().is_empty() {
            // TODO: Signal summary cannot be empty
            return Ok(());
        }

        let description = {
            let description = self.popup.description.lines().join("\n");
            if description.trim().is_empty() {
                None
            } else {
                Some(description)
            }
        };

        let id = match self.mode {
            AppMode::InsertEdit => Default::default(),
            AppMode::VisualEdit => self.cal_event_nodes[self.get_selected_index().unwrap_or(0)]
                .id
                .clone(),
            _ => return Ok(()),
        };
        let event_node = EventNode {
            id,
            summary,
            description,
            start_time,
            end_time,
        };

        match self.mode {
            AppMode::InsertEdit => {
                self.create_event(event_node);
            }
            AppMode::VisualEdit => {
                self.patch_event(event_node);
            }
            _ => {}
        }
        Ok(())
    }

    /// Pagination
    fn check_pagination(&mut self) {
        if self.is_fetching {
            return;
        }

        if self.start_date + self.num_days + BUFFER_DAYS >= self.loaded_end {
            self.fetch_events(self.loaded_end, self.loaded_end + FETCH_DAYS, false);
        } else if self.start_date - BUFFER_DAYS <= self.loaded_start {
            self.fetch_events(self.loaded_start - FETCH_DAYS, self.loaded_start, false);
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

    /// Insert event details
    fn enter_insert_event_details(&mut self) {
        self.mode = AppMode::InsertEdit;

        let local_start = self.insert_event.start_time.with_timezone(&Local);
        let local_end =
            (self.insert_event.start_time + self.insert_event.duration).with_timezone(&Local);

        let start_str = local_start.format(TIME_FORMAT).to_string();
        let end_str = local_end.format(TIME_FORMAT).to_string();

        // modify the textarea instead of creating new ones to preserve styles
        self.popup.summary.clear();
        self.popup.description.clear();

        self.popup.start_time.select_all();
        self.popup.end_time.select_all();
        self.popup.start_time.insert_str(start_str);
        self.popup.end_time.insert_str(end_str);

        self.switch_active_field(Default::default());
    }

    /// Visual event details
    fn enter_visual_event_details(&mut self) {
        self.mode = AppMode::VisualEdit;

        let Some(id) = self.get_selected_index() else {
            return;
        };

        let event_node = &self.cal_event_nodes[id];

        let local_start = event_node.start_time.with_timezone(&Local);
        let local_end = event_node.end_time.with_timezone(&Local);

        let start_str = local_start.format(TIME_FORMAT).to_string();
        let end_str = local_end.format(TIME_FORMAT).to_string();

        // modify the textarea instead of creating new ones to preserve styles
        self.popup.summary.clear();
        self.popup.description.clear();
        self.popup.start_time.clear();
        self.popup.end_time.clear();

        self.popup.summary.insert_str(&event_node.summary);
        if let Some(description) = &event_node.description {
            self.popup.description.insert_str(description);
        }
        self.popup.start_time.insert_str(start_str);
        self.popup.end_time.insert_str(end_str);

        self.switch_active_field(Default::default());
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
        let cursor_start_local = self.insert_event.start_time.with_timezone(&Local);
        let cursor_end_local =
            (self.insert_event.start_time + self.insert_event.duration).with_timezone(&Local);

        let start_date = cursor_start_local.date_naive();
        let end_date = cursor_end_local.date_naive();

        // 1. Horizontal sync
        // shift left if the start date moves out of view
        if start_date < self.start_date {
            self.start_date = start_date;
            self.check_pagination();
        // shift right if the end date pushes past the right edge of view
        } else if end_date >= self.start_date + self.num_days {
            self.start_date = end_date - self.num_days + TimeDelta::days(1);
            self.check_pagination();
        }

        // 2. Vertical sync
        let cursor_start_mins =
            (cursor_start_local.hour() * 60 + cursor_start_local.minute()) as u16;
        let cursor_end_mins = cursor_start_mins + self.insert_event.duration.num_minutes() as u16;
        let viewport_top = self.scroll_offset;
        let viewport_bottom = self.scroll_offset + self.viewport_mins;

        // scroll up if the top edge goes above the screen
        if cursor_start_mins < viewport_top {
            self.scroll_offset = cursor_start_mins;
        // scroll down if the bottom edge goves below the screen
        } else if cursor_end_mins >= viewport_bottom {
            let overflow = cursor_end_mins - viewport_bottom;
            let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);
            self.scroll_offset = (self.scroll_offset + overflow).min(max_offset);
        }
    }

    fn sync_viewport_to_selected_event(&mut self) {
        let Some(idx) = self.get_selected_index() else {
            return;
        };

        let event_node = &self.cal_event_nodes[idx];
        let start_local = event_node.start_time.with_timezone(&Local);
        let end_local = event_node.end_time.with_timezone(&Local);

        let event_start_date = start_local.date_naive();
        let event_end_date = end_local.date_naive();

        // 1. Horizontal sync
        if event_start_date < self.start_date {
            self.start_date = event_start_date;
            self.check_pagination();
        } else if event_end_date >= self.start_date + self.num_days {
            self.start_date = event_start_date - self.num_days + TimeDelta::days(1);
            self.check_pagination();
        }

        // 3. Vertical sync
        let start_mins = (start_local.hour() * 60 + start_local.minute()) as u16;
        let end_mins = (end_local.hour() * 60 + end_local.minute()) as u16;

        let viewport_top = self.scroll_offset;
        let viewport_bottom = self.scroll_offset + self.viewport_mins;

        if start_mins < viewport_top {
            self.scroll_offset = start_mins;
        } else if end_mins > viewport_bottom {
            let overflow = end_mins.saturating_sub(viewport_bottom);
            let max_offset = MINUTES_IN_DAY.saturating_sub(self.viewport_mins);
            self.scroll_offset = (self.scroll_offset + overflow).min(max_offset);
        }
    }

    fn select_first_visible_current_day_event(&mut self) {
        if self.cal_event_nodes.is_empty() {
            self.sel_event_id = None;
            return;
        };
        let current_day = (self.start_date + START_OFFSET)
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let current_day_local = Local.from_local_datetime(&current_day).unwrap();

        // convert the current viewport top into UTC timestamp
        let viewport_start_utc =
            (current_day_local + TimeDelta::minutes(self.scroll_offset as i64)).with_timezone(&Utc);

        // Find the first event whose end time extends past the top of viewport
        let first_visible_idx = self
            .cal_event_nodes
            .iter()
            .position(|e| e.end_time > viewport_start_utc)
            .unwrap_or(0);

        self.sel_event_id = self
            .cal_event_nodes
            .get(first_visible_idx)
            .map(|e| e.id.clone());
    }

    fn switch_active_field(&mut self, field: ActiveField) {
        self.popup.active_field = field;

        configure_insert_ta(
            &mut self.popup.summary,
            " Summary ",
            self.popup.active_field == ActiveField::Summary,
        );
        configure_insert_ta(
            &mut self.popup.description,
            " Description ",
            self.popup.active_field == ActiveField::Description,
        );
        configure_insert_ta(
            &mut self.popup.start_time,
            " Start Time ",
            self.popup.active_field == ActiveField::StartTime,
        );
        configure_insert_ta(
            &mut self.popup.end_time,
            " End Time ",
            self.popup.active_field == ActiveField::EndTime,
        );
    }
}

fn configure_insert_ta<'a>(ta: &mut TextArea<'a>, title: &'a str, is_active: bool) {
    let mut block = Block::bordered().title(title);

    if is_active {
        block = block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        ta.set_cursor_line_style(Style::default());
        ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    } else {
        block = block.border_style(Style::default().fg(Color::DarkGray));
        ta.set_cursor_style(Style::default());
    };

    ta.set_block(block);
}
