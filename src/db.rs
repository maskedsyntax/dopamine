use crate::models::{Track, artist_credit_matches, artist_credits};
use anyhow::Result;
use rusqlite::{Connection, Transaction, params};
use std::collections::HashMap;

const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub name: String,
    pub track_count: i64,
    pub duration_secs: i64,
    pub representative_track: Option<Track>,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn init(&mut self) -> Result<()> {
        self.conn.pragma_update(None, "foreign_keys", true)?;
        let tx = self.conn.transaction()?;

        tx.execute(
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
                lyrics_offset INTEGER DEFAULT 0,
                duration INTEGER
            )",
            [],
        )?;

        add_column_if_missing(&tx, "genre", "TEXT DEFAULT 'Unknown'")?;
        add_column_if_missing(&tx, "year", "INTEGER DEFAULT 0")?;
        add_column_if_missing(&tx, "favorite", "INTEGER DEFAULT 0")?;
        add_column_if_missing(&tx, "play_count", "INTEGER DEFAULT 0")?;
        add_column_if_missing(&tx, "last_played", "INTEGER")?;
        add_column_if_missing(&tx, "lyrics", "TEXT")?;
        add_column_if_missing(&tx, "lyrics_offset", "INTEGER DEFAULT 0")?;
        add_column_if_missing(&tx, "duration", "INTEGER DEFAULT 0")?;

        tx.execute(
            "DELETE FROM tracks WHERE rowid NOT IN (SELECT MIN(rowid) FROM tracks GROUP BY path)",
            [],
        )?;

        tx.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_path ON tracks(path)",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE
            )",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id INTEGER,
                track_path TEXT,
                PRIMARY KEY(playlist_id, track_path),
                FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY(track_path) REFERENCES tracks(path) ON DELETE CASCADE
            )",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        )?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
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
                self.conn
                    .execute("DELETE FROM tracks WHERE path = ?", [path])?;
            }
        }
        Ok(())
    }

    pub fn insert_track(&self, track: &Track) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tracks (path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(path) DO UPDATE SET
                 title = excluded.title,
                 artist = excluded.artist,
                 album = excluded.album,
                 genre = excluded.genre,
                 year = excluded.year,
                 lyrics = COALESCE(excluded.lyrics, tracks.lyrics),
                 duration = excluded.duration",
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
                track.lyrics_offset_ms,
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

    pub fn update_lyrics_offset(&self, path: &str, offset_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET lyrics_offset = ?1 WHERE path = ?2",
            params![offset_ms, path],
        )?;
        Ok(())
    }

    pub fn get_favorites(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE favorite = 1 
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    pub fn get_recently_played(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE last_played IS NOT NULL 
             ORDER BY last_played DESC 
             LIMIT 50"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    pub fn get_most_played(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE play_count > 0 
             ORDER BY play_count DESC 
             LIMIT 50"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    fn map_tracks(
        &self,
        stmt: &mut rusqlite::Statement,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Track>> {
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
                    lyrics_offset_ms: row.get(10)?,
                    duration_secs: row.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(tracks)
    }

    pub fn get_artists(&self) -> Result<Vec<String>> {
        let mut grouped: HashMap<String, (String, bool)> = HashMap::new();
        for track in self.get_all_tracks()? {
            let credits = artist_credits(&track.artist);
            let standalone = credits.len() == 1;
            for credit in credits {
                if credit.eq_ignore_ascii_case("Unknown Artist") {
                    continue;
                }
                let key = credit.to_lowercase();
                let entry = grouped
                    .entry(key)
                    .or_insert_with(|| (credit.to_string(), standalone));
                if standalone && !entry.1 {
                    *entry = (credit.to_string(), true);
                }
            }
        }
        let mut artists = grouped
            .into_values()
            .map(|(artist, _)| artist)
            .collect::<Vec<_>>();
        artists.sort_by_key(|artist| artist.to_lowercase());
        Ok(artists)
    }

    pub fn get_albums(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT album
             FROM tracks
             WHERE trim(album) != '' AND album != 'Unknown Album'
             GROUP BY album
             HAVING COUNT(*) > 1 OR lower(trim(album)) != lower(trim(MIN(title)))
             ORDER BY album COLLATE NOCASE",
        )?;
        let albums = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(albums)
    }

    pub fn get_genres(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT genre FROM tracks WHERE genre != 'Unknown' ORDER BY genre")?;
        let genres = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(genres)
    }

    pub fn get_years(&self) -> Result<Vec<i32>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT year FROM tracks WHERE year > 0 ORDER BY year DESC")?;
        let years = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(years)
    }

    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>> {
        let mut tracks = self
            .get_all_tracks()?
            .into_iter()
            .filter(|track| artist_credit_matches(&track.artist, artist))
            .collect::<Vec<_>>();
        tracks.sort_by(|left, right| {
            left.album
                .to_lowercase()
                .cmp(&right.album.to_lowercase())
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        });
        Ok(tracks)
    }

    pub fn get_tracks_by_album(&self, album: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE album = ? 
             ORDER BY title"
        )?;
        self.map_tracks(&mut stmt, [album])
    }

    pub fn get_tracks_by_genre(&self, genre: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE genre = ? 
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [genre])
    }

    pub fn get_tracks_by_year(&self, year: i32) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             WHERE year = ? 
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [year])
    }

    pub fn get_all_tracks(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, genre, year, favorite, play_count, last_played, lyrics, lyrics_offset, duration 
             FROM tracks 
             ORDER BY artist, album, title"
        )?;
        self.map_tracks(&mut stmt, [])
    }

    pub fn get_playlists(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM playlists ORDER BY name")?;
        let playlists = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(playlists)
    }

    pub fn get_playlist_summaries(&self) -> Result<Vec<PlaylistSummary>> {
        let mut stmt = self.conn.prepare(
            "WITH summaries AS (
                SELECT
                    p.id,
                    p.name,
                    COUNT(t.path) AS track_count,
                    COALESCE(SUM(t.duration), 0) AS duration_secs,
                    MIN(t.path) AS representative_path
                FROM playlists p
                LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
                LEFT JOIN tracks t ON t.path = pt.track_path
                GROUP BY p.id, p.name
            )
            SELECT
                s.name,
                s.track_count,
                s.duration_secs,
                t.path,
                t.title,
                t.artist,
                t.album,
                t.genre,
                t.year,
                t.favorite,
                t.play_count,
                t.last_played,
                t.lyrics,
                t.lyrics_offset,
                t.duration
            FROM summaries s
            LEFT JOIN tracks t ON t.path = s.representative_path
            ORDER BY s.name",
        )?;

        let summaries = stmt
            .query_map([], |row| {
                let representative_track = match row.get::<_, Option<String>>(3)? {
                    Some(path) => Some(Track {
                        path,
                        title: row.get(4)?,
                        artist: row.get(5)?,
                        album: row.get(6)?,
                        genre: row.get(7)?,
                        year: row.get(8)?,
                        favorite: row.get::<_, i32>(9)? == 1,
                        play_count: row.get(10)?,
                        last_played: row.get(11)?,
                        lyrics: row.get(12)?,
                        lyrics_offset_ms: row.get(13)?,
                        duration_secs: row.get(14)?,
                    }),
                    None => None,
                };

                Ok(PlaylistSummary {
                    name: row.get(0)?,
                    track_count: row.get(1)?,
                    duration_secs: row.get(2)?,
                    representative_track,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(summaries)
    }

    pub fn create_playlist(&self, name: &str) -> Result<()> {
        self.conn
            .execute("INSERT OR IGNORE INTO playlists (name) VALUES (?1)", [name])?;
        Ok(())
    }

    pub fn delete_playlist(&self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE name = ?1", [name])?;
        Ok(())
    }

    pub fn add_track_to_playlist(&self, playlist_name: &str, track_path: &str) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM playlists WHERE name = ?1")?;
        let playlist_id: i64 = stmt.query_row([playlist_name], |row| row.get(0))?;

        self.conn.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_path) VALUES (?1, ?2)",
            params![playlist_id, track_path],
        )?;
        Ok(())
    }

    pub fn remove_track_from_playlist(&self, playlist_name: &str, track_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM playlist_tracks
             WHERE playlist_id = (SELECT id FROM playlists WHERE name = ?1)
               AND track_path = ?2",
            params![playlist_name, track_path],
        )?;
        Ok(())
    }

    pub fn get_tracks_by_playlist(&self, playlist_name: &str) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.path, t.title, t.artist, t.album, t.genre, t.year, t.favorite, t.play_count, t.last_played, t.lyrics, t.lyrics_offset, t.duration 
             FROM tracks t
             JOIN playlist_tracks pt ON t.path = pt.track_path
             JOIN playlists p ON pt.playlist_id = p.id
             WHERE p.name = ?1
             ORDER BY t.title"
        )?;
        self.map_tracks(&mut stmt, [playlist_name])
    }

    pub fn get_total_stats(&self) -> Result<(i64, i64)> {
        let mut stmt = self
            .conn
            .prepare("SELECT SUM(play_count), SUM(play_count * duration) FROM tracks")?;
        let res = stmt.query_row([], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            ))
        })?;
        Ok(res)
    }

    pub fn get_top_artists(&self) -> Result<Vec<(String, i64)>> {
        let canonical = self
            .get_artists()?
            .into_iter()
            .map(|artist| (artist.to_lowercase(), artist))
            .collect::<HashMap<_, _>>();
        let mut totals = HashMap::<String, i64>::new();
        for track in self
            .get_all_tracks()?
            .into_iter()
            .filter(|track| track.play_count > 0)
        {
            for credit in artist_credits(&track.artist) {
                if !credit.eq_ignore_ascii_case("Unknown Artist") {
                    *totals.entry(credit.to_lowercase()).or_default() +=
                        i64::from(track.play_count);
                }
            }
        }
        let mut artists = totals
            .into_iter()
            .map(|(key, plays)| (canonical.get(&key).cloned().unwrap_or(key), plays))
            .collect::<Vec<_>>();
        artists.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
        });
        artists.truncate(10);
        Ok(artists)
    }
}

fn add_column_if_missing(tx: &Transaction<'_>, name: &str, definition: &str) -> Result<()> {
    let mut stmt = tx.prepare("PRAGMA table_info(tracks)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if !columns.iter().any(|column| column == name) {
        tx.execute(
            &format!("ALTER TABLE tracks ADD COLUMN {name} {definition}"),
            [],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str) -> Track {
        Track {
            path: path.to_string(),
            title: "Original".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            genre: "Genre".to_string(),
            year: 2020,
            favorite: true,
            play_count: 7,
            last_played: Some(123),
            duration_secs: 180,
            lyrics: Some("stored lyrics".to_string()),
            lyrics_offset_ms: 500,
        }
    }

    #[test]
    fn init_sets_schema_version_and_enables_foreign_keys() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;

        let version: i64 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        let foreign_keys: bool = db
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))?;

        assert_eq!(version, SCHEMA_VERSION);
        assert!(foreign_keys);
        Ok(())
    }

    #[test]
    fn rescan_upsert_preserves_user_state_and_database_lyrics() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.insert_track(&track("/music/song.mp3"))?;

        let mut rescanned = track("/music/song.mp3");
        rescanned.title = "Retagged".to_string();
        rescanned.favorite = false;
        rescanned.play_count = 0;
        rescanned.last_played = None;
        rescanned.lyrics = None;
        rescanned.lyrics_offset_ms = 0;
        rescanned.duration_secs = 181;
        db.insert_track(&rescanned)?;

        let saved = db.get_all_tracks()?.pop().expect("saved track");
        assert_eq!(saved.title, "Retagged");
        assert_eq!(saved.duration_secs, 181);
        assert!(saved.favorite);
        assert_eq!(saved.play_count, 7);
        assert_eq!(saved.last_played, Some(123));
        assert_eq!(saved.lyrics.as_deref(), Some("stored lyrics"));
        assert_eq!(saved.lyrics_offset_ms, 500);
        Ok(())
    }

    #[test]
    fn tracks_with_matching_tags_remain_distinct() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.insert_track(&track("/music/first.mp3"))?;
        db.insert_track(&track("/music/second.flac"))?;

        let tracks = db.get_all_tracks()?;
        assert_eq!(tracks.len(), 2);
        Ok(())
    }

    #[test]
    fn artist_collections_split_collaborations_and_match_their_tracks() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;

        let mut travis = track("/music/travis.mp3");
        travis.artist = "Travis Scott".into();
        let mut don = track("/music/don.mp3");
        don.artist = "Don Toliver".into();
        let mut collaboration = track("/music/collaboration.mp3");
        collaboration.artist = "TRAVIS SCOTT, Don Toliver, Don Toliver".into();
        db.insert_track(&travis)?;
        db.insert_track(&don)?;
        db.insert_track(&collaboration)?;

        assert_eq!(db.get_artists()?, ["Don Toliver", "Travis Scott"]);
        let don_tracks = db.get_tracks_by_artist("Don Toliver")?;
        assert_eq!(don_tracks.len(), 2);
        assert!(
            don_tracks
                .iter()
                .any(|track| track.path == collaboration.path)
        );
        Ok(())
    }

    #[test]
    fn albums_hide_redundant_single_release_titles() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;

        let mut redundant_single = track("/music/single.mp3");
        redundant_single.title = "Standalone Single".into();
        redundant_single.album = "Standalone Single".into();
        let mut album_track = track("/music/album-track.mp3");
        album_track.title = "Song".into();
        album_track.album = "Real Album".into();
        db.insert_track(&redundant_single)?;
        db.insert_track(&album_track)?;

        assert_eq!(db.get_albums()?, ["Real Album"]);
        Ok(())
    }

    #[test]
    fn playlist_summaries_include_empty_playlists() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.create_playlist("Empty")?;

        assert_eq!(
            db.get_playlist_summaries()?,
            vec![PlaylistSummary {
                name: "Empty".to_string(),
                track_count: 0,
                duration_secs: 0,
                representative_track: None,
            }]
        );
        Ok(())
    }

    #[test]
    fn playlist_summaries_count_tracks_and_sum_duration() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.create_playlist("Favorites")?;

        let mut first = track("/music/first.mp3");
        first.duration_secs = 61;
        let mut second = track("/music/second.mp3");
        second.duration_secs = 119;
        db.insert_track(&first)?;
        db.insert_track(&second)?;
        db.add_track_to_playlist("Favorites", &first.path)?;
        db.add_track_to_playlist("Favorites", &second.path)?;

        let summary = db.get_playlist_summaries()?.pop().expect("summary");
        assert_eq!(summary.track_count, 2);
        assert_eq!(summary.duration_secs, 180);
        Ok(())
    }

    #[test]
    fn playlist_summary_representative_is_lowest_track_path() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.create_playlist("Mixed")?;

        let mut later = track("/music/z-last.mp3");
        later.title = "Inserted First".to_string();
        let mut representative = track("/music/a-first.mp3");
        representative.title = "Inserted Last".to_string();
        db.insert_track(&later)?;
        db.insert_track(&representative)?;
        db.add_track_to_playlist("Mixed", &later.path)?;
        db.add_track_to_playlist("Mixed", &representative.path)?;

        let summary = db.get_playlist_summaries()?.pop().expect("summary");
        assert_eq!(summary.representative_track, Some(representative));
        Ok(())
    }

    #[test]
    fn playlist_summaries_are_ordered_by_name() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.init()?;
        db.create_playlist("Zulu")?;
        db.create_playlist("Alpha")?;
        db.create_playlist("Middle")?;

        let names: Vec<_> = db
            .get_playlist_summaries()?
            .into_iter()
            .map(|summary| summary.name)
            .collect();
        assert_eq!(names, ["Alpha", "Middle", "Zulu"]);
        Ok(())
    }

    #[test]
    fn init_migrates_legacy_track_schema() -> Result<()> {
        let mut db = Db::new(":memory:")?;
        db.conn.execute(
            "CREATE TABLE tracks (
                path TEXT,
                title TEXT,
                artist TEXT,
                album TEXT
            )",
            [],
        )?;
        db.conn.execute(
            "INSERT INTO tracks (path, title, artist, album)
             VALUES ('/music/song.mp3', 'Song', 'Artist', 'Album')",
            [],
        )?;

        db.init()?;

        let migrated = db.get_all_tracks()?.pop().expect("migrated track");
        assert_eq!(migrated.path, "/music/song.mp3");
        assert_eq!(migrated.genre, "Unknown");
        assert_eq!(migrated.duration_secs, 0);
        Ok(())
    }
}
