package plugins

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"time"
	"unicode"
)

// APIBase is the GitHub REST API, replaced in tests.
var APIBase = "https://api.github.com"

// MarketTopic is the GitHub topic that lists a repository in the marketplace.
const MarketTopic = "herdr-plugin"

// MarketLimit is how far GitHub search pages: it serves at most 1000 results.
const MarketLimit = 1000

// Repo is one marketplace search result.
type Repo struct {
	FullName    string
	Stars       int
	Description string
	Updated     time.Time
}

// Results is one page of a marketplace search.
type Results struct {
	Total int
	Repos []Repo
}

// A result name reaches herdr plugin install and the browser, so only plain
// owner/repo slugs are accepted from the API.
var slugPattern = regexp.MustCompile(`^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$`)

// Search finds repositories tagged with MarketTopic. sort is "stars" or
// "updated"; page counts from 1.
func Search(ctx context.Context, query, sort string, page, perPage int) (Results, error) {
	q := "topic:" + MarketTopic
	if query = strings.TrimSpace(query); query != "" {
		q += " " + query
	}
	params := url.Values{"q": {q}, "sort": {sort}, "order": {"desc"}, "per_page": {fmt.Sprint(perPage)}, "page": {fmt.Sprint(page)}}
	// The 50-result page is a few hundred KB; slow links need the time.
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, APIBase+"/search/repositories?"+params.Encode(), nil)
	if err != nil {
		return Results{}, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	if token := githubToken(); token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		if ctx.Err() != nil {
			return Results{}, fmt.Errorf("marketplace search timed out")
		}
		return Results{}, fmt.Errorf("marketplace search failed: no connection")
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return Results{}, fmt.Errorf("marketplace search failed: %w", err)
	}
	var data struct {
		Message string `json:"message"`
		Total   int    `json:"total_count"`
		Items   []struct {
			FullName    string    `json:"full_name"`
			Stars       int       `json:"stargazers_count"`
			Description string    `json:"description"`
			Updated     time.Time `json:"pushed_at"`
		} `json:"items"`
	}
	decodeErr := json.Unmarshal(body, &data)
	if resp.StatusCode != http.StatusOK {
		reason := "marketplace search failed"
		if resp.StatusCode == http.StatusForbidden || resp.StatusCode == http.StatusTooManyRequests {
			reason = "GitHub rate limit"
		}
		if data.Message != "" {
			return Results{}, fmt.Errorf("%s (HTTP %d): %s", reason, resp.StatusCode, data.Message)
		}
		return Results{}, fmt.Errorf("%s (HTTP %d)", reason, resp.StatusCode)
	}
	if decodeErr != nil {
		return Results{}, fmt.Errorf("marketplace search: %w", decodeErr)
	}
	results := Results{Total: min(data.Total, MarketLimit)}
	for _, item := range data.Items {
		if !slugPattern.MatchString(item.FullName) {
			continue
		}
		results.Repos = append(results.Repos, Repo{FullName: item.FullName, Stars: item.Stars, Description: sanitize(item.Description), Updated: item.Updated})
	}
	return results, nil
}

func sanitize(text string) string {
	return strings.TrimSpace(strings.Map(func(r rune) rune {
		if !unicode.IsPrint(r) {
			return ' '
		}
		return r
	}, text))
}

// githubToken raises the search rate limit when one is available: GH_TOKEN,
// GITHUB_TOKEN, then the gh CLI's stored login. HERDR_CUSTOMIZE_NO_TOKEN=1
// keeps searches anonymous.
func githubToken() string {
	if os.Getenv("HERDR_CUSTOMIZE_NO_TOKEN") == "1" {
		return ""
	}
	for _, name := range []string{"GH_TOKEN", "GITHUB_TOKEN"} {
		if token := os.Getenv(name); token != "" {
			return token
		}
	}
	if _, err := exec.LookPath("gh"); err != nil {
		return ""
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, "gh", "auth", "token").Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}
