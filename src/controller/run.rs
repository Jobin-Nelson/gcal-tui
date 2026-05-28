use crate::{App, Config, Result};

pub async fn run() -> Result<()> {
    // let config = Config::new()?;

    let terminal = ratatui::init();

    let result = App::new().run(terminal).await;
    ratatui::restore();

    // let cal = Calendar::new(config.calendar_ids).await?;
    //
    // let events = cal.get_events().await?;

    result
}
