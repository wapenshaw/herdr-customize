package theme

// Look is what the picker shows of a palette before it is applied.
type Look struct {
	Background string
	Foreground string
	// ANSI holds palette entries 0–15; missing entries are empty.
	ANSI   [16]string
	Dark   bool
	Tokens map[string]string
}

// LoadLook reads the palette behind slug.
func LoadLook(slug string) (Look, error) {
	path, err := ResolvePalette(slug)
	if err != nil {
		return Look{}, err
	}
	pal, err := ParsePaletteFile(path)
	if err != nil {
		return Look{}, err
	}
	return LookOf(pal)
}

// LookOf describes a parsed palette.
func LookOf(pal *Palette) (Look, error) {
	tokens, err := PaletteToTokens(pal)
	if err != nil {
		return Look{}, err
	}
	look := Look{Background: pal.Background, Foreground: pal.Foreground, Tokens: tokens}
	for i := range look.ANSI {
		look.ANSI[i] = pal.PaletteColors[i]
	}
	look.Dark = IsDark(pal.Background)
	return look, nil
}

// IsDark reports whether a background color reads as dark.
func IsDark(hex string) bool {
	r, g, b := ParseRGB(hex)
	return 0.2126*float64(r)+0.7152*float64(g)+0.0722*float64(b) < 128
}
