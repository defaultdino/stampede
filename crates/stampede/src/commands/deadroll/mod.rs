use anyhow::Context;
use clap::Parser;
use figment::Figment;
use media::job::process_media;
use serde::Serialize;
use std::{process::ExitCode, sync::Arc};

use crate::{commands::deadroll::pipeline::deadroll, config::Config};

mod blackdetect;
mod pipeline;
mod silencedetect;

#[derive(Serialize)]
struct DetectOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<usize>,
}

#[derive(Serialize)]
pub struct Overrides {
    detect: DetectOverrides,
}

#[derive(Parser, Debug)]
pub struct Options {
    #[arg(short, long)]
    length: Option<usize>,
}

impl Options {
    pub fn overrides(&self) -> Overrides {
        Overrides {
            detect: DetectOverrides {
                length: self.length,
            },
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
