package ui

import tea "github.com/charmbracelet/bubbletea"

type stubPage struct{ name string }

func (p *stubPage) title() string                 { return p.name }
func (p *stubPage) init() tea.Cmd                 { return nil }
func (p *stubPage) update(tea.Msg) tea.Cmd        { return nil }
func (p *stubPage) view(width, height int) string { return "coming soon" }
func (p *stubPage) help() string                  { return "" }
func (p *stubPage) capturing() bool               { return false }

func newPluginsPage() page  { return &stubPage{name: "Plugins"} }
func newKeysPage() page     { return &stubPage{name: "Keys"} }
func newSettingsPage() page { return &stubPage{name: "Settings"} }
