package ui

import (
	"os"
	"path/filepath"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/maskedsyntax/dopamine/internal/audio"
	"github.com/maskedsyntax/dopamine/internal/library"
)

type Config struct {
	MusicDir string
	DBPath   string
}

func LoadConfig() Config {
	home, _ := os.UserHomeDir()
	configDir := filepath.Join(home, ".config", "dopamine")
	os.MkdirAll(configDir, 0755)

	return Config{
		MusicDir: filepath.Join(home, "Music"),
		DBPath:   filepath.Join(configDir, "library.db"),
	}
}

func InitialModelWithDeps(cfg Config) (Model, error) {
	db, err := library.NewDB(cfg.DBPath)
	if err != nil {
		return Model{}, err
	}

	engine, err := audio.NewEngine()
	if err != nil {
		return Model{}, err
	}

	tracks, _ := db.GetAllTracks()
	artists, _ := db.GetArtists()
	albums, _ := db.GetAlbums()

	ti := textinput.New()
	ti.Placeholder = "Search tracks..."
	ti.CharLimit = 156
	ti.Width = 20

	return Model{
		styles:      GetStyles(DefaultTheme),
		mode:        HomeView,
		audioEngine: engine,
		db:          db,
		tracks:      tracks,
		artists:     artists,
		albums:      albums,
		searchInput: ti,
	}, nil
}
