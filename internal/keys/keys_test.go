package keys

import (
	"slices"
	"strings"
	"testing"
)

func TestParseCanonicalizes(t *testing.T) {
	cases := map[string]string{
		"prefix+shift+t":   "prefix+shift+t",
		"Prefix+T":         "prefix+shift+t",
		"shift+ctrl+K":     "ctrl+shift+k",
		"control+option+x": "ctrl+alt+x",
		"prefix+-":         "prefix+minus",
		"prefix+minus":     "prefix+minus",
		"escape":           "esc",
		"F12":              "f12",
		"ctrl++":           "ctrl+plus",
		"super+k":          "cmd+k",
		"prefix+tab":       "prefix+tab",
	}
	for in, want := range cases {
		c, err := Parse(in)
		if err != nil {
			t.Errorf("%q: %v", in, err)
			continue
		}
		if c.String() != want {
			t.Errorf("%q = %q, want %q", in, c.String(), want)
		}
	}
	for _, bad := range []string{"", "prefix+zz", "hyper+x", "f25", "prefix+"} {
		if _, err := Parse(bad); err == nil {
			t.Errorf("accepted %q", bad)
		}
	}
}

func TestExpandRanges(t *testing.T) {
	got := Expand("prefix+alt+1..9")
	if len(got) != 9 || got[0] != "prefix+alt+1" || got[8] != "prefix+alt+9" {
		t.Fatalf("expand: %q", got)
	}
}

func TestProblems(t *testing.T) {
	if p := Problems("zoom", "z"); len(p) != 1 || !strings.Contains(p[0], "intercepts typing") {
		t.Fatalf("plain key: %q", p)
	}
	if p := Problems("zoom", "prefix+z"); len(p) != 0 {
		t.Fatalf("prefix key flagged: %q", p)
	}
	if p := Problems("navigate_pane_down", "j"); len(p) != 0 {
		t.Fatalf("navigate plain key flagged: %q", p)
	}
	for _, bad := range []string{"prefix+j", "enter", "shift+tab", "3"} {
		if p := Problems("navigate_pane_down", bad); len(p) == 0 {
			t.Errorf("navigate accepted %q", bad)
		}
	}
	if p := Problems("prefix", "prefix+b"); len(p) == 0 {
		t.Fatal("prefix+ prefix accepted")
	}
	if p := Problems("zoom", "prefix+zz"); len(p) == 0 {
		t.Fatal("invalid key accepted")
	}
}

func TestConflicts(t *testing.T) {
	bindings := []Binding{
		{Owner: "keys.previous_tab", Keys: []string{"prefix+p"}},
		{Owner: "command: plugin manager", Keys: []string{"prefix+P"}},
		{Owner: "command: other", Keys: []string{"prefix+p"}},
		{Owner: "keys.switch_tab", Keys: []string{"prefix+1..9"}},
		{Owner: "command: one", Keys: []string{"prefix+1"}},
		{Owner: "keys.navigate_pane_down", Keys: []string{"j"}, Navigate: true},
		{Owner: "command: j", Keys: []string{"prefix+j"}},
	}
	conflicts := Conflicts(bindings)
	if got := conflicts["prefix+p"]; !slices.Equal(got, []string{"keys.previous_tab", "command: other"}) {
		t.Fatalf("prefix+p owners: %q", got)
	}
	if got := Clashes(conflicts, bindings[3]); !slices.Equal(got, []string{"command: one"}) {
		t.Fatalf("range clash: %q", got)
	}
	if got := Clashes(conflicts, bindings[1]); len(got) != 0 {
		t.Fatalf("shifted key clashed: %q", got)
	}
	if got := Clashes(conflicts, bindings[5]); len(got) != 0 {
		t.Fatalf("navigate key clashed with normal key: %q", got)
	}
}
