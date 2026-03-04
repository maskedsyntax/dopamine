package ui

import (
	"github.com/charmbracelet/lipgloss"
)

type Theme struct {
	Primary   lipgloss.AdaptiveColor
	Secondary lipgloss.AdaptiveColor
	Accent    lipgloss.AdaptiveColor
	Background lipgloss.AdaptiveColor
	Foreground lipgloss.AdaptiveColor
	Success    lipgloss.AdaptiveColor
	Error      lipgloss.AdaptiveColor
	Warning    lipgloss.AdaptiveColor
}

var DefaultTheme = Theme{
	Primary:   lipgloss.AdaptiveColor{Light: "#7D56F4", Dark: "#7D56F4"},
	Secondary: lipgloss.AdaptiveColor{Light: "#04B575", Dark: "#04B575"},
	Accent:    lipgloss.AdaptiveColor{Light: "#EE6FF8", Dark: "#EE6FF8"},
	Background: lipgloss.AdaptiveColor{Light: "#F2F2F2", Dark: "#171717"},
	Foreground: lipgloss.AdaptiveColor{Light: "#171717", Dark: "#EEEEEE"},
	Success:    lipgloss.AdaptiveColor{Light: "#04B575", Dark: "#04B575"},
	Error:      lipgloss.AdaptiveColor{Light: "#EF4444", Dark: "#EF4444"},
	Warning:    lipgloss.AdaptiveColor{Light: "#F59E0B", Dark: "#F59E0B"},
}

type Styles struct {
	Sidebar    lipgloss.Style
	MainView   lipgloss.Style
	PlayerBar  lipgloss.Style
	Title      lipgloss.Style
	ActiveItem lipgloss.Style
}

func GetStyles(t Theme) Styles {
	return Styles{
		Sidebar: lipgloss.NewStyle().
			Width(25).
			Border(lipgloss.NormalBorder(), false, true, false, false).
			BorderForeground(t.Secondary).
			Padding(1, 2),
		MainView: lipgloss.NewStyle().
			Padding(1, 2),
		PlayerBar: lipgloss.NewStyle().
			Border(lipgloss.NormalBorder(), true, false, false, false).
			BorderForeground(t.Primary).
			Padding(0, 2).
			Height(3),
		Title: lipgloss.NewStyle().
			Foreground(t.Primary).
			Bold(true).
			MarginBottom(1),
		ActiveItem: lipgloss.NewStyle().
			Foreground(t.Accent).
			Bold(true),
	}
}
