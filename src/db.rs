use crate::models::Track;
use anyhow::Result;
use rusqlite::{params, Connection};

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn init(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY,
                path TEXT,
                title TEXT,
                artist TEXT,
                album TEXT,
                duration INTEGER
            )",
            [],
        )?;

        // Ensure path UNIQUE index exists independently of table creation
        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_path ON tracks(path)",
            [],
        )?;
        
        // Final cleanup for any duplicates that bypassed constraints
        self.conn.execute(
            "DELETE FROM tracks WHERE rowid NOT IN (SELECT MIN(rowid) FROM tracks GROUP BY path)",
            [],
        )?;
        
        Ok(())
    }

    pub fn clear_db(&self) -> Result<()> {
        self.conn.execute("DELETE FROM tracks", [])?;
        Ok(())
    }

    pub fn cleanup_stale_tracks(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT path FROM tracks")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();

        for path in paths {
            if !std::path::Path::new(&path).exists() {
                self.conn.execute("DELETE FROM tracks WHERE path = ?", [path])?;
            }
        }
        Ok(())
    }

    pub fn insert_track(&self, track: &Track) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO tracks (path, title, artist, album, duration)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                track.path,
                track.title,
                track.artist,
                track.album,
                track.duration_secs
            ],
        )?;
        Ok(())
    }

    pub fn get_all_tracks(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, duration 
             FROM tracks 
             GROUP BY title, artist, album 
             ORDER BY artist, album, title"
        )?;
        let tracks = stmt
            .query_map([], |row| {
                Ok(Track {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    duration_secs: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(tracks)
    }
}
