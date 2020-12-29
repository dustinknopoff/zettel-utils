use chrono::prelude::*;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use sqlx::{Connection, Executor, SqliteConnection};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

static TAGS_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"#[A-Za-z0-9-._]+"#).unwrap());
static LINKS_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"\[([^\[]+)\]\((.*)\)"#).unwrap());
static HEADERS_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::RegexBuilder::new(r#"^(#{1,6})\s(.*)$"#)
        .multi_line(true)
        .build()
        .unwrap()
});

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let should_initialize = !Path::new("zettel.db").exists();
    if should_initialize {
        let _ = File::create("zettel.db")?;
    }
    let mut conn = SqliteConnection::connect("zettel.db").await?;
    if should_initialize {
        initialize_db(&mut conn).await?
    }
    let dir_entries: Vec<_> =
        walkdir::WalkDir::new("/Users/dustinknopoff/Documents/1-Areas/Notes/wiki")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("md")))
            .collect();
    let zettels = dir_entries
        .par_iter()
        .map(gather_info)
        .filter_map(|e| e.ok())
        .collect::<Vec<_>>();
    conn.execute("BEGIN").await?;
    for zettel in zettels {
        conn.execute(
            sqlx::query("INSERT OR REPLACE INTO full_text VALUES(?,?);")
                .bind(&zettel.zettel_id)
                .bind(&zettel.text),
        )
        .await?;
        conn.execute(
            sqlx::query("INSERT OR REPLACE INTO zettels VALUES(?,?,?);")
                .bind(&zettel.zettel_id)
                .bind(&zettel.title)
                .bind(&zettel.path.to_str()),
        )
        .await?;
        for (level, text) in zettel.headers {
            conn.execute(
                sqlx::query("INSERT OR REPLACE INTO headers VALUES(?,?,?);")
                    .bind(&zettel.zettel_id)
                    .bind(level)
                    .bind(text),
            )
            .await?;
        }
        for tag in zettel.tags {
            conn.execute(
                sqlx::query("INSERT OR REPLACE INTO tags VALUES(?,?);")
                    .bind(&zettel.zettel_id)
                    .bind(tag),
            )
            .await?;
        }
        for (label, link) in zettel.links {
            conn.execute(
                sqlx::query("INSERT OR REPLACE INTO links VALUES(?,?,?);")
                    .bind(&zettel.zettel_id)
                    .bind(label)
                    .bind(link),
            )
            .await?;
        }
    }
    conn.execute("COMMIT").await?;
    Ok(())
}

fn gather_info(entry: &walkdir::DirEntry) -> Result<ParserGatherer, anyhow::Error> {
    let path = entry.path().to_path_buf();
    let metadata = entry.metadata()?;
    let zettel_id: chrono::DateTime<Utc> = metadata.created()?.into();
    let zettel_id = zettel_id.format("%Y%m%d%H%M%S").to_string();
    let mut content = String::new();
    let mut file = File::open(&path)?;
    file.read_to_string(&mut content)?;
    let tags = TAGS_REGEX
        .captures_iter(&content)
        .filter_map(|v| v.get(0))
        .map(|v| v.as_str().to_string())
        .collect::<Vec<_>>();
    let links = LINKS_REGEX
        .captures_iter(&content)
        .filter(|v| v.get(0).is_some())
        .map(|v| {
            (
                v.get(1).unwrap().as_str().to_string(),
                v.get(2).unwrap().as_str().to_string(),
            )
        })
        .collect::<Vec<_>>();
    let headers = HEADERS_REGEX
        .captures_iter(&content)
        .filter(|v| v.get(0).is_some())
        .map(|v| (v[1].len() as i32, v.get(2).unwrap().as_str().to_string()))
        .collect::<Vec<_>>();
    let title = headers
        .iter()
        .find(|h| h.0 == 1)
        .map(|v| v.1.clone())
        .unwrap_or_else(|| path.to_str().unwrap().to_string());
    Ok(ParserGatherer {
        text: content,
        path,
        zettel_id,
        headers,
        tags,
        links,
        title,
    })
}

async fn initialize_db(conn: &mut SqliteConnection) -> Result<(), anyhow::Error> {
    conn.execute("BEGIN").await?;
    conn.execute(
        "PRAGMA foreign_keys=on;
PRAGMA WAL=on;",
    )
    .await?;
    conn.execute(
        "CREATE VIRTUAL TABLE full_text USING FTS5
(zettel_id, body);",
    )
    .await?;
    conn.execute(
        "CREATE TABLE zettels
(
    zettel_id TEXT PRIMARY KEY,
    title TEXT,
    file_path TEXT NOT NULL
);",
    )
    .await?;
    conn.execute(
        "CREATE TABLE headers
(
    zettel_id TEXT NOT NULL,
    level INTEGER NOT NULL,
    text TEXT,
        FOREIGN KEY (zettel_id) REFERENCES zettels(zettel_id)
);",
    )
    .await?;
    conn.execute(
        "CREATE TABLE tags
(
    zettel_id TEXT NOT NULL,
    tag TEXT,
    FOREIGN KEY (zettel_id) REFERENCES zettels(zettel_id)
);",
    )
    .await?;
    conn.execute(
        "CREATE TABLE links
(
    zettel_id TEXT NOT NULL,
    link TEXT,
    label TEXT,
    FOREIGN KEY (zettel_id) REFERENCES zettels(zettel_id)
);",
    )
    .await?;
    conn.execute("COMMIT").await?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct ParserGatherer {
    title: String,
    text: String,
    path: PathBuf,
    zettel_id: String,
    headers: Vec<(i32, String)>,
    links: Vec<(String, String)>,
    tags: Vec<String>,
}
