use arguments::{Config, Opts, SubCommand};
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use sqlx::{Connection, SqliteConnection};
use std::fs::{self, File};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

/// Command line arguments and Configuration file formats
pub mod arguments;
/// CRUD ops for database
pub mod db;
use db::{initialize, query};
/// Write out results
pub mod output;
use output::execute;

static TAGS_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"#[A-Za-z0-9-._]+"#).unwrap());
static LINKS_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"(\[([^\[]+)\]\((.*)\)|\[\[([^\[]+)\]\])"#).unwrap());
static HEADERS_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::RegexBuilder::new(r#"^(#{1,6})\s(.*)$"#)
        .multi_line(true)
        .build()
        .unwrap()
});

// TODO:
// Move queries into lib functions

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    use clap::Clap;
    let opts = Opts::parse();
    let config: Config = {
        if !Path::new("config.toml").exists() {
            anyhow::bail!("config.toml not found")
        }
        let contents = fs::read_to_string("config.toml")?;
        match toml::from_str(&contents) {
            Ok(config) => config,
            Err(_) => {
                anyhow::bail!(
                    "config.toml does not have a wiki-location and/or zettel-dateformat key"
                )
            }
        }
    };
    let should_initialize = !Path::new("zettel.db").exists();
    if should_initialize {
        let _ = File::create("zettel.db")?;
    }
    let mut conn = SqliteConnection::connect("zettel.db").await?;
    if should_initialize {
        initialize::initialize_db(&mut conn).await?;
        initialize::fill_db(&mut conn, &config).await?;
    }
    // If the DB didn't exist, we NEED to run create first
    match opts.subcmd {
        SubCommand::Create if !should_initialize => {
            initialize::fill_db(&mut conn, &config).await?;
        }
        SubCommand::FullText(ref s) => {
            let zettels = query::fulltext(&mut conn, &s.text).await?;
            execute(zettels, &opts.format)?;
        }
        SubCommand::Tags(ref s) => {
            let zettels = query::tags(&mut conn, &s.text).await?;
            execute(zettels, &opts.format)?;
        }
        SubCommand::Links(ref s) => {
            let zettels = query::links(&mut conn, &s.text).await?;
            execute(zettels, &opts.format)?;
        }
        SubCommand::Update(ref u) => {
            if u.all {
                initialize::fill_db(&mut conn, &config).await?;
            } else {
                initialize::fill_n(&mut conn, &config, &u.paths).await?;
            }
        }
        SubCommand::Create => return Ok(()),
        SubCommand::Watch => loop {
            // Create a channel to receive the events.
            let (tx, rx) = channel();

            // Create a watcher object, delivering debounced events.
            // The notification back-end is selected based on the platform.
            let mut watcher = watcher(tx, Duration::from_secs(10)).unwrap();

            // Add a path to be watched. All files and directories at that path and
            // below will be monitored for changes.
            watcher
                .watch(&config.wiki_location, RecursiveMode::Recursive)
                .unwrap();

            loop {
                match rx.recv() {
                    Ok(DebouncedEvent::Write(path)) => {
                        initialize::fill_n(&mut conn, &config, &[path]).await?;
                    }
                    Ok(DebouncedEvent::NoticeWrite(path)) => {
                        initialize::fill_n(&mut conn, &config, &[path]).await?;
                    }
                    // TODO: Implement name change and deletion handling
                    Ok(event) => println!("{:?}", event),
                    Err(e) => println!("watch error: {:?}", e),
                }
            }
        },
    }

    Ok(())
}
