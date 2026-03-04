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
	playlists   []string
	
	// Search state
	searchInput      textinput.Model
	isSearching      bool
	filteredTracks   []library.Track
	filteredArtists  []string
	filteredAlbums   []string
	filteredPlaylists []string
	
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
		tea.Tick(time.Millisecond*50, func(t time.Time) tea.Msg {
			return TickMsg(t)
		}),
		textinput.Blink,
	)
}

type TickMsg time.Time
type ScanCompleteMsg struct {
	tracks    []library.Track
	artists   []string
	albums    []string
	playlists []string
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd
	var tickCmd tea.Cmd

	// Standard tick command to keep the loop going
	tickCmd = tea.Tick(time.Millisecond*50, func(t time.Time) tea.Msg {
		return TickMsg(t)
	})

	if m.isSearching {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			switch msg.String() {
			case "enter", "esc":
				m.isSearching = false
				m.searchInput.Blur()
				return m, tickCmd
			}
		case TickMsg:
			return m, tickCmd
		}
		m.searchInput, cmd = m.searchInput.Update(msg)
		m.filterCurrentView()
		return m, tea.Batch(cmd, tickCmd)
	}

	switch msg := msg.(type) {
	case tea.KeyMsg:
		if m.showHelp {
			m.showHelp = false
			return m, tickCmd
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
			return m, tea.Batch(textinput.Blink, tickCmd)
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
				if m.searchInput.Value() != "" {
					maxVisible -= 2
				}
				if m.cursor >= m.topIndex+maxVisible {
					m.topIndex = m.cursor - maxVisible + 1
				}
			}
		case "s":
			if !m.scanning {
				m.scanning = true
				return m, tea.Batch(func() tea.Msg {
					m.db.ClearTracks()
					scanner := library.NewScanner(m.db)
					home, _ := os.UserHomeDir()
					musicDir := filepath.Join(home, "Music")
					scanner.ScanDirectory(musicDir)
					tracks, _ := m.db.GetAllTracks()
					artists, _ := m.db.GetArtists()
					albums, _ := m.db.GetAlbums()
					playlists, _ := m.db.GetPlaylists()
					return ScanCompleteMsg{tracks, artists, albums, playlists}
				}, tickCmd)
			}
		case "enter":
			if m.mode == HomeView {
				tracks := m.getCurrentTracks()
				if len(tracks) > 0 {
					m.queue = tracks
					m.playingIndex = m.cursor
					track := m.queue[m.playingIndex]
					m.current = &track
					m.audioEngine.PlayFile(track.Path)
				}
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
			m.resetNavigation()
		case "2":
			m.mode = ArtistView
			m.resetNavigation()
		case "3":
			m.mode = AlbumView
			m.resetNavigation()
		case "4":
			m.mode = PlaylistView
			m.resetNavigation()
		}
		return m, tickCmd
	case ScanCompleteMsg:
		m.scanning = false
		m.tracks = msg.tracks
		m.artists = msg.artists
		m.albums = msg.albums
		m.playlists = msg.playlists
		return m, tickCmd
	case TickMsg:
		return m, tickCmd
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		return m, tickCmd
	}
	return m, nil
}

func (m *Model) resetNavigation() {
	m.cursor = 0
	m.topIndex = 0
	m.filteredTracks = nil
	m.filteredArtists = nil
	m.filteredAlbums = nil
	m.filteredPlaylists = nil
	m.searchInput.SetValue("")
}

func (m *Model) filterCurrentView() {
	query := m.searchInput.Value()
	if query == "" {
		m.filteredTracks = nil
		m.filteredArtists = nil
		m.filteredAlbums = nil
		m.filteredPlaylists = nil
		return
	}

	switch m.mode {
	case HomeView:
		var targets []string
		for _, t := range m.tracks {
			targets = append(targets, fmt.Sprintf("%s %s %s", t.Title, t.Artist, t.Album))
		}
		matches := fuzzy.Find(query, targets)
		m.filteredTracks = make([]library.Track, len(matches))
		for i, match := range matches {
			m.filteredTracks[i] = m.tracks[match.Index]
		}
	case ArtistView:
		matches := fuzzy.Find(query, m.artists)
		m.filteredArtists = make([]string, len(matches))
		for i, match := range matches {
			m.filteredArtists[i] = m.artists[match.Index]
		}
	case AlbumView:
		matches := fuzzy.Find(query, m.albums)
		m.filteredAlbums = make([]string, len(matches))
		for i, match := range matches {
			m.filteredAlbums[i] = m.albums[match.Index]
		}
	case PlaylistView:
		matches := fuzzy.Find(query, m.playlists)
		m.filteredPlaylists = make([]string, len(matches))
		for i, match := range matches {
			m.filteredPlaylists[i] = m.playlists[match.Index]
		}
	}
	
	if m.cursor >= m.getItemCount() {
		m.cursor = 0
		m.topIndex = 0
	}
}

func (m Model) getItemCount() int {
	isSearching := m.searchInput.Value() != ""
	switch m.mode {
	case HomeView:
		if isSearching { return len(m.filteredTracks) }
		return len(m.tracks)
	case ArtistView:
		if isSearching { return len(m.filteredArtists) }
		return len(m.artists)
	case AlbumView:
		if isSearching { return len(m.filteredAlbums) }
		return len(m.albums)
	case PlaylistView:
		if isSearching { return len(m.filteredPlaylists) }
		return len(m.playlists)
	default:
		return 0
	}
}

func (m Model) getCurrentTracks() []library.Track {
	if m.searchInput.Value() != "" {
		return m.filteredTracks
	}
	return m.tracks
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
  /            : Search in current view
  s            : Scan Music folder
  1, 2, 3, 4   : Switch views (Home, Artists, Albums, Playlists)
  ?            : Toggle help
  q / Ctrl+C   : Quit

  SEARCH
  Type to filter items. Press Enter or Esc to finish searching.

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

	activeStyle := m.styles.ActiveItem
	normalStyle := lipgloss.NewStyle()

	for _, item := range items {
		style := normalStyle
		if m.mode == item.mode {
			style = activeStyle
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
		return searchBar + m.renderArtists()
	case AlbumView:
		return searchBar + m.renderAlbums()
	case PlaylistView:
		return searchBar + m.renderPlaylists()
	default:
		return ""
	}
}

func (m Model) renderTracks() string {
	tracks := m.getCurrentTracks()
	title := "All Tracks"
	if m.searchInput.Value() != "" {
		title = fmt.Sprintf("Search Results (%d)", len(tracks))
	}

	if len(tracks) == 0 {
		if m.searchInput.Value() != "" { return "No matches found." }
		return "No tracks found.\n\nPress 's' to scan your Music folder.\nPress '?' for help."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render(title))
	b.WriteString("\n\n")

	offset := 5
	if m.searchInput.Value() != "" { offset = 7 }
	maxVisible := m.height - 3 - offset
	if maxVisible <= 0 { return "Terminal too small" }

	endIndex := m.topIndex + maxVisible
	if endIndex > len(tracks) { endIndex = len(tracks) }

	activeStyle := m.styles.ActiveItem
	normalStyle := lipgloss.NewStyle()
	titleWidth := m.width - 25 - 4 - 2 - 3 - 25
	if titleWidth < 10 { titleWidth = 10 }

	for i := m.topIndex; i < endIndex; i++ {
		track := tracks[i]
		cursor := " "
		style := normalStyle
		if i == m.cursor {
			cursor = ">"
			style = activeStyle
		}
		
		line := fmt.Sprintf("%s %-*s | %s", cursor, titleWidth, truncate(track.Title, titleWidth), truncate(track.Artist, 20))
		b.WriteString(style.Render(line))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderArtists() string {
	artists := m.artists
	if m.searchInput.Value() != "" { artists = m.filteredArtists }

	if len(artists) == 0 {
		if m.searchInput.Value() != "" { return "No matches found." }
		return "No artists found. Scan your library first."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Artists"))
	b.WriteString("\n\n")

	offset := 5
	if m.searchInput.Value() != "" { offset = 7 }
	maxVisible := m.height - 3 - offset
	if maxVisible <= 0 { return "Terminal too small" }

	endIndex := m.topIndex + maxVisible
	if endIndex > len(artists) { endIndex = len(artists) }

	activeStyle := m.styles.ActiveItem
	normalStyle := lipgloss.NewStyle()

	for i := m.topIndex; i < endIndex; i++ {
		cursor := " "
		style := normalStyle
		if i == m.cursor {
			cursor = ">"
			style = activeStyle
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, artists[i])))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderAlbums() string {
	albums := m.albums
	if m.searchInput.Value() != "" { albums = m.filteredAlbums }

	if len(albums) == 0 {
		if m.searchInput.Value() != "" { return "No matches found." }
		return "No albums found. Scan your library first."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Albums"))
	b.WriteString("\n\n")

	offset := 5
	if m.searchInput.Value() != "" { offset = 7 }
	maxVisible := m.height - 3 - offset
	if maxVisible <= 0 { return "Terminal too small" }

	endIndex := m.topIndex + maxVisible
	if endIndex > len(albums) { endIndex = len(albums) }

	activeStyle := m.styles.ActiveItem
	normalStyle := lipgloss.NewStyle()

	for i := m.topIndex; i < endIndex; i++ {
		cursor := " "
		style := normalStyle
		if i == m.cursor {
			cursor = ">"
			style = activeStyle
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, albums[i])))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderPlaylists() string {
	p := m.playlists
	if m.searchInput.Value() != "" { p = m.filteredPlaylists }

	if len(p) == 0 {
		if m.searchInput.Value() != "" { return "No matches found." }
		return "No playlists found. Create one with '+' (not yet implemented)."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Playlists"))
	b.WriteString("\n\n")

	offset := 5
	if m.searchInput.Value() != "" { offset = 7 }
	maxVisible := m.height - 3 - offset
	if maxVisible <= 0 { return "Terminal too small" }

	endIndex := m.topIndex + maxVisible
	if endIndex > len(p) { endIndex = len(p) }

	activeStyle := m.styles.ActiveItem
	normalStyle := lipgloss.NewStyle()

	for i := m.topIndex; i < endIndex; i++ {
		cursor := " "
		style := normalStyle
		if i == m.cursor {
			cursor = ">"
			style = activeStyle
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, p[i])))
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
		if len > 0 { progress = float64(pos) / float64(len) }
		
		sr := m.audioEngine.SampleRate
		currentTime := time.Duration(pos) * time.Second / time.Duration(sr)
		totalTime := time.Duration(len) * time.Second / time.Duration(sr)
		timeInfo = fmt.Sprintf("%02d:%02d / %02d:%02d", 
			int(currentTime.Minutes()), int(currentTime.Seconds())%60,
			int(totalTime.Minutes()), int(totalTime.Seconds())%60)
	}

	width := m.width - 4
	barWidth := 25
	bar := renderSmoothBar(barWidth, progress)

	visualizer := m.renderVisualizer(8)

	return fmt.Sprintf("%s %s %s [%s] %s | 󰒭 Prev  󰒮 Next  󰓃 Vol", 
		visualizer, status, truncate(trackInfo, width-65), bar, timeInfo)
}

func (m Model) renderVisualizer(width int) string {
	if m.audioEngine == nil || m.audioEngine.Ctrl == nil || m.audioEngine.Ctrl.Paused {
		return "      "
	}
	
	samples := m.audioEngine.GetSamples()
	if len(samples) == 0 {
		return "      "
	}

	bars := []string{" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"}
	var res strings.Builder
	
	bucketSize := len(samples) / width
	if bucketSize == 0 { bucketSize = 1 }
	for i := 0; i < width; i++ {
		sum := 0.0
		for j := 0; j < bucketSize && (i*bucketSize+j) < len(samples); j++ {
			val := samples[i*bucketSize+j]
			if val < 0 { val = -val }
			sum += val
		}
		avg := sum / float64(bucketSize)
		idx := int(avg * 15)
		if idx >= len(bars) { idx = len(bars) - 1 }
		res.WriteString(bars[idx])
	}
	
	return m.styles.ActiveItem.Render(res.String())
}

func renderSmoothBar(width int, progress float64) string {
	if progress < 0 { progress = 0 }
	if progress > 1 { progress = 1 }

	blocks := []string{" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"}
	
	totalValue := progress * float64(width)
	fullBlocks := int(totalValue)
	remainder := totalValue - float64(fullBlocks)
	
	var bar strings.Builder
	for i := 0; i < fullBlocks; i++ {
		bar.WriteString("█")
	}
	
	if fullBlocks < width {
		blockIdx := int(remainder * 8)
		bar.WriteString(blocks[blockIdx])
		
		for i := fullBlocks + 1; i < width; i++ {
			bar.WriteString("░")
		}
	}
	
	return bar.String()
}

func truncate(s string, l int) string {
	if len(s) > l {
		if l > 3 { return s[:l-3] + "..." }
		return s[:l]
	}
	return s
}
