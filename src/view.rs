use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    text::Line,
    widgets::{
        Block, BorderType, Borders, Paragraph, Widget,
        canvas::{Canvas, Line as CLine},
    },
};

use crate::app::App;

const TIME_HOUR: u8 = 24;

impl Widget for &App {
    /// Renders the user interface widgets.
    ///
    // This is where you add new widgets.
    // See the following resources:
    // - https://docs.rs/ratatui/latest/ratatui/widgets/index.html
    // - https://github.com/ratatui/ratatui/tree/master/examples
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header_layout = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]);
        let horizontal_layout = Layout::horizontal([Constraint::Fill(1); 8]);
        let [header, calendar] = area.layout(&header_layout);
        let [time_h, mon_h, tue_h, wed_h, thu_h, fri_h, sat_h, sun_h] =
            header.layout(&horizontal_layout);
        let [time, mon, tue, wed, thu, fri, sat, sun] = calendar.layout(&horizontal_layout);

        let block = Block::bordered().border_type(BorderType::Plain);
        let cal_border = Block::default().borders(Borders::LEFT | Borders::RIGHT);

        // headers
        let timeline = Paragraph::new("Time").block(block.clone()).centered();
        timeline.render(time_h, buf);

        // canvas
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
        let timeline: Paragraph = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(cal_border);
        timeline.render(time, buf);

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
