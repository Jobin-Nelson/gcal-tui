use j_gcal::{Result, run};

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}
