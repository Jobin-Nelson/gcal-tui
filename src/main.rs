use gcal_tui::{Result, run};

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}
