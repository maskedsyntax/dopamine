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
	PlaylistTrackView
)

type InputMode int

const (
	NoInput InputMode = iota
	SearchInput
	PlaylistNameInput
	PlaylistSelectInput
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
	
	// Input state
	inputMode       InputMode
	searchInput     textinput.Model
	playlistInput   textinput.Model
	filteredTracks  []library.Track
	filteredArtists []string
	filteredAlbums  []string
	filteredPlaylists []string
	
	cursor      int
	topIndex    int // For scrolling
	current     *library.Track
	
	// Player state
	playingIndex int
	queue        []library.Track
	
	// Playlist state
	selectedPlaylist string
	trackToPlaylist  *library.Track

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

	tickCmd = tea.Tick(time.Millisecond*50, func(t time.Time) tea.Msg {
		return TickMsg(t)
	})

	// Handle Modal Inputs first
	if m.inputMode == SearchInput {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			switch msg.String() {
			case "enter", "esc":
				m.inputMode = NoInput
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

	if m.inputMode == PlaylistNameInput {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			switch msg.String() {
			case "enter":
				name := m.playlistInput.Value()
				if name != "" {
					m.db.CreatePlaylist(name)
					m.playlists, _ = m.db.GetPlaylists()
				}
				m.inputMode = NoInput
				m.playlistInput.Blur()
				m.playlistInput.SetValue("")
				return m, tickCmd
			case "esc":
				m.inputMode = NoInput
				m.playlistInput.Blur()
				return m, tickCmd
			}
		case TickMsg:
			return m, tickCmd
		}
		m.playlistInput, cmd = m.playlistInput.Update(msg)
		return m, tea.Batch(cmd, tickCmd)
	}

	if m.inputMode == PlaylistSelectInput {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			switch msg.String() {
			case "up", "k":
				if m.cursor > 0 { m.cursor-- }
			case "down", "j":
				if m.cursor < len(m.playlists)-1 { m.cursor++ }
			case "enter":
				if len(m.playlists) > 0 && m.trackToPlaylist != nil {
					m.db.AddTrackToPlaylist(m.playlists[m.cursor], m.trackToPlaylist.Path)
				}
				m.inputMode = NoInput
				m.cursor = 0
				return m, tickCmd
			case "esc":
				m.inputMode = NoInput
				m.cursor = 0
				return m, tickCmd
			}
		case TickMsg:
			return m, tickCmd
		}
		return m, tickCmd
	}

	// Main App Interaction
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
			m.inputMode = SearchInput
			m.searchInput.Focus()
			// Keep existing filter if any, or reset if desired
			return m, tea.Batch(textinput.Blink, tickCmd)
		case "backspace":
			// If we have a search query, backspace opens search again to edit
			if m.searchInput.Value() != "" {
				m.inputMode = SearchInput
				m.searchInput.Focus()
				return m, tickCmd
			}
			// Otherwise handle view navigation
			if m.mode == PlaylistTrackView {
				m.mode = PlaylistView
				m.resetNavigation()
				return m, tickCmd
			}
		case "esc":
			if m.searchInput.Value() != "" {
				m.resetNavigation()
				return m, tickCmd
			}
			if m.mode == PlaylistTrackView {
				m.mode = PlaylistView
				m.resetNavigation()
				return m, tickCmd
			}
		case "+":
			m.inputMode = PlaylistNameInput
			m.playlistInput.Focus()
			return m, tea.Batch(textinput.Blink, tickCmd)
		case "a":
			tracks := m.getCurrentTracks()
			if len(tracks) > 0 && m.cursor < len(tracks) {
				track := tracks[m.cursor]
				m.trackToPlaylist = &track
				m.inputMode = PlaylistSelectInput
				m.cursor = 0
				return m, tickCmd
			}
		case "h":
			m.audioEngine.Seek(-10)
		case "l":
			m.audioEngine.Seek(10)
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
				maxVisible := m.getMaxVisibleItems()
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
			if m.mode == HomeView || m.mode == PlaylistTrackView {
				tracks := m.getCurrentTracks()
				if len(tracks) > 0 && m.cursor < len(tracks) {
					m.queue = tracks
					m.playingIndex = m.cursor
					track := m.queue[m.playingIndex]
					m.current = &track
					m.audioEngine.PlayFile(track.Path)
				}
			} else if m.mode == PlaylistView {
				playlists := m.getCurrentPlaylists()
				if len(playlists) > 0 && m.cursor < len(playlists) {
					m.selectedPlaylist = playlists[m.cursor]
					m.mode = PlaylistTrackView
					m.tracks, _ = m.db.GetPlaylistTracks(m.selectedPlaylist)
					m.resetNavigation()
				}
			}
		case "n":
			if len(m.queue) > 0 && m.playingIndex < len(m.queue)-1 {
				m.playingIndex++
				track := m.queue[m.playingIndex]
				m.current = &track
				m.audioEngine.PlayFile(track.Path)
			}
		case "p":
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
			m.tracks, _ = m.db.GetAllTracks()
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
	m.inputMode = NoInput
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
	case HomeView, PlaylistTrackView:
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
	case HomeView, PlaylistTrackView:
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

func (m Model) getMaxVisibleItems() int {
	// sidebarHeight(m.height-3) - title(1) - spacing(1) - searchBar(3)
	offset := 5
	if m.inputMode == SearchInput || m.searchInput.Value() != "" || m.inputMode == PlaylistNameInput {
		offset = 8
	}
	v := m.height - 3 - offset
	if v <= 0 { return 1 }
	return v
}

func (m Model) getCurrentTracks() []library.Track {
	if m.searchInput.Value() != "" {
		return m.filteredTracks
	}
	return m.tracks
}

func (m Model) getCurrentPlaylists() []string {
	if m.searchInput.Value() != "" {
		return m.filteredPlaylists
	}
	return m.playlists
}

func (m Model) View() string {
	if m.width == 0 || m.height == 0 {
		return "Initializing..."
	}

	if m.showHelp {
		return m.renderHelp()
	}

	if m.inputMode == PlaylistSelectInput {
		return m.renderPlaylistSelect()
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

func (m Model) renderPlaylistSelect() string {
	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Add to Playlist"))
	b.WriteString("\n\n")
	for i, p := range m.playlists {
		cursor := " "
		style := m.styles.InactiveItem
		if i == m.cursor {
			cursor = ">"
			style = m.styles.ActiveItem
		}
		b.WriteString(style.Render(fmt.Sprintf("%s %s", cursor, p)))
		b.WriteString("\n")
	}
	if len(m.playlists) == 0 {
		b.WriteString(m.styles.HelpDesc.Render("No playlists. Press '+' to create one."))
	}
	b.WriteString("\n\n")
	b.WriteString(m.styles.HelpDesc.Render("(Enter to select, Esc to cancel)"))
	
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center,
		m.styles.MainView.BorderStyle(lipgloss.RoundedBorder()).BorderForeground(DefaultTheme.Accent).Render(b.String()))
}

func (m Model) renderHelp() string {
	var b strings.Builder
	b.WriteString(m.styles.Title.Render("DOPAMINE HELP"))
	b.WriteString("\n\n")

	keys := [][]string{
		{"k / ↑", "Move up"},
		{"j / ↓", "Move down"},
		{"Enter", "Play track / Open item"},
		{"Backspace", "Back / Edit Search"},
		{"Esc", "Reset Search / Back"},
		{"Space", "Pause / Resume"},
		{"n / p", "Next / Previous track"},
		{"h / l", "Seek -10s / +10s"},
		{"/", "Search in current view"},
		{"s", "Scan Music folder"},
		{"1-4", "Switch views (Home, Artists, Albums, Playlists)"},
		{"+", "Create New Playlist"},
		{"a", "Add track to playlist"},
		{"?", "Toggle help"},
		{"q", "Quit"},
	}

	for _, k := range keys {
		b.WriteString(fmt.Sprintf("%s %s\n", m.styles.HelpKey.Render(fmt.Sprintf("%-12s", k[0])), m.styles.HelpDesc.Render(k[1])))
	}

	b.WriteString("\n")
	b.WriteString(m.styles.HelpDesc.Render("Press any key to return..."))

	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, 
		m.styles.MainView.BorderStyle(lipgloss.RoundedBorder()).BorderForeground(DefaultTheme.Primary).Render(b.String()))
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
	normalStyle := m.styles.InactiveItem

	for _, item := range items {
		style := normalStyle
		isActive := m.mode == item.mode || (item.mode == PlaylistView && m.mode == PlaylistTrackView)
		if isActive {
			style = activeStyle
			b.WriteString(style.Render(fmt.Sprintf(" %s %s", item.icon, item.name)))
		} else {
			b.WriteString(style.Render(fmt.Sprintf("  %s %s", item.icon, item.name)))
		}
		b.WriteString("\n")
	}

	b.WriteString("\n\n")
	b.WriteString(m.styles.HelpDesc.Render("  Help (?)"))
	return b.String()
}

func (m Model) renderMainView() string {
	if m.scanning {
		return lipgloss.Place(m.width-30, m.height-5, lipgloss.Center, lipgloss.Center, "Scanning library...\n\nThis may take a moment.")
	}

	var searchBar string
	if m.inputMode == SearchInput || m.searchInput.Value() != "" {
		prompt := " "
		if m.inputMode == SearchInput {
			searchBar = m.styles.SearchHeader.Render(m.styles.ActiveItem.Render(prompt) + m.searchInput.View()) + "\n"
		} else {
			searchBar = m.styles.SearchHeader.Render(m.styles.InactiveItem.Render(prompt) + m.searchInput.Value()) + "\n"
		}
	}
	if m.inputMode == PlaylistNameInput {
		searchBar = m.styles.SearchHeader.Render(m.styles.ActiveItem.Render("󰲸 Name: ") + m.playlistInput.View()) + "\n"
	}

	switch m.mode {
	case HomeView, PlaylistTrackView:
		title := "All Tracks"
		if m.mode == PlaylistTrackView { title = "Playlist: " + m.selectedPlaylist }
		return searchBar + m.renderTracks(title)
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

func (m Model) renderTracks(title string) string {
	tracks := m.getCurrentTracks()
	if m.searchInput.Value() != "" {
		title = fmt.Sprintf("Search Results (%d)", len(tracks))
	}

	if len(tracks) == 0 {
		if m.searchInput.Value() != "" { return "\n No matches found." }
		return "\n No tracks found.\n Press 's' to scan."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render(title))
	b.WriteString("\n\n")

	maxVisible := m.getMaxVisibleItems()
	endIndex := m.topIndex + maxVisible
	if endIndex > len(tracks) { endIndex = len(tracks) }

	activeStyle := m.styles.ActiveItem
	normalStyle := m.styles.InactiveItem
	
	// Pre-calculate widths
	availableWidth := m.width - 25 - 10
	titleWidth := int(float64(availableWidth) * 0.6)
	if titleWidth < 20 { titleWidth = 20 }
	artistWidth := availableWidth - titleWidth
	if artistWidth < 15 { artistWidth = 15 }

	for i := m.topIndex; i < endIndex; i++ {
		track := tracks[i]
		cursor := "  "
		style := normalStyle
		if i == m.cursor {
			cursor = "❯ "
			style = activeStyle
		}
		
		line := fmt.Sprintf("%s %-*s  %s", cursor, titleWidth, truncate(track.Title, titleWidth), truncate(track.Artist, artistWidth))
		b.WriteString(style.Render(line))
		b.WriteString("\n")
	}
	return b.String()
}

func (m Model) renderArtists() string {
	artists := m.artists
	if m.searchInput.Value() != "" { artists = m.filteredArtists }

	if len(artists) == 0 {
		if m.searchInput.Value() != "" { return "\n No matches found." }
		return "\n No artists found."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Artists"))
	b.WriteString("\n\n")

	maxVisible := m.getMaxVisibleItems()
	endIndex := m.topIndex + maxVisible
	if endIndex > len(artists) { endIndex = len(artists) }

	activeStyle := m.styles.ActiveItem
	normalStyle := m.styles.InactiveItem

	for i := m.topIndex; i < endIndex; i++ {
		cursor := "  "
		style := normalStyle
		if i == m.cursor {
			cursor = "❯ "
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
		if m.searchInput.Value() != "" { return "\n No matches found." }
		return "\n No albums found."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Albums"))
	b.WriteString("\n\n")

	maxVisible := m.getMaxVisibleItems()
	endIndex := m.topIndex + maxVisible
	if endIndex > len(albums) { endIndex = len(albums) }

	activeStyle := m.styles.ActiveItem
	normalStyle := m.styles.InactiveItem

	for i := m.topIndex; i < endIndex; i++ {
		cursor := "  "
		style := normalStyle
		if i == m.cursor {
			cursor = "❯ "
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
		if m.searchInput.Value() != "" { return "\n No matches found." }
		return "\n No playlists found.\n Press '+' to create one."
	}

	var b strings.Builder
	b.WriteString(m.styles.Title.Render("Playlists"))
	b.WriteString("\n\n")

	maxVisible := m.getMaxVisibleItems()
	endIndex := m.topIndex + maxVisible
	if endIndex > len(p) { endIndex = len(p) }

	activeStyle := m.styles.ActiveItem
	normalStyle := m.styles.InactiveItem

	for i := m.topIndex; i < endIndex; i++ {
		cursor := "  "
		style := normalStyle
		if i == m.cursor {
			cursor = "❯ "
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
