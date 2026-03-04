package ui

import (
	"os"
	"path/filepath"

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

	return Model{
		styles:      GetStyles(DefaultTheme),
		mode:        HomeView,
		audioEngine: engine,
		db:          db,
		tracks:      tracks,
	}, nil
}
