use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect, Spacing},
    symbols::merge::MergeStrategy,
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use crate::app::App;

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
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .spacing(Spacing::Overlap(1));
        let [header, calendar] = area.layout(&header_layout);
        let header_columns: [Rect; 8] = header.layout(&horizontal_layout);
        let columns: [Rect; 8] = calendar.layout(&horizontal_layout);

        let block = Block::bordered().merge_borders(MergeStrategy::Exact);

        // headers
        let headers = ["Time", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

        headers
            .iter()
            .zip(header_columns.iter())
            .for_each(|(&header, &header_column)| {
                Paragraph::new(header)
                    .block(block.clone())
                    .centered()
                    .render(header_column, buf);
            });

        // Time
        let mut lines = vec![];
        let start_hour = self.scroll_offset;
        let end_hour = self.scroll_offset + self.viewport_hours;
        for hour in start_hour..=end_hour {
            lines.push(Line::from(format!("{:02}:00", hour)));
            if hour == end_hour {
                continue;
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!("{:02}:30", hour)));
            lines.push(Line::from(""));
        }
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(block.clone())
            .render(columns[0], buf);

        // Calendar
        for day_idx in 0..=7 {
            let day_area = columns[day_idx];
            block.clone().render(day_area, buf);
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
