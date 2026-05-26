use crate::{Calendar, Config, Result};

pub async fn run() -> Result<()> {
    let config = Config::new()?;
    dbg!(&config);

    let cal = Calendar::new(config.calendar_ids).await?;

    let events = cal.get_events().await?;

    Ok(())
}
