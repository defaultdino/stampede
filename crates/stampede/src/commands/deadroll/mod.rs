use anyhow::Context;
use clap::Parser;
use figment::Figment;
use media::job::process_media;
use serde::Serialize;
use std::{process::ExitCode, sync::Arc};

use crate::{
    commands::{ForceOptions, ProcessingOptions, ProcessingOverrides, deadroll::pipeline::deadroll},
    config::Config,
};

mod analysis;
mod filter;
mod pipeline;

#[derive(Serialize)]
struct BlackrollOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    min_duration: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_db: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force: Option<bool>,
}

#[derive(Serialize)]
pub struct Overrides {
    blackroll: BlackrollOverrides,
    processing: ProcessingOverrides,
}

#[derive(Parser, Debug)]
pub struct Options {
    #[arg(long)]
    min_duration: Option<usize>,
    #[arg(long)]
    min_db: Option<usize>,
    #[command(flatten)]
    processing: ProcessingOptions,
    #[command(flatten)]
    force: ForceOptions,
}

impl Options {
    pub fn overrides(&self) -> Overrides {
        Overrides {
            blackroll: BlackrollOverrides {
                min_duration: self.min_duration,
                min_db: self.min_db,
                force: self.force.overrides(),
            },
            processing: self.processing.overrides(),
        }
    }

    pub fn run(self, figment: &Figment) -> anyhow::Result<ExitCode> {
        let config: Arc<Config> = Arc::new(figment.extract().context("failed to extract config")?);
        let closure_config = config.clone();

        process_media(&config.processing, move |path| {
            match deadroll(&closure_config, path) {
                Ok(_) => {
                    log::info!("finished deadrolling media streams");
                }
                Err(e) => {
                    log::error!("failed to deadroll detect media streams: {}", e);
                }
            }
        });

        Ok(ExitCode::SUCCESS)
    }
}

fn intersect_ranges(a: &[(i64, i64)], b: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut result = Vec::new();
    for &(a_start, a_end) in a {
        for &(b_start, b_end) in b {
            let start = a_start.max(b_start);
            let end = a_end.min(b_end);
            if start < end {
                result.push((start, end));
            }
        }
    }
    result
}
