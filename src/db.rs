use chrono::prelude::*;
use rayon::prelude::*;
use serde::Serialize;
use sqlx::{Executor, SqliteConnection};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::arguments::Config;
use crate::{HEADERS_REGEX, LINKS_REGEX, TAGS_REGEX};

#[derive(sqlx::FromRow, Debug, Clone, Serialize)]
pub struct Zettel {
    pub zettel_id: String,
    pub title: String,
    pub file_path: String,
}

/// Functions for initializing and updating a Zettel Database
pub mod edit {
    use super::*;
    use std::fs::{self, Metadata};
    use std::path::Path;
    /// Walk [config.wiki_location](crate::arguments::Config) for markdown files and add metadata to database.
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
            .map(|e| gather_info(e.clone().into_path(), e.metadata().unwrap(), &config))
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        add_to_db(conn, zettels).await?;
        Ok(())
    }

    /// For any given Zettel, insert the following:
    ///
    /// - Entire zettel into the full text search table
    /// - Extracted links into the links table
    /// - Extracted tags into the tags table
    /// - Extracted headers into the headers table
    /// - High level metadata of zettels into the zettels table.
    ///
    /// **NOTE**: In the case of duplicate `zettel_ids` the basename of the file will be used
    async fn add_to_db(
        conn: &mut SqliteConnection,
        zettels: Vec<ParserGatherer>,
    ) -> Result<(), anyhow::Error> {
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

    /// Incredibly similar to [fill_db](crate::db::initialize::fill_db) except that it operates on a received list of Paths rather than walking the config path for markdown files
    pub async fn fill_n(
        conn: &mut SqliteConnection,
        config: &Config,
        paths: &[PathBuf],
    ) -> Result<(), anyhow::Error> {
        let zettels = paths
            .par_iter()
            .map(|path| gather_info(path.clone(), fs::metadata(path).unwrap(), &config))
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
    fn gather_info(
        path: PathBuf,
        metadata: Metadata,
        config: &Config,
    ) -> Result<ParserGatherer, anyhow::Error> {
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
        /// (Header Level, Text)
        headers: Vec<(i32, String)>,
        /// (Label, URL)
        links: Vec<(String, String)>,
        tags: Vec<String>,
    }

    /// Update a zettel when notified of a file name change
    pub async fn namechange(
        conn: &mut SqliteConnection,
        config: &Config,
        old: &Path,
        new: &Path,
    ) -> Result<(), anyhow::Error> {
        let maybe_id = {
            let meta = fs::metadata(old)?;
            let zettel_id: chrono::DateTime<Utc> = meta.created()?.into();
            zettel_id.format(&config.zettel_date_format).to_string()
        };
        let new_id = {
            let meta = fs::metadata(new)?;
            let zettel_id: chrono::DateTime<Utc> = meta.created()?.into();
            zettel_id.format(&config.zettel_date_format).to_string()
        };
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE zettels SET zettel_id = ?, title = ? WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(&new_id)
            .bind(new.file_name().unwrap().to_str())
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE full_text SET zettel_id = ? WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(&new_id)
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE headers SET zettel_id = ?, title = ? WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(&new_id)
            .bind(new.file_name().unwrap().to_str())
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE links SET zettel_id = ?, title = ? WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(&new_id)
            .bind(new.file_name().unwrap().to_str())
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "UPDATE tags SET zettel_id = ?, title = ? WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(&new_id)
            .bind(new.file_name().unwrap().to_str())
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        Ok(())
    }

    /// Update a zettel when notified of a file name change
    pub async fn remove(
        conn: &mut SqliteConnection,
        config: &Config,
        old: &Path,
    ) -> Result<(), anyhow::Error> {
        let maybe_id = {
            let meta = fs::metadata(old)?;
            let zettel_id: chrono::DateTime<Utc> = meta.created()?.into();
            zettel_id.format(&config.zettel_date_format).to_string()
        };
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "DROP FROM TABLE zettels WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "DROP FROM TABLE headers WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "DROP FROM TABLE links WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "DROP FROM TABLE tags WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
        )
        .await?;
        conn.execute(
            sqlx::query_as::<_, Zettel>(
                "DROP FROM TABLE full_text WHERE zettel_id = ? OR zettel_id = ?",
            )
            .bind(old.file_name().unwrap().to_str())
            .bind(&maybe_id),
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
        Ok(sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, file_path FROM full_text ft JOIN zettels z ON z.zettel_id = ft.zettel_id WHERE full_text MATCH ?")
            .bind(text)
            .fetch_all( conn).await?)
    }

    /// Search links table for zettels matching `text`
    pub async fn links(
        conn: &mut SqliteConnection,
        text: &str,
    ) -> Result<Vec<Zettel>, anyhow::Error> {
        Ok(sqlx::query_as::<_, Zettel>("SELECT DISTINCT z.zettel_id, title, file_path FROM links l JOIN zettels z ON z.zettel_id = l.zettel_id WHERE link LIKE ?;")
            .bind(format!("%{}%",text))
            .fetch_all( conn).await?)
    }

    /// Search tags table for zettels matching `text`
    pub async fn tags(
        conn: &mut SqliteConnection,
        text: &str,
    ) -> Result<Vec<Zettel>, anyhow::Error> {
        Ok(sqlx::query_as::<_, Zettel>("SELECT z.zettel_id, title, file_path FROM tags t JOIN zettels z ON z.zettel_id = t.zettel_id WHERE tag LIKE ?;")
            .bind(format!("%{}%",text))
            .fetch_all( conn).await?)
    }
}
