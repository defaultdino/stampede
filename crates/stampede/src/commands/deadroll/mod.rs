use std::process::ExitCode;

use clap::Parser;
use figment::Figment;

#[derive(Parser, Debug)]
pub struct Options {}

impl Options {
    pub fn run(self, _figment: &Figment) -> anyhow::Result<ExitCode> {
        anyhow::bail!("deadroll: not implemented")
    }
}
