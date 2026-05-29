use chrono::{DateTime, Datelike, Local, NaiveTime, Timelike, Utc, Weekday};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Margin, Rect, Spacing},
    symbols::merge::MergeStrategy,
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};
use tracing::Level;

use crate::{
    app::App,
    constants::{MTW, RESOLUTION_IN_MINS, ROWS_PER_HOUR},
    trace_dbg,
};

impl Widget for &App {
    /// Renders the user interface widgets.
    ///
    // This is where you add new widgets.
    // See the following resources:
    // - https://docs.rs/ratatui/latest/ratatui/widgets/index.html
    // - https://github.com/ratatui/ratatui/tree/master/examples
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header_layout = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
            .spacing(Spacing::Overlap(1));
        let horizontal_layout = Layout::horizontal([
            Constraint::Length(9),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .spacing(Spacing::Overlap(1));
        let [header, calendar] = area.layout(&header_layout);
        let header_columns: [Rect; 4] = header.layout(&horizontal_layout);
        let columns: [Rect; 4] = calendar.layout(&horizontal_layout);

        let block = Block::bordered().merge_borders(MergeStrategy::Exact);
        let event_block = Block::bordered().border_type(BorderType::Rounded);

        // headers
        let current_day = Local::now().weekday();
        let day_headers = [current_day.pred(), current_day, current_day.succ()];
        std::iter::once("Time")
            .chain(day_headers.map(|d| MTW[d.num_days_from_monday() as usize]))
            .zip(header_columns.iter())
            .for_each(|(header, &header_column)| {
                Paragraph::new(header)
                    .block(block.clone())
                    .centered()
                    .render(header_column, buf);
            });

        // Time
        let viewport_start_time = self.scroll_offset;
        let viewport_end_time = self.scroll_offset + self.viewport_hours;
        let mut lines = vec![];

        let num_fillers_each_half_hour = ROWS_PER_HOUR / 2;

        for hour in viewport_start_time..=viewport_end_time {
            lines.push(Line::from(format!("{:02}:00", hour)));
            if hour == viewport_end_time {
                continue;
            }
            lines.extend(std::iter::repeat_n(
                Line::from(""),
                num_fillers_each_half_hour as usize,
            ));
            lines.push(Line::from(format!("{:02}:30", hour)));
            lines.extend(std::iter::repeat_n(
                Line::from(""),
                num_fillers_each_half_hour as usize,
            ));
        }
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(block.clone())
            .render(columns[0], buf);

        // Calendar
        let is_this_day = |d: &DateTime<Utc>, day: Weekday| d.weekday() == day;
        let is_within_viewport = |d: &DateTime<Utc>| {
            let start_time = NaiveTime::from_hms_opt(viewport_start_time as u32, 0, 0).unwrap();
            let end_time = NaiveTime::from_hms_opt(viewport_end_time as u32, 0, 0).unwrap();
            let current_time = d.time();
            current_time >= start_time && current_time <= end_time
        };

        for (&day_area, day) in columns.iter().skip(1).zip(day_headers) {
            block.clone().render(day_area, buf);

            let cal_events_per_day_in_view = self.cal_events.iter().filter(|e| {
                let start_end_datetime = e.start.as_ref().zip(e.end.as_ref());
                start_end_datetime.is_some_and(|(s, e)| {
                    s.date_time
                        .as_ref()
                        .is_some_and(|d| is_this_day(d, day) && is_within_viewport(d))
                        && e.date_time
                            .as_ref()
                            .is_some_and(|d| is_this_day(d, day) && is_within_viewport(d))
                })
            });

            for cal_event in cal_events_per_day_in_view {
                let start_time = cal_event.start.as_ref().and_then(|d| d.date_time);
                let end_time = cal_event.end.as_ref().and_then(|d| d.date_time);

                let (Some(start_time), Some(end_time)) = (start_time, end_time) else {
                    continue;
                };

                let start_hour_scroll_adj =
                    start_time.hour().saturating_sub(self.scroll_offset as u32) as u16;
                let start_min_scroll_adj =
                    start_hour_scroll_adj + (start_time.minute() as u16 / RESOLUTION_IN_MINS);

                let inner_area = day_area.inner(Margin {
                    vertical: 1,
                    horizontal: 1,
                });

                let delta = end_time - start_time;
                let duration_hours = delta.num_hours() * ROWS_PER_HOUR as i64;
                let duration_mins = (delta.num_minutes() % 60) / RESOLUTION_IN_MINS as i64;
                let duration = (duration_hours + duration_mins) as u16;

                let event_area = Rect {
                    x: inner_area.x,
                    y: inner_area.y + start_min_scroll_adj,
                    width: inner_area.width,
                    height: duration,
                };
                Paragraph::new(Line::from(
                    cal_event
                        .summary
                        .as_ref()
                        .unwrap_or(&"Untitled".to_string())
                        .as_str(),
                ))
                .block(event_block.clone())
                .render(event_area, buf);
            }
        }

        // let window_top = -self.scroll_offset;
        // let window_bottom = -(self.scroll_offset + self.viewport_hours);
        // let time_canvas = Canvas::default()
        //     .block(right_border)
        //     .x_bounds([0.0, 100.0])
        //     .y_bounds([window_bottom as f64, window_top as f64])
        //     .marker(ratatui::symbols::Marker::Dot)
        //     .paint(|ctx| ctx.draw(timeline));

        // time_canvas.render(time, buf);
    }
}
