use crate::{App, Result, logging::initialize_logging};

pub async fn run() -> Result<()> {
    initialize_logging()?;

    let app = App::new().await?;

    let terminal = ratatui::init();
    let result = app.run(terminal).await;
    ratatui::restore();

    result
}
