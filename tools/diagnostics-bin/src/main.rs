use anyhow::Result;
use james_diagnostics::Diagnostics;

#[tokio::main]
async fn main() -> Result<()> {
    // Standalone diagnostics shell: no core attached yet, so every
    // section without data reports "not available" instead of guessing.
    let diagnostics = Diagnostics::default();
    diagnostics.start().await?;
    let result = diagnostics.run_cli().await;
    diagnostics.stop().await?;
    result
}
