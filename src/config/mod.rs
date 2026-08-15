use std::fs;

use anyhow::{Result, Error};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    target: String,
    jobs: u16,
    threads_per_job: u16,
    folders: Vec<String>
}

pub fn parse_toml(path: &str) -> Result<Config, Error> {
    let contents = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)?;

    Ok(config)
}