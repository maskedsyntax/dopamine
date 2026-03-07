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
                genre TEXT,
                year INTEGER,
                favorite INTEGER DEFAULT 0,
                play_count INTEGER DEFAULT 0,
                last_played INTEGER,
                lyrics TEXT,
                duration INTEGER
            )",
            [],
        )?;
        
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN genre TEXT DEFAULT 'Unknown'", []);
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN year INTEGER DEFAULT 0", []);
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN favorite INTEGER DEFAULT 0", []);
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN play_count INTEGER DEFAULT 0", []);
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN last_played INTEGER", []);
        let _ = self.conn.execute("ALTER TABLE tracks ADD COLUMN lyrics TEXT", []);

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

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        )?;
        
        Ok(())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let res = stmt.query_row([key], |row| row.get(0)).ok();
        Ok(res)
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
            "INSERT OR REPLACE INTO tracks (path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                track.path,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.year,
                if track.favorite { 1 } else { 0 },
                track.play_count,
                track.last_played,
                track.lyrics,
                track.duration_secs
            ],
        )?;
        Ok(())
    }

    pub fn toggle_favorite(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET favorite = (1 - favorite) WHERE path = ?1",
            [path],
        )?;
        Ok(())
    }

    pub fn record_play(&self, path: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.conn.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = ?1 WHERE path = ?2",
            params![now, path],
        )?;
        Ok(())
    }

    pub fn update_track_lyrics(&self, path: &str, lyrics: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET lyrics = ?1 WHERE path = ?2",
            [lyrics, path],
        )?;
        Ok(())
    }

    pub fn get_favorites(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE favorite = 1 
             GROUP BY title, artist, album
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    pub fn get_recently_played(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE last_played IS NOT NULL 
             GROUP BY title, artist, album
             ORDER BY last_played DESC 
             LIMIT 50"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    pub fn get_most_played(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE play_count > 0 
             GROUP BY title, artist, album
             ORDER BY play_count DESC 
             LIMIT 50"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    fn map_tracks(&self, stmt: &mut rusqlite::Statement, params: impl rusqlite::Params) -> Result<Vec<Track>> {
        let tracks = stmt
            .query_map(params, |row| {
                Ok(Track {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    genre: row.get(4)?,
                    year: row.get(5)?,
                    favorite: row.get::<_, i32>(6)? == 1,
                    play_count: row.get(7)?,
                    last_played: row.get(8)?,
                    lyrics: row.get(9)?,
                    duration_secs: row.get(10)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(tracks)
    }

    pub fn get_artists(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT artist FROM tracks WHERE artist != 'Unknown Artist' ORDER BY artist")?;
        let artists = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(artists)
    }

    pub fn get_albums(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT album FROM tracks WHERE album != 'Unknown Album' ORDER BY album")?;
        let albums = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(albums)
    }

    pub fn get_genres(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT genre FROM tracks WHERE genre != 'Unknown' ORDER BY genre")?;
        let genres = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(genres)
    }

    pub fn get_years(&self) -> Result<Vec<i32>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT year FROM tracks WHERE year > 0 ORDER BY year DESC")?;
        let years = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(years)
    }

    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE artist = ? 
             GROUP BY title, artist, album
             ORDER BY album, title"
        )?;
        self.map_tracks(&mut stmt, [artist])
    }

    pub fn get_tracks_by_album(&self, album: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE album = ? 
             GROUP BY title, artist, album
             ORDER BY title"
        )?;
        self.map_tracks(&mut stmt, [album])
    }

    pub fn get_tracks_by_genre(&self, genre: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE genre = ? 
             GROUP BY title, artist, album
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [genre])
    }

    pub fn get_tracks_by_year(&self, year: i32) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             WHERE year = ? 
             GROUP BY title, artist, album
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [year])
    }

    pub fn get_all_tracks(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, duration 
             FROM tracks 
             GROUP BY title, artist, album
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [])
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

    pub fn delete_playlist(&self, name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM playlists WHERE name = ?1", [name])?;
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
            "SELECT t.path, t.title, t.artist, t.album, t.genre, t.year, t.favorite, t.play_count, t.last_played, t.lyrics, t.duration 
             FROM tracks t
             JOIN playlist_tracks pt ON t.path = pt.track_path
             JOIN playlists p ON pt.playlist_id = p.id
             WHERE p.name = ?1
             ORDER BY t.title"
        )?;
        self.map_tracks(&mut stmt, [playlist_name])
    }
}
