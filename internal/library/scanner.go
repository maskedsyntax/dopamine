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
			// If folder is named "Liked Music" or similar, we could treat it as a playlist
			// For now, let's stick to standard indexing
			return nil
		}

		if isAudioFile(path) {
			track, err := s.extractMetadata(path)
			if err != nil {
				// Final fallback
				fileName := filepath.Base(path)
				fileName = strings.TrimSuffix(fileName, filepath.Ext(fileName))
				return s.db.AddTrack(Track{
					Path:   path,
					Title:  fileName,
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
	
	fileName := filepath.Base(path)
	fileName = strings.TrimSuffix(fileName, ext)
	
	// Default values derived from path
	parentDir := filepath.Base(filepath.Dir(path))
	defaultArtist := "Unknown Artist"
	if parentDir != "Music" && parentDir != "Liked Music" && parentDir != "Downloads" && parentDir != "." {
		defaultArtist = parentDir
	}

	title := fileName
	artist := defaultArtist
	album := "Unknown Album"

	// Special handling for WAV using our manual parser
	if ext == ".wav" {
		t, a, al, err := ExtractWavMetadata(path)
		if err == nil {
			if t != "" { title = t }
			if a != "" { artist = a }
			if al != "" { album = al }
			return Track{
				Path: path,
				Title: title,
				Artist: artist,
				Album: album,
			}, nil
		}
	}

	// Special handling for MP3s
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

	// Fallback to dhowden/tag
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

	// Filename dash heuristic
	if (artist == "Unknown Artist" || artist == defaultArtist) && strings.Contains(fileName, " - ") {
		parts := strings.SplitN(fileName, " - ", 2)
		artist = strings.TrimSpace(parts[0])
		title = strings.TrimSpace(parts[1])
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
