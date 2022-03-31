use chrono::prelude::*;
use rayon::prelude::*;
use serde::Serialize;
use sqlx::{Executor, SqliteConnection};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::arguments::Config;
use crate::{HEADERS_REGEX, LINKS_REGEX, TAGS_REGEX};

#[derive(sqlx::FromRow, Debug, Clone, Serialize)]
pub struct Zettel {
    pub zettel_id: String,
    pub timestamp: i64,
    pub title: String,
    pub file_path: String,
}

/// Functions for initializing and updating a Zettel Database
pub mod edit {
    use super::*;
    use std::fs::{self, Metadata};
    use std::path::Path;
    use uuid::Uuid;
    use walkdir::DirEntry;
    /// Walk [config.wiki_location](crate::arguments::Config) for markdown files and add metadata to database.
    pub async fn fill_db(
        conn: &mut SqliteConnection,
        config: &Config,
        after: Option<DateTime<Utc>>,
    ) -> Result<(), anyhow::Error> {
        let dir_entries: Vec<_> = walkdir::WalkDir::new(config.wiki_location.as_path())
            .into_iter()
            .filter_map(|e| e.ok())
            // Implicitly filters out directory entries
            .filter(|e| e.path().extension() == Some(OsStr::new("md")))
            .filter(|e| {
                if let Some(after) = after {
                    created(e).unwrap() > after
                } else {
                    true
                }
            })
            .collect();
        let zettels = dir_entries
            .par_iter()
            .map(|e| gather_info(e.clone().into_path(), e.metadata().unwrap()))
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        add_to_db(conn, zettels).await?;
        Ok(())
    }

    fn created(e: &DirEntry) -> Result<DateTime<Utc>, anyhow::Error> {
        Ok(Into::<DateTime<Utc>>::into(e.metadata()?.created()?))
    }

    /// For any given Zettel, insert the following:
    ///
    /// - Entire zettel into the full text search table
    /// - Extracted links into the links table
    /// - Extracted tags into the tags table
    /// - Extracted headers into the headers table
    /// - High level metadata of zettels into the zettels table.
    ///
    async fn add_to_db(
        conn: &mut SqliteConnection,
        zettels: Vec<ParserGatherer>,
    ) -> Result<(), anyhow::Error> {
        conn.execute("BEGIN").await?;
        for zettel in zettels {
            conn.execute(
                sqlx::query("INSERT OR REPLACE INTO full_text VALUES(?,?);")
                    .bind(&zettel.zettel_id)
                    .bind(&zettel.text),
            )
            .await?;
            conn.execute(
                sqlx::query("INSERT OR REPLACE INTO zettels VALUES(?,?,?,?);")
                    .bind(&zettel.zettel_id)
                    .bind(&zettel.timestamp)
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

    /// Incredibly similar to [fill_db](crate::db::initialize::fill_db) except that it operates on a received list of Paths rather than walking the config path for markdown files
    pub async fn fill_n(
        conn: &mut SqliteConnection,
        paths: &[PathBuf],
    ) -> Result<(), anyhow::Error> {
        let zettels = paths
            .par_iter()
            .map(|path| gather_info(path.clone(), fs::metadata(path).unwrap()))
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        add_to_db(conn, zettels).await?;
        Ok(())
    }

    /// From a file path, gather the following data
    /// - A `zettel_id` from the created timestamp
    /// - The contents of file
    /// - The title of the zettel (basename of filename)
    /// - list of tags
    /// - list of links
    /// - list of headers
    fn gather_info(path: PathBuf, metadata: Metadata) -> Result<ParserGatherer, anyhow::Error> {
        let timestamp: chrono::DateTime<Utc> = metadata.created()?.into();
        let timestamp = timestamp.timestamp();
        let mut content = String::new();
        let mut file = File::open(&path)?;
        file.read_to_string(&mut content)?;
        let zettel_id = Uuid::new_v4().to_string();
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
            timestamp,
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
    timestamp INTEGER,
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
        timestamp: i64,
        zettel_id: String,
        /// (Header Level, Text)
        headers: Vec<(i32, String)>,
        /// (Label, URL)
        links: Vec<(String, String)>,
        tags: Vec<String>,
    }

    /// Update a zettel when notified of a file name change
    pub async fn namechange(
        conn: &mut SqliteConnection,
        old: &Path,
        new: &Path,
    ) -> Result<(), anyhow::Error> {
        let id = query::get_by_path(conn, old.to_str().unwrap())
            .await?
            .zettel_id;
        let new_timestamp = {
            let meta = fs::metadata(new)?;
            let ts: chrono::DateTime<Utc> = meta.created()?.into();
            ts.timestamp()
        };
        let new_title = new.file_name().unwrap().to_str();
        let old_title = old.file_name().unwrap().to_str();
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE zettels SET timestamp = ?, title = ? WHERE zettel_id = ?",
            )
            .bind(&new_timestamp)
            .bind(new_title)
            .bind(old_title)
            .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>("UPDATE full_text SET timestamp = ? WHERE zettel_id = ?")
                .bind(&new_timestamp)
                .bind(old_title)
                .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE headers SET timestamp = ?, title = ? zettel_id = ?",
            )
            .bind(&new_timestamp)
            .bind(new_title)
            .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE links SET timestamp = ?, title = ? WHERE zettel_id = ?",
            )
            .bind(&new_timestamp)
            .bind(new_title)
            .bind(old_title)
            .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE tags SET timestamp = ?, title = ? WHERE zettel_id = ?",
            )
            .bind(&new_timestamp)
            .bind(new_title)
            .bind(&id),
        )
        .await?;
        Ok(())
    }

    /// Update a zettel when notified of a file name change
    pub async fn remove(conn: &mut SqliteConnection, old: &Path) -> Result<(), anyhow::Error> {
        let id = query::get_by_path(conn, old.to_str().unwrap())
            .await?
            .zettel_id;
        let old_title = old.file_name().unwrap().to_str();
        conn.execute(
            sqlx::query_as::<_, Zettel>("DROP FROM TABLE zettels WHERE zettel_id = ? ")
                .bind(old_title)
                .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>("DROP FROM TABLE headers WHERE zettel_id = ?")
                .bind(old_title)
                .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>("DROP FROM TABLE links WHERE zettel_id = ?")
                .bind(old_title)
                .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>("DROP FROM TABLE tags WHERE zettel_id = ?")
                .bind(old_title)
                .bind(&id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>("DROP FROM TABLE full_text WHERE zettel_id = ?")
                .bind(old_title),
        )
        .await?;
        Ok(())
    }
}

/// Functions for querying a Zettel Database
pub mod query {
    use super::*;
    /// Search full text search table for zettels matching `text`
    pub async fn fulltext(
        conn: &mut SqliteConnection,
        text: &str,
    ) -> Result<Vec<Zettel>, anyhow::Error> {
        Ok(sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, timestamp, file_path FROM full_text ft JOIN zettels z ON z.zettel_id = ft.zettel_id WHERE full_text MATCH ? ORDER BY rank;")
            .bind(text)
            .fetch_all( conn).await?)
    }

    /// Search links table for zettels matching `text`
    pub async fn links(
        conn: &mut SqliteConnection,
        text: &str,
    ) -> Result<Vec<Zettel>, anyhow::Error> {
        Ok(sqlx::query_as::<_, Zettel>("SELECT DISTINCT z.zettel_id, title, timestamp, file_path FROM links l JOIN zettels z ON z.zettel_id = l.zettel_id WHERE link LIKE ?;")
            .bind(format!("%{}%",text))
            .fetch_all( conn).await?)
    }

    /// Search tags table for zettels matching `text`
    pub async fn tags(
        conn: &mut SqliteConnection,
        text: &str,
    ) -> Result<Vec<Zettel>, anyhow::Error> {
        Ok(sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, timestamp, file_path FROM tags t JOIN zettels z ON z.zettel_id = t.zettel_id WHERE tag LIKE ?;")
            .bind(format!("%{}%",text))
            .fetch_all( conn).await?)
    }

    pub async fn get_by_path(
        conn: &mut SqliteConnection,
        path: &str,
    ) -> Result<Zettel, anyhow::Error> {
        Ok(
            sqlx::query_as::<_, Zettel>("SELECT zettel_id FROM zettels WHERE file_path = ?")
                .bind(path)
                .fetch_one(conn)
                .await?,
        )
    }

    pub async fn latest_zettel(conn: &mut SqliteConnection) -> Result<Zettel, anyhow::Error> {
        Ok(
            sqlx::query_as::<_, Zettel>("select * from zettels order by timestamp DESC limit 1;")
                .fetch_one(conn)
                .await?,
        )
    }
}
