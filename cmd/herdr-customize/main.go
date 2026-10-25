package main

import (
	"errors"
	"fmt"
	"os"
	"slices"

	"github.com/wapenshaw/herdr-customize/internal/herdr"
	"github.com/wapenshaw/herdr-customize/internal/terminal"
	"github.com/wapenshaw/herdr-customize/internal/theme"
	"github.com/wapenshaw/herdr-customize/internal/ui"
)

const usage = `Usage: herdr-customize [command]

  ui [themes|plugins|keys|settings]  open the customize popup (default: themes)
  open [page]                        open the popup as a Herdr plugin pane
  apply <slug>                       apply a theme
  sync [--client <pid>]              re-send the applied colors to local clients
  preview <slug>                     print a theme preview
  add [--clipboard]                  add a theme from an editor or the clipboard
  edit <slug>                        edit a user theme's token colors
  delete <slug>                      delete a user theme

Themes are written to this machine's Herdr config. Clients attached with
herdr --remote use their own machine's config: run herdr-customize there.`

var pages = []string{"themes", "plugins", "keys", "settings"}

func main() {
	args := os.Args[1:]
	if len(args) == 0 {
		args = []string{"ui"}
	}
	if err := run(args); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func run(args []string) error {
	switch args[0] {
	case "help", "--help", "-h":
		fmt.Println(usage)
		fmt.Printf("\nBrowse 400+ additional themes at: %s\n", theme.Hyperlink("https://terminalcolors.com", "https://terminalcolors.com"))
		return nil

	case "ui", "picker":
		page := "themes"
		if len(args) > 1 {
			page = args[1]
		}
		if !slices.Contains(pages, page) {
			return fmt.Errorf("unknown page %q (want one of %v)", page, pages)
		}
		return ui.Run(ui.Options{Tab: page})

	case "open":
		page := "themes"
		if len(args) > 1 {
			page = args[1]
		}
		if !slices.Contains(pages, page) {
			return fmt.Errorf("unknown page %q (want one of %v)", page, pages)
		}
		cmd := herdr.Command("plugin", "pane", "open",
			"--plugin", theme.PluginID,
			"--entrypoint", page,
			"--placement", "popup",
			"--focus",
		)
		cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("open the customize popup: %w", err)
		}
		return nil

	case "client-setup":
		theme.ShowClientSetup()
		return nil

	case "sync":
		client := os.Getenv("HERDR_CUSTOMIZE_CLIENT_PID")
		if len(args) == 3 && args[1] == "--client" {
			client = args[2]
		} else if len(args) != 1 {
			return errors.New("usage: herdr-customize sync [--client <pid>]")
		}
		if err := theme.SyncAppliedTheme(client); err != nil {
			if errors.Is(err, terminal.ErrUnavailable) {
				return errors.New("no Herdr client terminal found on this machine")
			}
			return err
		}
		return nil

	case "terminal-sync":
		if len(args) != 2 {
			return errors.New("usage: herdr-customize terminal-sync <client-pid>")
		}
		if err := terminal.RunHelper(args[1]); err != nil {
			if errors.Is(err, terminal.ErrUnavailable) {
				os.Exit(3)
			}
			return err
		}
		return nil

	case "preview", "--preview":
		if len(args) != 2 {
			return errors.New("usage: herdr-customize preview <slug>")
		}
		fmt.Print(theme.RenderSwatch(args[1]))
		return nil

	case "apply":
		if len(args) != 2 {
			return errors.New("usage: herdr-customize apply <slug>")
		}
		return theme.ApplyTheme(args[1])

	case "add":
		var slug string
		var err error
		if len(args) > 1 && args[1] == "--clipboard" {
			slug, err = theme.AddThemeClipboard()
		} else {
			slug, err = theme.AddThemeEditor()
		}
		if err != nil {
			return fmt.Errorf("add theme: %w", err)
		}
		if slug == "" {
			return nil
		}
		return theme.ApplyTheme(slug)

	case "edit":
		if len(args) != 2 {
			return errors.New("usage: herdr-customize edit <slug>")
		}
		if _, err := theme.UserThemeBody(args[1]); err != nil {
			return err
		}
		return ui.Run(ui.Options{Tab: "themes", EditTheme: args[1]})

	case "delete":
		if len(args) != 2 {
			return errors.New("usage: herdr-customize delete <slug>")
		}
		return theme.DeleteTheme(args[1])
	}
	// A bare slug applies that theme.
	if len(args) == 1 && theme.IsValidSlug(args[0]) {
		return theme.ApplyTheme(args[0])
	}
	return fmt.Errorf("unknown command: %s (see herdr-customize help)", args[0])
}
