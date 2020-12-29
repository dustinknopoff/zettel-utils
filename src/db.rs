use chrono::prelude::*;
use rayon::prelude::*;
use sqlx::{Executor, SqliteConnection};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::arguments::Config;
use crate::{HEADERS_REGEX, LINKS_REGEX, TAGS_REGEX};

pub mod initialize {
    use super::*;
    pub async fn fill_db(
        conn: &mut SqliteConnection,
        config: &Config,
    ) -> Result<(), anyhow::Error> {
        let dir_entries: Vec<_> = walkdir::WalkDir::new(config.wiki_location.as_path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("md")))
            .collect();
        let zettels = dir_entries
            .par_iter()
            .map(|e| gather_info(e, &config))
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        conn.execute("BEGIN").await?;
        let mut set = HashSet::new();
        for mut zettel in zettels {
            if set.contains(&zettel.zettel_id) {
                zettel.zettel_id = zettel
                    .path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
            }
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
            set.insert(zettel.zettel_id);
        }
        conn.execute("COMMIT").await?;
        Ok(())
    }

    fn gather_info(
        entry: &walkdir::DirEntry,
        config: &Config,
    ) -> Result<ParserGatherer, anyhow::Error> {
        let path = entry.path().to_path_buf();
        let metadata = entry.metadata()?;
        let zettel_id: chrono::DateTime<Utc> = metadata.created()?.into();
        let zettel_id = zettel_id.format(&config.zettel_date_format).to_string();
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
                if v.get(4).is_some() {
                    (
                        v.get(4).unwrap().as_str().to_string(),
                        v.get(4).unwrap().as_str().to_string(),
                    )
                } else {
                    (
                        v.get(1).unwrap().as_str().to_string(),
                        v.get(2).unwrap().as_str().to_string(),
                    )
                }
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

    pub async fn initialize_db(conn: &mut SqliteConnection) -> Result<(), anyhow::Error> {
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
    zettel_id TEXT UNIQUE PRIMARY KEY,
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
    pub struct ParserGatherer {
        title: String,
        text: String,
        path: PathBuf,
        zettel_id: String,
        headers: Vec<(i32, String)>,
        links: Vec<(String, String)>,
        tags: Vec<String>,
    }
}
