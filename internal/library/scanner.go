package library

import (
	"os"
	"path/filepath"
	"strings"
	"os/exec"
	"encoding/json"

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

	// 1. Try our manual WAV parser for RIFF INFO
	if ext == ".wav" {
		t, a, al, err := ExtractWavMetadata(path)
		if err == nil {
			if t != "" { title = t }
			if a != "" { artist = a }
			if al != "" { album = al }
		}
	}

	// 2. Try FFPROBE as a high-reliability fallback if tags are still missing or known-generic
	if artist == defaultArtist || artist == "Unknown Artist" || title == fileName {
		t, a, al, err := extractWithFFProbe(path)
		if err == nil {
			if t != "" { title = t }
			if a != "" { artist = a }
			if al != "" { album = al }
		}
	}

	// 3. Try standard Go libraries if we still don't have good data
	if artist == "Unknown Artist" || artist == defaultArtist {
		if ext == ".mp3" {
			t, err := id3v2.Open(path, id3v2.Options{Parse: true})
			if err == nil {
				defer t.Close()
				if ti := t.Title(); ti != "" { title = ti }
				if a := t.Artist(); a != "" { artist = a }
				if al := t.Album(); al != "" { album = al }
			}
		}

		f, err := os.Open(path)
		if err == nil {
			m, err := tag.ReadFrom(f)
			if err == nil {
				if t := m.Title(); t != "" { title = t }
				if a := m.Artist(); a != "" { artist = a } else if aa := m.AlbumArtist(); aa != "" { artist = aa }
				if al := m.Album(); al != "" { album = al }
			}
			f.Close()
		}
	}

	// 4. Cleanup and heuristics
	if artist == "" || artist == "Unknown Artist" { artist = defaultArtist }
	if title == "" { title = fileName }
	
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

func extractWithFFProbe(path string) (title, artist, album string, err error) {
	cmd := exec.Command("ffprobe", "-v", "quiet", "-show_format", "-show_streams", "-print_format", "json", path)
	out, err := cmd.Output()
	if err != nil {
		return "", "", "", err
	}

	var data struct {
		Format struct {
			Tags map[string]string `json:"tags"`
		} `json:"format"`
	}

	if err := json.Unmarshal(out, &data); err != nil {
		return "", "", "", err
	}

	tags := data.Format.Tags
	title = tags["title"]
	artist = tags["artist"]
	if artist == "" {
		artist = tags["album_artist"]
	}
	album = tags["album"]

	return title, artist, album, nil
}

func isAudioFile(path string) bool {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".mp3", ".flac", ".ogg", ".wav", ".m4a":
		return true
	}
	return false
}
