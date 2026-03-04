package library

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/dhowden/tag"
)

type Scanner struct {
	db *DB
}

func NewScanner(db *DB) *Scanner {
	return &Scanner{db: db}
}

func (s *Scanner) ScanDirectory(root string) error {
	return filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}

		if isAudioFile(path) {
			track, err := s.extractMetadata(path)
			if err != nil {
				// Log error but continue scanning
				return nil
			}
			return s.db.AddTrack(track)
		}
		return nil
	})
}

func (s *Scanner) extractMetadata(path string) (Track, error) {
	f, err := os.Open(path)
	if err != nil {
		return Track{}, err
	}
	defer f.Close()

	title := filepath.Base(path)
	artist := "Unknown Artist"
	album := "Unknown Album"

	m, err := tag.ReadFrom(f)
	if err == nil {
		if t := m.Title(); t != "" {
			title = t
		}
		if a := m.Artist(); a != "" {
			artist = a
		}
		if al := m.Album(); al != "" {
			album = al
		}
	}

	return Track{
		Path:   path,
		Title:  title,
		Artist: artist,
		Album:  album,
	}, nil
}

func isAudioFile(path string) bool {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".mp3", ".flac", ".ogg", ".wav", ".m4a":
		return true
	}
	return false
}
