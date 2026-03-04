package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/maskedsyntax/dopamine/internal/ui"
)

func main() {
	cfg := ui.LoadConfig()
	m, err := ui.InitialModelWithDeps(cfg)
	if err != nil {
		fmt.Printf("Error initializing model: %v", err)
		os.Exit(1)
	}

	p := tea.NewProgram(m, tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		fmt.Printf("Error running program: %v", err)
		os.Exit(1)
	}
}
