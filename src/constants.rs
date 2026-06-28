use chrono::TimeDelta;

pub const MINUTES_IN_DAY: u16 = 24 * 60;
pub const SCROLL_OFFSET_MINS: u16 = 7 * 60;
pub const VIEWPORT_MINS: u16 = 6 * 60;
pub const ROWS_PER_HOUR: u16 = 4;
pub const RESOLUTION_IN_MINS: u16 = 60 / ROWS_PER_HOUR;
/// Days to display in TUI
pub const NUM_DAYS: TimeDelta = TimeDelta::days(3);
/// Determines when to the fetch new events
pub const BUFFER_DAYS: TimeDelta = TimeDelta::days(3);
pub const FETCH_DAYS: TimeDelta = TimeDelta::days(7);
pub const START_OFFSET: TimeDelta = TimeDelta::days(1);
pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M";
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar";
