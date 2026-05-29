use crate::{App, Calendar, Config, Result, logging::initialize_logging};

pub async fn run() -> Result<()> {
    initialize_logging()?;
    let config = Config::new()?;

    let cal = Calendar::new(config.calendar_ids).await?;
    let events = cal.get_events().await?;

    let terminal = ratatui::init();
    let result = App::new(events).run(terminal).await;
    ratatui::restore();

    result
}
