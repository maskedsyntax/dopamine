package ui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/maskedsyntax/dopamine/internal/audio"
	"github.com/maskedsyntax/dopamine/internal/library"
)

type ViewMode int

const (
	HomeView ViewMode = iota
	ArtistView
	AlbumView
	PlaylistView
)

type Model struct {
	width  int
	height int
	styles Styles
	mode   ViewMode
	
	audioEngine *audio.Engine
	db          *library.DB
	tracks      []library.Track
	cursor      int
	current     *library.Track
	
	scanning bool
	err      error
}

func (m Model) Init() tea.Cmd {
	return tea.Tick(time.Second/10, func(t time.Time) tea.Msg {
		return TickMsg(t)
	})
}

type TickMsg time.Time
type ScanCompleteMsg []library.Track

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "up", "k":
			if m.cursor > 0 {
				m.cursor--
			}
		case "down", "j":
			if m.cursor < len(m.tracks)-1 {
				m.cursor++
			}
		case "s":
			if !m.scanning {
				m.scanning = true
				return m, func() tea.Msg {
					scanner := library.NewScanner(m.db)
					home, _ := os.UserHomeDir()
					musicDir := filepath.Join(home, "Music")
					scanner.ScanDirectory(musicDir)
					tracks, _ := m.db.GetAllTracks()
					return ScanCompleteMsg(tracks)
				}
			}
		case "enter":
			if len(m.tracks) > 0 && m.audioEngine != nil {
				track := m.tracks[m.cursor]
				m.current = &track
				m.audioEngine.PlayFile(track.Path)
			}
		case " ":
			if m.audioEngine != nil {
				m.audioEngine.TogglePause()
			}
		case "1":
			m.mode = HomeView
		case "2":
			m.mode = ArtistView
		case "3":
			m.mode = AlbumView
		case "4":
			m.mode = PlaylistView
		}
	case ScanCompleteMsg:
		m.scanning = false
		m.tracks = msg
		return m, nil
	case TickMsg:
		return m, tea.Tick(time.Second/10, func(t time.Time) tea.Msg {
			return TickMsg(t)
		})
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
	}
	return m, nil
}

func (m Model) View() string {
	if m.width == 0 || m.height == 0 {
		return "Initializing..."
	}

	sidebarHeight := m.height - 3
	
	sidebar := m.styles.Sidebar.
		Height(sidebarHeight).
		Render(m.renderSidebar())

	mainViewWidth := m.width - 25 - 4
	mainView := m.styles.MainView.
		Width(mainViewWidth).
		Height(sidebarHeight).
		Render(m.renderMainView())

	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainView)
	
	playerBar := m.styles.PlayerBar.
		Width(m.width - 4).
		Render(m.renderPlayerBar())

	return lipgloss.JoinVertical(lipgloss.Left, content, playerBar)
}

func (m Model) renderSidebar() string {
	var b strings.Builder
	b.WriteString(m.styles.Title.Render("DOPAMINE"))
	b.WriteString("\n\n")
	
	items := []struct {
		icon string
		name string
		mode ViewMode
	}{
		{"", "Home", HomeView},
		{"󰠃", "Artists", ArtistView},
		{"󰀥", "Albums", AlbumView},
		{"󰲸", "Playlists", PlaylistView},
	}

	for _, item := range items {
		style := lipgloss.NewStyle()
		if m.mode == item.mode {
			style = m.styles.ActiveItem
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", item.icon, item.name)))
		b.WriteString("\n")
	}

	b.WriteString("\n\n")
	b.WriteString(" Help (?)")
	return b.String()
}

func (m Model) renderMainView() string {
	if m.scanning {
		return "Scanning library... please wait."
	}

	if len(m.tracks) == 0 {
		return "No tracks found. Press 's' to scan your Music folder."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("All Tracks"))
	b.WriteString("\n\n")

	for i, track := range m.tracks {
		cursor := " "
		style := lipgloss.NewStyle()
		if i == m.cursor {
			cursor = ">"
			style = m.styles.ActiveItem
		}
		
		artist := track.Artist
		if artist == "" {
			artist = "Unknown Artist"
		}
		
		line := fmt.Sprintf("%s %-30s | %s", cursor, truncate(track.Title, 30), artist)
		b.WriteString(style.Render(line))
		b.WriteString("\n")
	}

	return b.String()
}

func (m Model) renderPlayerBar() string {
	status := "󰐊 Play"
	if m.audioEngine != nil && m.audioEngine.Ctrl != nil && !m.audioEngine.Ctrl.Paused {
		status = "󰏤 Pause"
	}
	
	trackInfo := "No track playing"
	if m.current != nil {
		artist := m.current.Artist
		if artist == "" {
			artist = "Unknown"
		}
		trackInfo = fmt.Sprintf("%s - %s", m.current.Title, artist)
	}

	// Simple visualizer mock
	visualizer := " ▂▃▅▆▇"
	if m.audioEngine != nil && m.audioEngine.Ctrl != nil && !m.audioEngine.Ctrl.Paused {
		// randomize visualizer slightly based on time?
		// for now just static
	} else {
		visualizer = "      "
	}

	return fmt.Sprintf("%s  %s  %s | 󰒭 Prev  󰒮 Next  󰓃 Volume 80%%", visualizer, status, trackInfo)
}

func truncate(s string, l int) string {
	if len(s) > l {
		return s[:l-3] + "..."
	}
	return s
}
