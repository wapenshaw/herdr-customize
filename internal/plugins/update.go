package plugins

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/pelletier/go-toml/v2"
)

// Remote locations, replaced in tests.
var (
	// GitBase is where git ls-remote finds owner/repo.
	GitBase = "https://github.com"
	// RawBase serves files at a commit: RawBase/owner/repo/sha/path.
	RawBase = "https://raw.githubusercontent.com"
)

// State is the result of an update check.
type State int

const (
	Unchecked State = iota
	Current
	Behind
	Failed
)

// Status says whether a GitHub plugin has an update.
type Status struct {
	State State
	// Remote is the commit an update would move to.
	Remote string
	// Version is the remote manifest's version, when it could be read.
	Version string
	Err     error
}

var shaPattern = regexp.MustCompile(`^[0-9a-fA-F]{7,40}$`)

// Pinned reports a plugin installed at an exact commit, which never moves
// by itself.
func (p Plugin) Pinned() bool {
	ref := p.Ref()
	return shaPattern.MatchString(ref) && strings.HasPrefix(strings.ToLower(p.Source.Commit), strings.ToLower(ref))
}

// ResolveRemote returns the commit ref points at on GitHub; "" means the
// default branch. Annotated tags resolve to the commit they tag.
func ResolveRemote(ctx context.Context, slug, ref string) (string, error) {
	if ref == "" {
		ref = "HEAD"
	}
	if len(ref) == 40 && shaPattern.MatchString(ref) {
		return ref, nil
	}
	name := strings.TrimPrefix(strings.TrimPrefix(ref, "refs/tags/"), "refs/heads/")
	ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "git", "ls-remote", GitBase+"/"+slug,
		"HEAD", "refs/tags/"+name, "refs/tags/"+name+"^{}", "refs/heads/"+name)
	// Never prompt for credentials, and give up on a stalled connection.
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_HTTP_LOW_SPEED_LIMIT=1000", "GIT_HTTP_LOW_SPEED_TIME=5")
	out, err := cmd.Output()
	if err != nil {
		var exit *exec.ExitError
		if errors.As(err, &exit) && len(exit.Stderr) > 0 {
			return "", fmt.Errorf("git ls-remote %s: %s", slug, strings.TrimSpace(string(exit.Stderr)))
		}
		return "", fmt.Errorf("git ls-remote %s: %w", slug, err)
	}
	refs := map[string]string{}
	scanner := bufio.NewScanner(strings.NewReader(string(out)))
	for scanner.Scan() {
		if fields := strings.Fields(scanner.Text()); len(fields) == 2 {
			refs[fields[1]] = fields[0]
		}
	}
	for _, key := range []string{"refs/tags/" + name + "^{}", "refs/tags/" + name, "refs/heads/" + name} {
		if sha := refs[key]; sha != "" {
			return sha, nil
		}
	}
	if name == "HEAD" && refs["HEAD"] != "" {
		return refs["HEAD"], nil
	}
	return "", fmt.Errorf("%s has no ref %q", slug, name)
}

// RemoteVersion reads the version from the plugin manifest at commit.
func RemoteVersion(ctx context.Context, p Plugin, commit string) (string, error) {
	url := RawBase + "/" + p.Slug() + "/" + commit
	if p.Source.Subdir != "" {
		url += "/" + p.Source.Subdir
	}
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url+"/herdr-plugin.toml", nil)
	if err != nil {
		return "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("manifest: %s", resp.Status)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return "", err
	}
	var manifest struct {
		Version string `toml:"version"`
	}
	if err := toml.Unmarshal(data, &manifest); err != nil {
		return "", err
	}
	return manifest.Version, nil
}

// Check compares an installed GitHub plugin with its remote ref.
func Check(ctx context.Context, p Plugin) Status {
	if !p.GitHub() {
		return Status{}
	}
	// An exact-commit pin cannot drift, and ls-remote cannot resolve one.
	if p.Pinned() {
		return Status{State: Current, Remote: p.Source.Commit}
	}
	remote, err := ResolveRemote(ctx, p.Slug(), p.Ref())
	if err != nil {
		return Status{State: Failed, Err: err}
	}
	if strings.EqualFold(remote, p.Source.Commit) {
		return Status{State: Current, Remote: remote}
	}
	status := Status{State: Behind, Remote: remote}
	status.Version, _ = RemoteVersion(ctx, p, remote)
	return status
}

// CheckAll checks every GitHub plugin in parallel, keyed by plugin id.
func CheckAll(ctx context.Context, list []Plugin) map[string]Status {
	out := make(map[string]Status)
	var mu sync.Mutex
	var wg sync.WaitGroup
	for _, p := range list {
		if !p.GitHub() {
			continue
		}
		wg.Add(1)
		go func() {
			defer wg.Done()
			status := Check(ctx, p)
			mu.Lock()
			out[p.ID] = status
			mu.Unlock()
		}()
	}
	wg.Wait()
	return out
}

// HaveGit reports whether update checks can run.
func HaveGit() bool {
	_, err := exec.LookPath("git")
	return err == nil
}
