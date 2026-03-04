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

	m, err := tag.ReadFrom(f)
	if err != nil {
		return Track{
			Path:  path,
			Title: filepath.Base(path),
		}, nil
	}

	return Track{
		Path:   path,
		Title:  m.Title(),
		Artist: m.Artist(),
		Album:  m.Album(),
		// Duration would need another library or more complex tag reading
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
