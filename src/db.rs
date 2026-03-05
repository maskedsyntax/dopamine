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
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT UNIQUE,
                title TEXT,
                artist TEXT,
                album TEXT,
                duration INTEGER
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_path ON tracks(path)",
            [],
        )?;
        
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE
            )",
            [],
        )?;

        // Check if playlist_tracks has the old schema (track_id)
        let has_track_id: bool = self.conn.query_row(
            "SELECT count(*) FROM pragma_table_info('playlist_tracks') WHERE name='track_id'",
            [],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(0) > 0;

        if has_track_id {
            let _ = self.conn.execute("DROP TABLE playlist_tracks", []);
        }

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id INTEGER,
                track_path TEXT,
                PRIMARY KEY(playlist_id, track_path),
                FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY(track_path) REFERENCES tracks(path) ON DELETE CASCADE
            )",
            [],
        )?;

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

    pub fn get_artists(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT artist FROM tracks ORDER BY artist")?;
        let artists = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(artists)
    }

    pub fn get_albums(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT album FROM tracks ORDER BY album")?;
        let albums = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(albums)
    }

    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, duration 
             FROM tracks 
             WHERE artist = ? 
             GROUP BY title, artist, album 
             ORDER BY album, title"
        )?;
        let tracks = stmt
            .query_map([artist], |row| {
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

    pub fn get_tracks_by_album(&self, album: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, duration 
             FROM tracks 
             WHERE album = ? 
             GROUP BY title, artist, album 
             ORDER BY title"
        )?;
        let tracks = stmt
            .query_map([album], |row| {
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

    pub fn get_playlists(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM playlists ORDER BY name")?;
        let playlists = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(playlists)
    }

    pub fn create_playlist(&self, name: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO playlists (name) VALUES (?1)", [name])?;
        Ok(())
    }

    pub fn add_track_to_playlist(&self, playlist_name: &str, track_path: &str) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT id FROM playlists WHERE name = ?1")?;
        let playlist_id: i64 = stmt.query_row([playlist_name], |row| row.get(0))?;

        self.conn.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_path) VALUES (?1, ?2)",
            params![playlist_id, track_path],
        )?;
        Ok(())
    }

    pub fn get_tracks_by_playlist(&self, playlist_name: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.path, t.title, t.artist, t.album, t.duration 
             FROM tracks t
             JOIN playlist_tracks pt ON t.path = pt.track_path
             JOIN playlists p ON pt.playlist_id = p.id
             WHERE p.name = ?1
             GROUP BY t.title, t.artist, t.album
             ORDER BY t.title"
        )?;
        let tracks = stmt
            .query_map([playlist_name], |row| {
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
