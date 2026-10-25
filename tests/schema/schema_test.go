package schema_test

import (
	"slices"
	"testing"

	"github.com/wapenshaw/herdr-customize/internal/schema"
)

func TestChoicesComeFromTheReference(t *testing.T) {
	ref := schema.Bundled()
	modifier, _ := ref.Lookup("ui.right_click_passthrough_modifier")
	if got := modifier.Choices(); !slices.Equal(got, []string{"", "ctrl", "alt", "cmd", "super", "meta", "hyper"}) || !modifier.Open() {
		t.Fatalf("modifier choices = %q", got)
	}
	cwd, _ := ref.Lookup("terminal.new_cwd")
	if !slices.Equal(cwd.Choices(), cwd.Values) || cwd.Open() {
		t.Fatalf("enum choices = %q", cwd.Choices())
	}
	// A description without a plain list offers nothing; it stays a text field.
	accent, _ := ref.Lookup("ui.accent")
	shell, _ := ref.Lookup("terminal.default_shell")
	if accent.Choices() != nil || shell.Choices() != nil {
		t.Fatal("choices guessed from prose")
	}
}

func TestChoicesFollowANewReference(t *testing.T) {
	k := schema.Key{Key: "ui.future", Type: schema.TypeString, Description: "Accepts left, right, or a number."}
	if got := k.Choices(); !slices.Equal(got, []string{"left", "right"}) {
		t.Fatalf("choices = %q", got)
	}
}
