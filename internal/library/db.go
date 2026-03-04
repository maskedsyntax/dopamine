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
	`)
	if err != nil {
		return nil, err
	}

	return &DB{conn: conn}, nil
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
