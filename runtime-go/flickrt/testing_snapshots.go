package flickrt

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
)

// The snapshots of Test.to.match_snapshot, as the JS runtime's
// core/snapshots.ts: the manifest `flick test` passes as `--snapshots`, an
// object of each module's snapshot file relative to it, and the files,
// `__snapshots__/<File>.snap` beside the modules: a header line
// `<full test name> » <snapshot name>`, each line of the snapshot after two
// spaces (an empty line stays empty), entries apart by a blank line and
// sorted by their header's bytes, LF line ends. No run writes them without
// `--update-snapshots`.

// snapshotFile is the snapshot file of a module in a run.
type snapshotFile struct {
	path string
	// nil until read
	entries map[string]string
	// the text each header was checked with in the run
	checked map[string]string
	written int
}

// snapshotRun is the snapshot files of a run, by module.
type snapshotRun struct {
	mu     sync.Mutex
	files  map[string]*snapshotFile
	update bool
}

// snapshotCase is the snapshots of one test.
type snapshotCase struct {
	run  *snapshotRun
	file *snapshotFile
	test string
}

const snapshotsKey = "flick/Test/snapshots"

func readSnapshotRun(manifest string, update bool) (*snapshotRun, error) {
	bytes, err := os.ReadFile(manifest)
	if err != nil {
		return nil, err
	}
	var paths map[string]string
	if err := json.Unmarshal(bytes, &paths); err != nil {
		return nil, fmt.Errorf("%s: %w", manifest, err)
	}
	run := &snapshotRun{files: map[string]*snapshotFile{}, update: update}
	dir := filepath.Dir(manifest)
	for module, path := range paths {
		if !filepath.IsAbs(path) {
			path = filepath.Join(dir, filepath.FromSlash(path))
		}
		run.files[module] = &snapshotFile{path: path, checked: map[string]string{}}
	}
	return run, nil
}

// parseSnapshots is the entries of a snapshot file, by header.
func parseSnapshots(text string) map[string]string {
	entries := map[string]string{}
	header, started := "", false
	var body []string
	end := func() {
		if !started {
			return
		}
		// the blank line after an entry ends it
		if len(body) > 0 && body[len(body)-1] == "" {
			body = body[:len(body)-1]
		}
		entries[header] = strings.Join(body, "\n")
	}
	text = strings.TrimSuffix(strings.ReplaceAll(text, "\r\n", "\n"), "\n")
	for _, line := range strings.Split(text, "\n") {
		if line == "" || strings.HasPrefix(line, "  ") {
			body = append(body, strings.TrimPrefix(line, "  "))
			continue
		}
		end()
		header, started, body = line, true, nil
	}
	end()
	return entries
}

// renderSnapshots is the text of a snapshot file with `entries`.
func renderSnapshots(entries map[string]string) string {
	headers := make([]string, 0, len(entries))
	for header := range entries {
		headers = append(headers, header)
	}
	sort.Strings(headers)
	parts := make([]string, len(headers))
	for i, header := range headers {
		lines := strings.Split(entries[header], "\n")
		for j, line := range lines {
			if line != "" {
				lines[j] = "  " + line
			}
		}
		parts[i] = header + "\n" + strings.Join(lines, "\n") + "\n"
	}
	return strings.Join(parts, "\n")
}

// read is the entries of the file, read the first time.
func (f *snapshotFile) read() (map[string]string, error) {
	if f.entries != nil {
		return f.entries, nil
	}
	text, err := os.ReadFile(f.path)
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return nil, err
	}
	f.entries = parseSnapshots(string(text))
	return f.entries, nil
}

// of is the snapshots of the test `test` of `module`, nil when the manifest
// has no file of it.
func (r *snapshotRun) of(module, test string) *snapshotCase {
	if r == nil || r.files[module] == nil {
		return nil
	}
	return &snapshotCase{run: r, file: r.files[module], test: test}
}

// check compares `text` with the snapshot `name`: "same" (or written),
// "missing", or "other" with the text it holds.
func (c *snapshotCase) check(name, text string) (string, string) {
	if strings.Contains(c.test, "\n") || strings.Contains(name, "\n") {
		panic(Defect{"a snapshot name, and the name of its test, cannot hold a line break: " + QuoteJSON(name)})
	}
	header := c.test + " » " + name
	c.run.mu.Lock()
	defer c.run.mu.Unlock()
	f := c.file
	// a name checked again in a run is compared with the text it was checked with first
	if seen, ok := f.checked[header]; ok {
		if seen == text {
			return "same", ""
		}
		return "other", seen
	}
	entries, err := f.read()
	if err != nil {
		panic(Defect{err.Error()})
	}
	f.checked[header] = text
	stored, ok := entries[header]
	switch {
	case ok && stored == text:
		return "same", ""
	case !c.run.update && !ok:
		return "missing", ""
	case !c.run.update:
		return "other", stored
	}
	entries[header] = text
	f.written++
	return "same", ""
}

// TestSnapshot is Test.snapshot: None when the snapshot `name` of the test
// this code runs for holds `text`, or the run wrote it; Some None when
// there is none, and Some (Some stored) when it holds another text.
func TestSnapshot(fb *Fiber, name, text Value) Value {
	v, _ := fb.env.get(snapshotsKey)
	c, ok := v.(*snapshotCase)
	if !ok {
		panic(Defect{"a snapshot is checked only in the body of a test that `flick test` runs"})
	}
	switch kind, stored := c.check(name.(string), text.(string)); kind {
	case "same":
		return None
	case "missing":
		return Some(None)
	default:
		return Some(Some(stored))
	}
}

// withSnapshots is `task`, whose snapshots `c` holds.
func withSnapshots(c *snapshotCase, task Value) Value {
	if c == nil {
		return task
	}
	return Eff(func(fb *Fiber) Value {
		inner := *fb
		inner.env = fb.env.with(snapshotsKey, c)
		return Run(&inner, task)
	})
}

// snapshotSummary is what a run did to its snapshot files.
type snapshotSummary struct {
	written, removed int
	// the entries no test checks any more, left in their files, as `file: header`
	obsolete []string
}

// ofOneOf is whether `header` is that of a snapshot of one of `tests`.
func ofOneOf(header string, tests map[string]bool) bool {
	for at := 0; ; at++ {
		i := strings.Index(header[at:], " » ")
		if i < 0 {
			return false
		}
		if at += i; tests[header[:at]] {
			return true
		}
	}
}

// finish is the end of a run: the entries no test checks any more, those of
// a test the module no longer has or one that passed without checking them;
// with --update-snapshots and no --filter they are removed, and each file
// that changed is written, or removed when it is left empty. `unfinished`
// are the full names of the tests that did not pass, whose entries are
// never obsolete.
func (r *snapshotRun) finish(unfinished map[string]bool, filtered bool) (snapshotSummary, error) {
	var summary snapshotSummary
	cwd, _ := os.Getwd()
	modules := make([]string, 0, len(r.files))
	for module := range r.files {
		modules = append(modules, module)
	}
	sort.Strings(modules)
	for _, module := range modules {
		f := r.files[module]
		entries, err := f.read()
		if err != nil {
			return summary, err
		}
		shown, err := filepath.Rel(cwd, f.path)
		if err != nil {
			shown = f.path
		}
		changed := f.written > 0
		headers := make([]string, 0, len(entries))
		for header := range entries {
			headers = append(headers, header)
		}
		sort.Strings(headers)
		for _, header := range headers {
			if _, ok := f.checked[header]; ok || ofOneOf(header, unfinished) {
				continue
			}
			if r.update && !filtered {
				delete(entries, header)
				summary.removed++
				changed = true
			} else {
				summary.obsolete = append(summary.obsolete, filepath.ToSlash(shown)+": "+header)
			}
		}
		summary.written += f.written
		if !r.update || !changed {
			continue
		}
		if len(entries) == 0 {
			err = os.Remove(f.path)
			if errors.Is(err, fs.ErrNotExist) {
				err = nil
			}
		} else if err = os.MkdirAll(filepath.Dir(f.path), 0o755); err == nil {
			err = os.WriteFile(f.path, []byte(renderSnapshots(entries)), 0o644)
		}
		if err != nil {
			return summary, err
		}
	}
	return summary, nil
}

// end is the end of the run of `all`, with `results` of those it ran: what
// finish did; a stopped run still writes what it updated, and finds no
// snapshot obsolete. It exits when a file cannot be read or written.
func (r *snapshotRun) end(all []named, results []testResult, stopped, filtered bool) *snapshotSummary {
	if r == nil {
		return nil
	}
	passed := map[string]bool{}
	for _, result := range results {
		passed[result.Name] = !stopped && result.Status == "passed"
	}
	unfinished := map[string]bool{}
	for _, t := range all {
		unfinished[t.name] = !passed[t.name]
	}
	summary, err := r.finish(unfinished, filtered)
	if err != nil {
		fmt.Fprintf(stdout, "snapshots: %v\n", err)
		stdout.Flush()
		os.Exit(1)
	}
	return &summary
}

// touched is whether the run wrote, removed or found obsolete a snapshot: a
// run that did none reports as before snapshots existed.
func (s *snapshotSummary) touched() bool {
	return s != nil && (s.written > 0 || s.removed > 0 || len(s.obsolete) > 0)
}

// json is the "snapshots" key of the JSON report, after a comma, if touched.
func (s *snapshotSummary) json() string {
	if !s.touched() {
		return ""
	}
	obsolete := make([]string, len(s.obsolete))
	for i, entry := range s.obsolete {
		obsolete[i] = QuoteJSON(entry)
	}
	return fmt.Sprintf(",\"snapshots\":{\"written\":%d,\"removed\":%d,\"obsolete\":[%s]}", s.written, s.removed, strings.Join(obsolete, ","))
}

// write writes the lines of the human report, if touched: the counts, then
// each obsolete entry.
func (s *snapshotSummary) write() {
	if !s.touched() {
		return
	}
	var counts []string
	for _, c := range []struct {
		n    int
		what string
	}{{s.written, "written"}, {s.removed, "removed"}, {len(s.obsolete), "obsolete"}} {
		if c.n > 0 {
			counts = append(counts, fmt.Sprintf("%d %s", c.n, c.what))
		}
	}
	fmt.Fprintf(stdout, "  Snapshots  %s\n", strings.Join(counts, " · "))
	for _, entry := range s.obsolete {
		fmt.Fprintf(stdout, "  obsolete snapshot %s\n", entry)
	}
	if len(s.obsolete) > 0 {
		fmt.Fprintln(stdout, "  a run with --update-snapshots and without --filter removes them")
	}
}
