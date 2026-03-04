package library

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/bogem/id3v2/v2"
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
				// We still add the track with at least its path/filename
				return s.db.AddTrack(Track{
					Path:   path,
					Title:  filepath.Base(path),
					Artist: "Unknown Artist",
					Album:  "Unknown Album",
				})
			}
			return s.db.AddTrack(track)
		}
		return nil
	})
}

func (s *Scanner) extractMetadata(path string) (Track, error) {
	ext := strings.ToLower(filepath.Ext(path))
	
	title := filepath.Base(path)
	artist := "Unknown Artist"
	album := "Unknown Album"

	// Special handling for MP3s which often have complex ID3 tags
	if ext == ".mp3" {
		t, err := id3v2.Open(path, id3v2.Options{Parse: true})
		if err == nil {
			defer t.Close()
			if ti := t.Title(); ti != "" {
				title = ti
			}
			if a := t.Artist(); a != "" {
				artist = a
			}
			if al := t.Album(); al != "" {
				album = al
			}
			return Track{
				Path:   path,
				Title:  title,
				Artist: artist,
				Album:  album,
			}, nil
		}
	}

	// Fallback to dhowden/tag for other formats or if id3v2 fails
	f, err := os.Open(path)
	if err != nil {
		return Track{Path: path, Title: title, Artist: artist, Album: album}, err
	}
	defer f.Close()

	m, err := tag.ReadFrom(f)
	if err == nil {
		if t := m.Title(); t != "" {
			title = t
		}
		if a := m.Artist(); a != "" {
			artist = a
		} else if aa := m.AlbumArtist(); aa != "" {
			artist = aa
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
