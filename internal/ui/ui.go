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
	topIndex    int // For scrolling
	current     *library.Track

	scanning bool
	showHelp bool
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
		if m.showHelp {
			m.showHelp = false
			return m, nil
		}

		switch msg.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "?":
			m.showHelp = true
		case "up", "k":
			if m.cursor > 0 {
				m.cursor--
				if m.cursor < m.topIndex {
					m.topIndex = m.cursor
				}
			}
		case "down", "j":
			if m.cursor < len(m.tracks)-1 {
				m.cursor++
				maxVisible := m.height - 3 - 5 // 3 for player, 5 for header/padding
				if m.cursor >= m.topIndex+maxVisible {
					m.topIndex = m.cursor - maxVisible + 1
				}
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

	if m.showHelp {
		return m.renderHelp()
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

func (m Model) renderHelp() string {
	help := `
  DOPAMINE HELP
  =============

  NAVIGATION
  k / ↑        : Move up
  j / ↓        : Move down
  Enter        : Play selected track
  Space        : Pause/Resume
  s            : Scan Music folder
  1, 2, 3, 4   : Switch views (Home, Artists, Albums, Playlists)
  ?            : Toggle help
  q / Ctrl+C   : Quit

  Press any key to return...
`
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, 
		m.styles.MainView.BorderStyle(lipgloss.RoundedBorder()).Render(help))
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
		return "No tracks found.\n\nPress 's' to scan your Music folder.\nPress '?' for help."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("All Tracks"))
	b.WriteString("\n\n")

	maxVisible := m.height - 3 - 5 // sidebarHeight - title - padding
	if maxVisible <= 0 {
		return "Terminal too small"
	}

	endIndex := m.topIndex + maxVisible
	if endIndex > len(m.tracks) {
		endIndex = len(m.tracks)
	}

	for i := m.topIndex; i < endIndex; i++ {
		track := m.tracks[i]
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

		// Calculate available width for title
		// Total width - sidebar(25) - padding(4) - cursor(2) - separator(3) - artist(20)
		titleWidth := m.width - 25 - 4 - 2 - 3 - 20
		if titleWidth < 10 {
			titleWidth = 10
		}

		line := fmt.Sprintf("%s %-*s | %s", cursor, titleWidth, truncate(track.Title, titleWidth), truncate(artist, 20))
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
	} else {
		visualizer = "      "
	}

	return fmt.Sprintf("%s  %s  %s | 󰒭 Prev  󰒮 Next  󰓃 Volume 80%%", visualizer, status, truncate(trackInfo, m.width-40))
}

func truncate(s string, l int) string {
	if len(s) > l {
		if l > 3 {
			return s[:l-3] + "..."
		}
		return s[:l]
	}
	return s
}
