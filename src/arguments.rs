use clap::Clap;
use serde::Deserialize;

#[derive(Clap, Debug)]
#[clap(name = "agnotestic-cli", version = "0.1")]
pub struct Opts {
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Clap, Debug)]
pub enum SubCommand {
    /// Search all documents in your wiki
    FullText(Search),
    /// Find all zettels with matching tag
    Tags(Search),
    /// Find backlinks
    Link(Search),
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
