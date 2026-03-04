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
	
	// Initial empty values
	var title, artist, album string

	// 1. Try FFPROBE first as it is the most reliable
	title, artist, album, _ = extractWithFFProbe(path)

	// 2. Try our manual WAV parser for RIFF INFO if still missing
	if ext == ".wav" && (title == "" || artist == "") {
		t, a, al, err := ExtractWavMetadata(path)
		if err == nil {
			if title == "" { title = t }
			if artist == "" { artist = a }
			if album == "" { album = al }
		}
	}

	// 3. Try standard Go libraries if still missing
	if title == "" || artist == "" {
		if ext == ".mp3" {
			t, err := id3v2.Open(path, id3v2.Options{Parse: true})
			if err == nil {
				defer t.Close()
				if title == "" { title = t.Title() }
				if artist == "" { artist = t.Artist() }
				if album == "" { album = t.Album() }
			}
		}

		f, err := os.Open(path)
		if err == nil {
			m, err := tag.ReadFrom(f)
			if err == nil {
				if title == "" { title = m.Title() }
				if artist == "" { 
					artist = m.Artist()
					if artist == "" { artist = m.AlbumArtist() }
				}
				if album == "" { album = m.Album() }
			}
			f.Close()
		}
	}

	// 4. Heuristics only as a last resort
	if artist == "" || artist == "Unknown Artist" {
		// Try filename dash heuristic
		if strings.Contains(fileName, " - ") {
			parts := strings.SplitN(fileName, " - ", 2)
			artist = strings.TrimSpace(parts[0])
			title = strings.TrimSpace(parts[1])
		} else {
			// Try folder name as artist ONLY if it looks like a real name
			parentDir := filepath.Base(filepath.Dir(path))
			if parentDir != "Music" && parentDir != "Liked Music" && parentDir != "Downloads" && parentDir != "." && parentDir != "new" && parentDir != "check" {
				artist = parentDir
			} else {
				artist = "Unknown Artist"
			}
		}
	}

	if title == "" {
		title = fileName
	}
	if album == "" {
		album = "Unknown Album"
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
	// ffprobe tags can be lowercase or uppercase depending on encoder
	for k, v := range tags {
		lowerK := strings.ToLower(k)
		switch lowerK {
		case "title":
			if title == "" { title = v }
		case "artist":
			if artist == "" { artist = v }
		case "album":
			if album == "" { album = v }
		case "album_artist":
			if artist == "" { artist = v }
		}
	}

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
