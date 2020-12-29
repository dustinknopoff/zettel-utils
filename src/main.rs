use arguments::{Config, Opts, SubCommand};
use db::Zettel;
use once_cell::sync::Lazy;
use sqlx::{Connection, SqliteConnection};
use std::fs::{self, File};
use std::path::Path;

pub mod arguments;
pub mod db;
pub use db::initialize;

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
// - Config file with: date format for id, wiki location, format to receive links, what to call to open files
// - Add clap and provide calls in for creating DB, updating DB, searching
// - Search:
// - Fulltext
// - Tag
// - Has link to received path

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
            let zettels = sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, file_path FROM full_text ft JOIN zettels z ON z.zettel_id = ft.zettel_id WHERE full_text MATCH ?")
            .bind(&s.text)
            .fetch_all(&mut conn);
            dbg!(&zettels.await?);
        }
        SubCommand::Tags(ref s) => {
            let zettels = sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, file_path FROM tags t JOIN zettels z ON z.zettel_id = t.zettel_id WHERE tag LIKE ?;")
            .bind(format!("%{}%",&s.text))
            .fetch_all(&mut conn);
            dbg!(&zettels.await?);
        }
        SubCommand::Links(ref s) => {
            let zettels = sqlx::query_as::<_, Zettel>("SELECT DISTINCT z.zettel_id, title, file_path FROM links l JOIN zettels z ON z.zettel_id = l.zettel_id WHERE link LIKE ?;")
            .bind(format!("%{}%",&s.text))
            .fetch_all(&mut conn);
            dbg!(&zettels.await?);
        }
        SubCommand::Update(ref u) => {
            if u.all {
                initialize::fill_db(&mut conn, &config).await?;
            } else {
                initialize::fill_n(&mut conn, &config, &u.paths).await?;
            }
        }
        SubCommand::Create => return Ok(()),
    }

    Ok(())
}
