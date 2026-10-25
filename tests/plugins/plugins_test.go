package plugins_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/wapenshaw/herdr-customize/internal/plugins"
	"github.com/wapenshaw/herdr-customize/tests/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

// remoteRepo creates owner/repo under a fake GitBase with one commit on main,
// an annotated tag v1 on it, and a second commit. It returns both commits.
func remoteRepo(t *testing.T) (first, second string) {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	base := t.TempDir()
	old := plugins.GitBase
	plugins.GitBase = base
	t.Cleanup(func() { plugins.GitBase = old })

	work := t.TempDir()
	git := func(dir string, args ...string) string {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t", "GIT_CONFIG_GLOBAL="+os.DevNull)
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	bare := filepath.Join(base, "owner", "repo")
	os.MkdirAll(bare, 0o755)
	git(bare, "init", "--bare", "-b", "main")
	git(work, "init", "-b", "main")
	git(work, "commit", "--allow-empty", "-m", "one")
	first = git(work, "rev-parse", "HEAD")
	git(work, "tag", "-a", "v1", "-m", "v1")
	git(work, "commit", "--allow-empty", "-m", "two")
	second = git(work, "rev-parse", "HEAD")
	git(work, "push", bare, "main", "v1")
	return first, second
}

func github(ref, commit string) plugins.Plugin {
	return plugins.Plugin{ID: "owner.repo", Source: plugins.Source{Kind: "github", Owner: "owner", Repo: "repo", RequestedRef: ref, Commit: commit}}
}

func TestResolveRemote(t *testing.T) {
	first, second := remoteRepo(t)
	for ref, want := range map[string]string{"": second, "main": second, "v1": first, "refs/tags/v1": first} {
		got, err := plugins.ResolveRemote(context.Background(), "owner/repo", ref)
		if err != nil || got != want {
			t.Errorf("ResolveRemote(%q) = %s, %v; want %s", ref, got, err, want)
		}
	}
	if _, err := plugins.ResolveRemote(context.Background(), "owner/repo", "nope"); err == nil {
		t.Error("missing ref resolved")
	}
	if _, err := plugins.ResolveRemote(context.Background(), "owner/missing", ""); err == nil {
		t.Error("missing repo resolved")
	}
}

func TestCheck(t *testing.T) {
	first, second := remoteRepo(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/owner/repo/"+second+"/herdr-plugin.toml" {
			w.Write([]byte("id = \"x\"\nversion = \"2.0.0\"\n"))
			return
		}
		http.NotFound(w, r)
	}))
	defer srv.Close()
	old := plugins.RawBase
	plugins.RawBase = srv.URL
	defer func() { plugins.RawBase = old }()

	behind := plugins.Check(context.Background(), github("", first))
	if behind.State != plugins.Behind || behind.Remote != second || behind.Version != "2.0.0" {
		t.Errorf("behind = %+v", behind)
	}
	if s := plugins.Check(context.Background(), github("", second)); s.State != plugins.Current {
		t.Errorf("current = %+v", s)
	}
	if s := plugins.Check(context.Background(), github("v1", first)); s.State != plugins.Current {
		t.Errorf("tag = %+v", s)
	}
	// An exact-commit pin is current without asking the remote.
	pinned := github(first[:10], first)
	pinned.Source.Owner = "nobody"
	if !pinned.Pinned() || plugins.Check(context.Background(), pinned).State != plugins.Current {
		t.Error("exact pin not treated as current")
	}
	missing := github("", first)
	missing.Source.Repo = "gone"
	if s := plugins.Check(context.Background(), missing); s.State != plugins.Failed || s.Err == nil {
		t.Errorf("missing = %+v", s)
	}
	local := plugins.Plugin{ID: "local", Source: plugins.Source{Kind: "local"}}
	statuses := plugins.CheckAll(context.Background(), []plugins.Plugin{github("", first), local})
	if _, ok := statuses["local"]; ok || statuses["owner.repo"].State != plugins.Behind {
		t.Errorf("CheckAll = %+v", statuses)
	}
}

func searchServer(t *testing.T, status int, body string) *http.Request {
	t.Helper()
	var seen http.Request
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = *r.Clone(context.Background())
		w.WriteHeader(status)
		w.Write([]byte(body))
	}))
	t.Cleanup(srv.Close)
	old := plugins.APIBase
	plugins.APIBase = srv.URL
	t.Cleanup(func() { plugins.APIBase = old })
	return &seen
}

func TestSearch(t *testing.T) {
	t.Setenv("GH_TOKEN", "secret")
	seen := searchServer(t, 200, `{"total_count": 5000, "items": [
		{"full_name": "a/b", "stargazers_count": 3, "description": "tab\there\u0007"},
		{"full_name": "bad name/x; rm -rf", "stargazers_count": 1}
	]}`)
	results, err := plugins.Search(context.Background(), "agents", "updated", 2, 50)
	if err != nil {
		t.Fatal(err)
	}
	if results.Total != plugins.MarketLimit || len(results.Repos) != 1 || results.Repos[0].FullName != "a/b" || results.Repos[0].Description != "tab here" {
		t.Fatalf("results = %+v", results)
	}
	q := seen.URL.Query()
	if q.Get("q") != "topic:herdr-plugin agents" || q.Get("sort") != "updated" || q.Get("page") != "2" || q.Get("per_page") != "50" {
		t.Errorf("query = %v", q)
	}
	if seen.Header.Get("Authorization") != "Bearer secret" {
		t.Error("token not sent")
	}

	t.Setenv("HERDR_CUSTOMIZE_NO_TOKEN", "1")
	seen = searchServer(t, 403, `{"message": "API rate limit exceeded"}`)
	_, err = plugins.Search(context.Background(), "", "stars", 1, 50)
	if err == nil || !strings.Contains(err.Error(), "rate limit") || !strings.Contains(err.Error(), "API rate limit exceeded") {
		t.Fatalf("err = %v", err)
	}
	if seen.Header.Get("Authorization") != "" {
		t.Error("token sent despite HERDR_CUSTOMIZE_NO_TOKEN")
	}
}

func TestListLogsAndManage(t *testing.T) {
	log := testutil.FakeHerdr(t,
		testutil.HerdrResponse{Args: "plugin list --json", Stdout: `{"result":{"plugins":[{"plugin_id":"a.b","name":"B","enabled":true,"source":{"kind":"github","owner":"a","repo":"b","subdir":"p","ref":"v2","resolved_commit":"abc"}},{"plugin_id":"l","name":"L","source":{"kind":"local"}}]}}`},
		testutil.HerdrResponse{Args: "plugin log list --plugin a.b --limit 5", Stdout: `{"result":{"logs":[{"log_id":"1","event":"startup","status":"succeeded"},{"log_id":"2","action_id":"run","status":"failed","exit_code":2}]}}`},
		testutil.HerdrResponse{Args: "plugin disable l", Stdout: `{"error":{"message":"nope"}}`, Exit: 1},
	)
	list, err := plugins.List()
	if err != nil || len(list) != 2 {
		t.Fatalf("list = %+v, %v", list, err)
	}
	b := list[0]
	if b.Spec() != "a/b/p" || b.Ref() != "v2" || !b.GitHub() || list[1].GitHub() {
		t.Errorf("plugin = %+v", b)
	}
	if b.RepoURL() != "https://github.com/a/b/tree/abc/p" {
		t.Errorf("RepoURL = %s", b.RepoURL())
	}
	logs, err := plugins.Logs("a.b", 5)
	if err != nil || len(logs) != 2 || logs[0].Trigger() != "run" || *logs[0].ExitCode != 2 {
		t.Fatalf("logs = %+v, %v", logs, err)
	}
	if err := plugins.Remove(list[1]); err != nil {
		t.Fatal(err)
	}
	if err := plugins.SetEnabled("l", false); err == nil || err.Error() != "nope" {
		t.Fatalf("disable err = %v", err)
	}
	calls := testutil.HerdrCalls(t, log)
	if !strings.Contains(strings.Join(calls, "\n"), "plugin unlink l") {
		t.Errorf("calls = %v", calls)
	}
}
