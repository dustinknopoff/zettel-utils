use clap::Clap;
use serde::Deserialize;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clap, Debug)]
#[clap(name = "agnotestic-cli", version = "0.1")]
pub struct Opts {
    #[clap(default_value = "stdout")]
    pub format: OutFormat,
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Clap, Debug)]
pub enum OutFormat {
    StdOut,
    Alfred,
    JSON,
}

impl Display for OutFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutFormat::StdOut => write!(f, "stdout"),
            OutFormat::Alfred => write!(f, "alfred"),
            OutFormat::JSON => write!(f, "json"),
        }
    }
}

impl FromStr for OutFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "stdout" => Ok(OutFormat::StdOut),
            "alfred" => Ok(OutFormat::Alfred),
            "json" => Ok(OutFormat::JSON),
            _ => Err(anyhow::anyhow!("{} is not stdout, alfred, or json", s)),
        }
    }
}

#[derive(Clap, Debug)]
pub enum SubCommand {
    /// Search all documents in your wiki
    FullText(Search),
    /// Find all zettels with matching tag
    Tags(Search),
    /// Find backlinks
    Links(Search),
    /// Creates a database storing your metadata about your zettels
    Create,
    /// Update all or some of the database
    Update(Update),
}

#[derive(Clap, Debug)]
pub struct Search {
    pub text: String,
}

#[derive(Clap, Debug)]
pub struct Update {
    /// Toggle to just UPSERT all wiki files
    #[clap(short)]
    pub all: bool,
    /// List of files that needed to be updated in database
    #[clap(long, short)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "wiki-location")]
    pub wiki_location: PathBuf,
    #[serde(rename = "zettel-dateformat")]
    pub zettel_date_format: String,
}