mod ui;
mod app;
mod ssh_config;

use anyhow::Result;

fn main() -> Result<()> {
    app::run()
}
