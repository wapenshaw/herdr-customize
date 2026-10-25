package theme

import (
	"fmt"
	"strings"
	"unicode/utf8"
)

func rgb(hexStr string) string {
	r, g, b := ParseRGB(hexStr)
	return fmt.Sprintf("%d;%d;%d", r, g, b)
}

func cell(bgHex, fgHex, text string) string {
	if bgHex == "" {
		bgHex = "#000000"
	}
	if fgHex == "" {
		fgHex = "#ffffff"
	}
	return fmt.Sprintf("\033[48;2;%s;38;2;%sm%s\033[0m", rgb(bgHex), rgb(fgHex), text)
}

func fg(fgHex, text string) string {
	if fgHex == "" {
		fgHex = "#ffffff"
	}
	return fmt.Sprintf("\033[38;2;%sm%s\033[0m", rgb(fgHex), text)
}

func padr(n int, s string) string {
	runeCount := utf8.RuneCountInString(s)
	if runeCount > n {
		runes := []rune(s)
		s = string(runes[:n])
		runeCount = n
	}
	return s + strings.Repeat(" ", n-runeCount)
}

func rep(ch string, n int) string {
	if n <= 0 {
		return ""
	}
	return strings.Repeat(ch, n)
}

// RenderSwatch renders the terminal mock of the Herdr layout in the theme's colors.
func RenderSwatch(slug string) string {
	palettePath, err := ResolvePalette(slug)
	if err != nil {
		return fmt.Sprintf("  (cannot load %s — %v)\n", slug, err)
	}

	return RenderSwatchFile(palettePath, slug)
}

// RenderSwatchFile also serves the token editor's private scratch palette.
func RenderSwatchFile(palettePath, label string) string {
	pal, err := ParsePaletteFile(palettePath)
	if err != nil {
		return fmt.Sprintf("  (invalid palette for %s: %v)\n", label, err)
	}

	tokens, err := PaletteToTokens(pal)
	if err != nil {
		return fmt.Sprintf("  (invalid tokens: %v)\n", err)
	}

	return RenderSwatchFromTokens(tokens, label)
}

// RenderSwatchFromTokens renders the preview UI from mapped tokens and a label.
func RenderSwatchFromTokens(tokens map[string]string, label string) string {
	panelBg := tokens["panel_bg"]
	var out strings.Builder
	out.WriteString(fmt.Sprintf("  %s\n\n", label))
	out.WriteString(RenderMock(tokens))
	out.WriteString("\n")

	// ANSI palette strip
	out.WriteString(" ")
	for _, t := range []string{"red", "green", "yellow", "blue", "mauve", "teal", "peach", "accent"} {
		out.WriteString(cell(tokens[t], panelBg, "  "))
	}
	out.WriteString("\n\n")
	out.WriteString(RenderTokenTable(tokens))
	return out.String()
}

// legendTokens are the tokens the table explains, most visible first.
var legendTokens = []string{
	"accent", "active_row_bg", "selection_bg", "overlay0", "surface_dim",
	"red", "green", "yellow", "blue", "teal", "mauve", "peach", "text", "panel_bg", "sidebar_bg",
}

// RenderTokenTable lists the main tokens with their color, source and role.
func RenderTokenTable(tokens map[string]string) string {
	panelBg, overlay0, subtext0 := tokens["panel_bg"], tokens["overlay0"], tokens["subtext0"]
	var out strings.Builder
	out.WriteString(fg(subtext0, "    token         source          role"))
	out.WriteString("\n")
	for _, t := range legendTokens {
		val := tokens[t]
		if val == "" {
			val = panelBg
		}
		out.WriteString(" ")
		out.WriteString(cell(val, panelBg, "  "))
		out.WriteString(fmt.Sprintf(" %s", padr(14, t)))
		out.WriteString(fg(overlay0, padr(16, TokenOrigin(t))))
		out.WriteString(fg(subtext0, TokenRole(t)))
		out.WriteString("\n")
	}
	return out.String()
}

// MockWidth is how many columns RenderMock uses.
const MockWidth = 60

// RenderMock draws a small Herdr workspace in the theme's colors.
func RenderMock(tokens map[string]string) string {
	tget := func(k string) string {
		return tokens[k]
	}

	panelBg := tget("panel_bg")
	textCol := tget("text")
	accent := tget("accent")
	sidebarBg := tget("sidebar_bg")
	activeRowBg := tget("active_row_bg")
	selectionBg := tget("selection_bg")
	blue := tget("blue")
	green := tget("green")
	peach := tget("peach")
	subtext0 := tget("subtext0")
	surface1 := tget("surface1")
	surfaceDim := tget("surface_dim")
	overlay0 := tget("overlay0")

	const SB = 18
	const PA = 26
	const PB = 16

	sb := func(fgHex, t string) string {
		return cell(sidebarBg, fgHex, padr(SB, t))
	}
	sbsel := func(fgHex, t string) string {
		return cell(activeRowBg, fgHex, padr(SB, t))
	}

	aTop := func() string {
		return cell(panelBg, accent, "┌"+rep("─", PA-2)+"┐")
	}
	aBot := func() string {
		return cell(panelBg, accent, "└"+rep("─", PA-2)+"┘")
	}
	aRow := func(fgHex, t string) string {
		return cell(panelBg, accent, "│") + cell(panelBg, fgHex, padr(PA-2, t)) + cell(panelBg, accent, "│")
	}
	aSel := func(t string) string {
		return cell(panelBg, accent, "│") + cell(selectionBg, panelBg, padr(PA-2, t)) + cell(panelBg, accent, "│")
	}

	iTop := func() string {
		return cell(surfaceDim, overlay0, "┌"+rep("─", PB-2)+"┐")
	}
	iBot := func() string {
		return cell(surfaceDim, overlay0, "└"+rep("─", PB-2)+"┘")
	}
	iRow := func(fgHex, t string) string {
		return cell(surfaceDim, overlay0, "│") + cell(surfaceDim, fgHex, padr(PB-2, t)) + cell(surfaceDim, overlay0, "│")
	}

	var out strings.Builder

	// Tab bar
	out.WriteString(sb(subtext0, " spaces"))
	out.WriteString(cell(accent, panelBg, " 1 · Shell "))
	out.WriteString(cell(surface1, subtext0, " 2 · agent "))
	out.WriteString(fg(overlay0, " +"))
	out.WriteString("\n")

	// Top border of panes + space1
	out.WriteString(sbsel(accent, " ○ space1"))
	out.WriteString(aTop())
	out.WriteString(iTop())
	out.WriteString("\n")

	out.WriteString(sb(textCol, "     main"))
	out.WriteString(aRow(peach, " ~   ✓ 09:31"))
	out.WriteString(iRow(overlay0, "187 command"))
	out.WriteString("\n")

	// space2
	out.WriteString(sb(subtext0, "   space2"))
	out.WriteString(aRow(textCol, " 187 command ="))
	out.WriteString(iRow(subtext0, "188 descrip"))
	out.WriteString("\n")

	out.WriteString(sb(overlay0, "     main"))
	out.WriteString(aRow(blue, " 188 command ="))
	out.WriteString(iRow(subtext0, "189 descrip"))
	out.WriteString("\n")

	// Selection inside focused pane
	out.WriteString(sb(sidebarBg, ""))
	out.WriteString(aSel(" 229 key = prefix+down"))
	out.WriteString(iRow(overlay0, ""))
	out.WriteString("\n")

	// Bottom border of panes
	out.WriteString(sb(sidebarBg, ""))
	out.WriteString(aBot())
	out.WriteString(iBot())
	out.WriteString("\n")

	// Sidebar footer
	half := SB / 2
	pbp := cell(panelBg, panelBg, padr(PA+PB, ""))
	out.WriteString(cell(sidebarBg, overlay0, padr(half, " new")))
	out.WriteString(cell(sidebarBg, overlay0, padr(half, "menu")))
	out.WriteString(pbp)
	out.WriteString("\n")

	out.WriteString(cell(sidebarBg, overlay0, padr(half, " agents")))
	out.WriteString(cell(sidebarBg, overlay0, padr(half, "priority")))
	out.WriteString(pbp)
	out.WriteString("\n")

	// Agent panel
	out.WriteString(cell(panelBg, subtext0, " AGENT "))
	out.WriteString("\n")
	out.WriteString(cell(activeRowBg, green, " ● "))
	out.WriteString(cell(activeRowBg, accent, "claude "))
	out.WriteString(cell(activeRowBg, overlay0, "herdr-customize"))
	out.WriteString("\n")
	out.WriteString(cell(panelBg, overlay0, " ○ "))
	out.WriteString(cell(panelBg, subtext0, "pi "))
	out.WriteString(cell(panelBg, overlay0, "idle"))
	out.WriteString("\n")
	return out.String()
}

// Paint renders text in truecolor on bg; empty colors fall back to black
// and white.
func Paint(bgHex, fgHex, text string) string { return cell(bgHex, fgHex, text) }
