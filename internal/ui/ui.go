package ui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/maskedsyntax/dopamine/internal/audio"
	"github.com/maskedsyntax/dopamine/internal/library"
	"github.com/sahilm/fuzzy"
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
	artists     []string
	albums      []string
	
	// Search state
	searchInput    textinput.Model
	isSearching    bool
	filteredTracks []library.Track
	
	cursor      int
	topIndex    int // For scrolling
	current     *library.Track
	
	// Player state
	playingIndex int
	queue        []library.Track

	scanning bool
	showHelp bool
	err      error
}

func (m Model) Init() tea.Cmd {
	return tea.Batch(
		tea.Tick(time.Second/1, func(t time.Time) tea.Msg {
			return TickMsg(t)
		}),
		textinput.Blink,
	)
}

type TickMsg time.Time
type ScanCompleteMsg struct {
	tracks  []library.Track
	artists []string
	albums  []string
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd

	if m.isSearching {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			switch msg.String() {
			case "enter", "esc":
				m.isSearching = false
				m.searchInput.Blur()
				return m, nil
			}
		}
		m.searchInput, cmd = m.searchInput.Update(msg)
		m.filterTracks()
		return m, cmd
	}

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
		case "/":
			m.isSearching = true
			m.searchInput.Focus()
			m.cursor = 0
			m.topIndex = 0
			return m, textinput.Blink
		case "up", "k":
			if m.cursor > 0 {
				m.cursor--
				if m.cursor < m.topIndex {
					m.topIndex = m.cursor
				}
			}
		case "down", "j":
			count := m.getItemCount()
			if m.cursor < count-1 {
				m.cursor++
				maxVisible := m.height - 3 - 5
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
					artists, _ := m.db.GetArtists()
					albums, _ := m.db.GetAlbums()
					return ScanCompleteMsg{tracks, artists, albums}
				}
			}
		case "enter":
			targetTracks := m.tracks
			if len(m.filteredTracks) > 0 || m.searchInput.Value() != "" {
				targetTracks = m.filteredTracks
			}

			if m.mode == HomeView && len(targetTracks) > 0 {
				m.queue = targetTracks
				m.playingIndex = m.cursor
				track := m.queue[m.playingIndex]
				m.current = &track
				m.audioEngine.PlayFile(track.Path)
			}
		case "n": // Next
			if len(m.queue) > 0 && m.playingIndex < len(m.queue)-1 {
				m.playingIndex++
				track := m.queue[m.playingIndex]
				m.current = &track
				m.audioEngine.PlayFile(track.Path)
			}
		case "p": // Previous
			if len(m.queue) > 0 && m.playingIndex > 0 {
				m.playingIndex--
				track := m.queue[m.playingIndex]
				m.current = &track
				m.audioEngine.PlayFile(track.Path)
			}
		case " ":
			if m.audioEngine != nil {
				m.audioEngine.TogglePause()
			}
		case "1":
			m.mode = HomeView
			m.cursor = 0
			m.topIndex = 0
			m.filteredTracks = nil
		case "2":
			m.mode = ArtistView
			m.cursor = 0
			m.topIndex = 0
		case "3":
			m.mode = AlbumView
			m.cursor = 0
			m.topIndex = 0
		case "4":
			m.mode = PlaylistView
			m.cursor = 0
			m.topIndex = 0
		}
	case ScanCompleteMsg:
		m.scanning = false
		m.tracks = msg.tracks
		m.artists = msg.artists
		m.albums = msg.albums
		return m, nil
	case TickMsg:
		// Auto-advance if track finished? 
		// Beep doesn't easily tell us when a track ends without wrapping it in a custom streamer
		return m, tea.Tick(time.Second/1, func(t time.Time) tea.Msg {
			return TickMsg(t)
		})
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
	}
	return m, nil
}

func (m *Model) filterTracks() {
	query := m.searchInput.Value()
	if query == "" {
		m.filteredTracks = nil
		return
	}

	var targets []string
	for _, t := range m.tracks {
		targets = append(targets, fmt.Sprintf("%s %s %s", t.Title, t.Artist, t.Album))
	}

	matches := fuzzy.Find(query, targets)
	m.filteredTracks = make([]library.Track, len(matches))
	for i, match := range matches {
		m.filteredTracks[i] = m.tracks[match.Index]
	}
	
	if m.cursor >= len(m.filteredTracks) {
		m.cursor = 0
		m.topIndex = 0
	}
}

func (m Model) getItemCount() int {
	if m.mode == HomeView && (len(m.filteredTracks) > 0 || m.searchInput.Value() != "") {
		return len(m.filteredTracks)
	}
	switch m.mode {
	case HomeView:
		return len(m.tracks)
	case ArtistView:
		return len(m.artists)
	case AlbumView:
		return len(m.albums)
	default:
		return 0
	}
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
  Enter        : Play selected track (Home view)
  Space        : Pause/Resume
  n            : Next track
  p            : Previous track
  /            : Search tracks
  s            : Scan Music folder
  1, 2, 3, 4   : Switch views (Home, Artists, Albums, Playlists)
  ?            : Toggle help
  q / Ctrl+C   : Quit

  SEARCH
  Type to filter tracks. Press Enter or Esc to finish searching.

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

	var searchBar string
	if m.isSearching || m.searchInput.Value() != "" {
		searchBar = m.styles.ActiveItem.Render(" ") + m.searchInput.View() + "\n\n"
	}

	switch m.mode {
	case HomeView:
		return searchBar + m.renderTracks()
	case ArtistView:
		return m.renderArtists()
	case AlbumView:
		return m.renderAlbums()
	case PlaylistView:
		return "Playlists view coming soon..."
	default:
		return ""
	}
}

func (m Model) renderTracks() string {
	tracks := m.tracks
	title := "All Tracks"
	
	if len(m.filteredTracks) > 0 || m.searchInput.Value() != "" {
		tracks = m.filteredTracks
		title = fmt.Sprintf("Search Results (%d)", len(tracks))
	}

	if len(tracks) == 0 {
		if m.searchInput.Value() != "" {
			return "No matches found."
		}
		return "No tracks found.\n\nPress 's' to scan your Music folder.\nPress '?' for help."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render(title))
	b.WriteString("\n\n")

	offset := 5
	if m.isSearching || m.searchInput.Value() != "" {
		offset = 7
	}
	maxVisible := m.height - 3 - offset
	if maxVisible <= 0 { return "Terminal too small" }

	endIndex := m.topIndex + maxVisible
	if endIndex > len(tracks) {
		endIndex = len(tracks)
	}

	for i := m.topIndex; i < endIndex; i++ {
		track := tracks[i]
		cursor := " "
		style := lipgloss.NewStyle()
		if i == m.cursor {
			cursor = ">"
			style = m.styles.ActiveItem
		}
		artist := track.Artist
		titleWidth := m.width - 25 - 4 - 2 - 3 - 25
		line := fmt.Sprintf("%s %-*s | %s", cursor, titleWidth, truncate(track.Title, titleWidth), truncate(artist, 20))
		b.WriteString(style.Render(line))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderArtists() string {
	if len(m.artists) == 0 {
		return "No artists found. Scan your library first."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Artists"))
	b.WriteString("\n\n")

	maxVisible := m.height - 3 - 5
	endIndex := m.topIndex + maxVisible
	if endIndex > len(m.artists) {
		endIndex = len(m.artists)
	}

	for i := m.topIndex; i < endIndex; i++ {
		cursor := " "
		style := lipgloss.NewStyle()
		if i == m.cursor {
			cursor = ">"
			style = m.styles.ActiveItem
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, m.artists[i])))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderAlbums() string {
	if len(m.albums) == 0 {
		return "No albums found. Scan your library first."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Albums"))
	b.WriteString("\n\n")

	maxVisible := m.height - 3 - 5
	endIndex := m.topIndex + maxVisible
	if endIndex > len(m.albums) {
		endIndex = len(m.albums)
	}

	for i := m.topIndex; i < endIndex; i++ {
		cursor := " "
		style := lipgloss.NewStyle()
		if i == m.cursor {
			cursor = ">"
			style = m.styles.ActiveItem
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, m.albums[i])))
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
	progress := 0.0
	timeInfo := "00:00 / 00:00"

	if m.current != nil && m.audioEngine != nil && m.audioEngine.Streamer != nil {
		trackInfo = fmt.Sprintf("%s - %s", m.current.Title, m.current.Artist)
		
		pos := m.audioEngine.Streamer.Position()
		len := m.audioEngine.Streamer.Len()
		if len > 0 {
			progress = float64(pos) / float64(len)
		}
		
		sr := m.audioEngine.SampleRate
		currentTime := time.Duration(pos) * time.Second / time.Duration(sr)
		totalTime := time.Duration(len) * time.Second / time.Duration(sr)
		timeInfo = fmt.Sprintf("%02d:%02d / %02d:%02d", 
			int(currentTime.Minutes()), int(currentTime.Seconds())%60,
			int(totalTime.Minutes()), int(totalTime.Seconds())%60)
	}

	width := m.width - 4
	barWidth := 20
	filled := int(float64(barWidth) * progress)
	if filled > barWidth { filled = barWidth }
	bar := strings.Repeat("█", filled) + strings.Repeat("░", barWidth-filled)

	visualizer := " ▂▃▅▆▇"
	if m.audioEngine != nil && m.audioEngine.Ctrl != nil && !m.audioEngine.Ctrl.Paused {
		// Mock randomization
	} else {
		visualizer = "      "
	}

	return fmt.Sprintf("%s %s %s [%s] %s | 󰒭 Prev  󰒮 Next  󰓃 Vol", 
		visualizer, status, truncate(trackInfo, width-60), bar, timeInfo)
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
