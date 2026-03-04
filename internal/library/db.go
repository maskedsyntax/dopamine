package library

import (
	"database/sql"
	_ "modernc.org/sqlite"
)

type DB struct {
	conn *sql.DB
}

type Track struct {
	ID       int
	Path     string
	Title    string
	Artist   string
	Album    string
	Duration int
}

func NewDB(path string) (*DB, error) {
	conn, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}

	_, err = conn.Exec(`
		CREATE TABLE IF NOT EXISTS tracks (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			path TEXT UNIQUE,
			title TEXT,
			artist TEXT,
			album TEXT,
			duration INTEGER
		);
		CREATE TABLE IF NOT EXISTS playlists (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			name TEXT UNIQUE
		);
		CREATE TABLE IF NOT EXISTS playlist_tracks (
			playlist_id INTEGER,
			track_id INTEGER,
			FOREIGN KEY(playlist_id) REFERENCES playlists(id),
			FOREIGN KEY(track_id) REFERENCES tracks(id),
			PRIMARY KEY(playlist_id, track_id)
		);
	`)
	if err != nil {
		return nil, err
	}

	return &DB{conn: conn}, nil
}

func (db *DB) ClearTracks() error {
	_, err := db.conn.Exec("DELETE FROM tracks")
	return err
}

func (db *DB) AddTrack(t Track) error {
	_, err := db.conn.Exec(`
		INSERT OR REPLACE INTO tracks (path, title, artist, album, duration)
		VALUES (?, ?, ?, ?, ?)
	`, t.Path, t.Title, t.Artist, t.Album, t.Duration)
	return err
}

func (db *DB) GetAllTracks() ([]Track, error) {
	rows, err := db.conn.Query("SELECT id, path, title, artist, album, duration FROM tracks")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tracks []Track
	for rows.Next() {
		var t Track
		err := rows.Scan(&t.ID, &t.Path, &t.Title, &t.Artist, &t.Album, &t.Duration)
		if err != nil {
			return nil, err
		}
		tracks = append(tracks, t)
	}
	return tracks, nil
}

func (db *DB) GetArtists() ([]string, error) {
	rows, err := db.conn.Query("SELECT DISTINCT artist FROM tracks WHERE artist != '' ORDER BY artist ASC")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var artists []string
	for rows.Next() {
		var a string
		if err := rows.Scan(&a); err == nil {
			artists = append(artists, a)
		}
	}
	return artists, nil
}

func (db *DB) GetAlbums() ([]string, error) {
	rows, err := db.conn.Query("SELECT DISTINCT album FROM tracks WHERE album != '' ORDER BY album ASC")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var albums []string
	for rows.Next() {
		var a string
		if err := rows.Scan(&a); err == nil {
			albums = append(albums, a)
		}
	}
	return albums, nil
}

func (db *DB) CreatePlaylist(name string) error {
	_, err := db.conn.Exec("INSERT OR IGNORE INTO playlists (name) VALUES (?)", name)
	return err
}

func (db *DB) AddTrackToPlaylist(playlistName string, trackPath string) error {
	var playlistID, trackID int
	err := db.conn.QueryRow("SELECT id FROM playlists WHERE name = ?", playlistName).Scan(&playlistID)
	if err != nil {
		return err
	}
	err = db.conn.QueryRow("SELECT id FROM tracks WHERE path = ?", trackPath).Scan(&trackID)
	if err != nil {
		return err
	}
	_, err = db.conn.Exec("INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id) VALUES (?, ?)", playlistID, trackID)
	return err
}

func (db *DB) GetPlaylistTracks(name string) ([]Track, error) {
	rows, err := db.conn.Query(`
		SELECT t.id, t.path, t.title, t.artist, t.album, t.duration 
		FROM tracks t
		JOIN playlist_tracks pt ON t.id = pt.track_id
		JOIN playlists p ON p.id = pt.playlist_id
		WHERE p.name = ?
	`, name)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tracks []Track
	for rows.Next() {
		var t Track
		err := rows.Scan(&t.ID, &t.Path, &t.Title, &t.Artist, &t.Album, &t.Duration)
		if err != nil {
			return nil, err
		}
		tracks = append(tracks, t)
	}
	return tracks, nil
}

func (db *DB) GetPlaylists() ([]string, error) {
	rows, err := db.conn.Query("SELECT name FROM playlists ORDER BY name ASC")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var p []string
	for rows.Next() {
		var n string
		if err := rows.Scan(&n); err == nil {
			p = append(p, n)
		}
	}
	return p, nil
}
