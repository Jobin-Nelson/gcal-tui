use std::cmp::Reverse;

use chrono::{DateTime, Local, NaiveDate, TimeDelta, TimeZone, Timelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Margin, Rect, Spacing},
    style::{Color, Modifier, Style},
    symbols::merge::MergeStrategy,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::{
    app::{App, AppMode, EventNode, InsertEvent},
    constants::RESOLUTION_IN_MINS,
};

#[derive(Debug)]
struct RenderedEvent<'a> {
    pub event: &'a EventNode,
    pub rect: Rect,
}

struct VisibleEvents<'a> {
    pub event: &'a EventNode,
    pub start_row: u16,
    pub end_row: u16,
}

fn calculate_viewport_rect<'a, 'b>(
    events: &'a [EventNode],
    viewport_start: DateTime<Utc>,
    viewport_end: DateTime<Utc>,
    column_area: &'b Rect,
) -> Vec<RenderedEvent<'a>> {
    // 1. Filter and convert time directly to grid rows
    let mut visible_events: Vec<VisibleEvents<'a>> = events
        .iter()
        .filter_map(|ev| {
            if ev.end_time <= viewport_start || ev.start_time >= viewport_end {
                return None;
            }

            let clamped_start = ev.start_time.max(viewport_start);
            let clamped_end = ev.end_time.min(viewport_end);

            // Calculate total minutes from top of the screen
            let start_mins = (clamped_start - viewport_start).num_minutes();
            let end_mins = (clamped_end - viewport_start).num_minutes();

            // Convert minutes to terminal rows
            let start_row = start_mins as u16 / RESOLUTION_IN_MINS;
            let mut end_row = end_mins as u16 / RESOLUTION_IN_MINS;

            // INFO: Ensure even short events (eg. 5 mins) take up at least 1 row
            if end_row <= start_row {
                end_row = start_row + 1;
            }

            Some(VisibleEvents {
                event: ev,
                start_row,
                end_row,
            })
        })
        .collect();

    if visible_events.is_empty() {
        return vec![];
    }

    // 2. Sort and Cluster (Using Row Indices)
    visible_events.sort_by_key(|e| (e.start_row, Reverse(e.end_row)));

    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut current_cluster: Vec<usize> = Vec::new();
    let mut cluster_end_row = 0;

    for (i, ev) in visible_events.iter().enumerate() {
        if ev.start_row >= cluster_end_row && !current_cluster.is_empty() {
            clusters.push(std::mem::take(&mut current_cluster));
            cluster_end_row = ev.end_row;
        } else {
            cluster_end_row = cluster_end_row.max(ev.end_row);
        }
        current_cluster.push(i);
    }

    if !current_cluster.is_empty() {
        clusters.push(current_cluster);
    }

    // 3. Generate Rects and find the Column index
    let mut results = Vec::with_capacity(visible_events.len());

    for cluster in clusters {
        let mut column_ends: Vec<u16> = Vec::new();
        let mut placements = Vec::new();

        for &idx in &cluster {
            let ev = &visible_events[idx];
            let mut placed_col = None;

            for (c_idx, c_end) in column_ends.iter_mut().enumerate() {
                if *c_end <= ev.start_row {
                    *c_end = ev.end_row;
                    placed_col = Some(c_idx);
                    break;
                }
            }

            let col_idx = placed_col.unwrap_or_else(|| {
                column_ends.push(ev.end_row);
                column_ends.len() - 1
            });
            placements.push((idx, col_idx));
        }

        let total_cols = column_ends.len() as u16;
        let col_width = (column_area.width / total_cols).max(1);

        for (idx, col_idx) in placements {
            let ev = &visible_events[idx];

            let rect = Rect {
                x: column_area.x + (col_idx as u16 * col_width),
                y: column_area.y + ev.start_row,
                width: col_width,
                height: ev.end_row - ev.start_row,
            };

            results.push(RenderedEvent {
                event: ev.event,
                rect,
            });
        }
    }

    results
}

fn insert_event_to_rect(
    insert_event: &InsertEvent,
    viewport_start: DateTime<Utc>,
    viewport_end: DateTime<Utc>,
    column_area: &Rect,
) -> Rect {
    let end_time = insert_event.start_time + insert_event.duration;

    let clamped_start = insert_event.start_time.max(viewport_start);
    let clamped_end = end_time.min(viewport_end);

    // Calculate total minutes from top of the screen
    let start_mins = (clamped_start - viewport_start).num_minutes();
    let end_mins = (clamped_end - viewport_start).num_minutes();

    // Convert minutes to terminal rows
    let start_row = start_mins as u16 / RESOLUTION_IN_MINS;
    let end_row = end_mins as u16 / RESOLUTION_IN_MINS;

    Rect {
        x: column_area.x,
        y: column_area.y + start_row,
        width: column_area.width,
        height: end_row.saturating_sub(start_row),
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header_layout = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
            .spacing(Spacing::Overlap(1));
        let horizontal_layout = Layout::horizontal([
            Constraint::Length(9),
            Constraint::Fill(1), // Yesterday
            Constraint::Fill(1), // Today
            Constraint::Fill(1), // Tommorrow
        ])
        .spacing(Spacing::Overlap(1));
        let [header, calendar] = area.layout(&header_layout);
        let header_columns: [Rect; 4] = header.layout(&horizontal_layout);
        let columns: [Rect; 4] = calendar.layout(&horizontal_layout);

        // Blocks
        let block = Block::bordered().merge_borders(MergeStrategy::Exact);

        // headers
        let target_dates: Vec<NaiveDate> = (0..self.num_days.num_days())
            .map(|i| self.start_date + TimeDelta::days(i))
            .collect();

        std::iter::once("Time".to_string())
            .chain(target_dates.iter().map(|d| d.format("%A").to_string()))
            .zip(header_columns.iter())
            .for_each(|(header, &header_column)| {
                Paragraph::new(header)
                    .block(block.clone())
                    .centered()
                    .render(header_column, buf);
            });

        // Time Gutter
        let start_mins = self.scroll_offset;
        let end_mins = self.scroll_offset + self.viewport_mins;

        let mut time_lines = Vec::new();
        for row_time in (start_mins..end_mins).step_by(RESOLUTION_IN_MINS as usize) {
            let hour = row_time / 60;
            let row_within_hour = (row_time % 60) / RESOLUTION_IN_MINS;
            match row_within_hour {
                0 => {
                    // Top of the hour (eg. 09:00)
                    time_lines.push(
                        Line::from(format!("{:02}:00", hour))
                            .style(Style::default().fg(Color::Gray)),
                    );
                }
                2 => {
                    // Half-hour mark (30 mins)
                    time_lines.push(
                        Line::from(format!("{:02}:30", hour))
                            .style(Style::default().fg(Color::Gray)),
                    );
                }
                _ => {
                    // 15, 45 minutes mark
                    time_lines.push(Line::from(""));
                }
            }
        }

        Paragraph::new(time_lines)
            .alignment(Alignment::Center)
            .block(block.clone())
            .render(columns[0], buf);

        // Draw events
        for (day, day_area) in target_dates.iter().zip(columns.iter().skip(1)) {
            block.clone().render(*day_area, buf);

            let inner_area = day_area.inner(Margin {
                horizontal: 1,
                vertical: 1,
            });
            let midnight = day.and_hms_opt(0, 0, 0).unwrap();
            let start_of_day_local = Local.from_local_datetime(&midnight).unwrap();

            let viewport_start =
                (start_of_day_local + TimeDelta::minutes(start_mins as i64)).with_timezone(&Utc);
            let viewport_end =
                (start_of_day_local + TimeDelta::minutes(end_mins as i64)).with_timezone(&Utc);

            let render_events = calculate_viewport_rect(
                &self.cal_event_nodes,
                viewport_start,
                viewport_end,
                &inner_area,
            );

            for re in render_events {
                let is_selected = self.sel_event_id.as_ref() == Some(&re.event.id);

                let (border_color, bg_color, text_color) = if is_selected {
                    (Color::Yellow, Color::LightBlue, Color::Black)
                } else {
                    (Color::Cyan, Color::DarkGray, Color::White)
                };

                Paragraph::new(re.event.summary.as_str())
                    .block(
                        Block::default().borders(Borders::LEFT).border_style(
                            Style::default()
                                .fg(border_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    )
                    .style(Style::default().bg(bg_color).fg(text_color))
                    .wrap(Wrap { trim: true })
                    .render(re.rect, buf);
            }

            // Draw current time line
            if self.is_now_timeline_visible && *day == self.now.date_naive() {
                let current_mins = (self.now.hour() * 60 + self.now.minute()) as u16;
                let start_mins = self.scroll_offset;
                let end_mins = start_mins + self.viewport_mins;

                if current_mins >= start_mins && current_mins < end_mins {
                    let rel_mins = current_mins - start_mins;
                    let y_offset = rel_mins / RESOLUTION_IN_MINS;

                    let timeline_rect = Rect {
                        x: inner_area.x,
                        y: inner_area.y + y_offset,
                        width: inner_area.width,
                        height: 1,
                    };

                    let line_width = inner_area.width.saturating_sub(1) as usize;
                    let line_str = format!("{}", "━".repeat(line_width));

                    Paragraph::new(line_str)
                        .style(Style::default().fg(Color::Red))
                        .render(timeline_rect, buf);
                }
            }

            // Draw insert time block
            if self.mode == AppMode::Insert {
                let insert_local_date = self
                    .insert_event
                    .start_time
                    .with_timezone(&Local)
                    .date_naive();
                if day == &insert_local_date {
                    let insert_rect = insert_event_to_rect(
                        &self.insert_event,
                        viewport_start,
                        viewport_end,
                        &inner_area,
                    );
                    Block::bordered()
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .border_style(Style::default().add_modifier(Modifier::BOLD))
                        .render(insert_rect, buf);
                }
            }
        }

        // Draw Popup
        if self.mode == AppMode::InsertTyping {
            let popup_area = area.centered(Constraint::Max(35), Constraint::Length((3 * 3) + 2));

            Clear.render(popup_area, buf);

            Block::bordered()
                .title(" Create Event ")
                .border_style(Style::default().fg(Color::Cyan))
                .render(popup_area, buf);

            let popup_inner_area = popup_area.inner(Margin {
                horizontal: 1,
                vertical: 1,
            });

            let [summary_area, start_time_area, end_time_area] =
                popup_inner_area.layout(&Layout::vertical([Constraint::Length(3); 3]));

            self.popup.summary.render(summary_area, buf);
            self.popup.start_time.render(start_time_area, buf);
            self.popup.end_time.render(end_time_area, buf);
        }
    }
}
