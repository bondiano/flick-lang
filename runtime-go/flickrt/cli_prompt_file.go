package flickrt

import (
	"os"
	"path/filepath"
	"strings"
)

// Effect's file prompt: a walk of the file system from a directory, its
// entries filtered by what the user types.

const fileConfirmMessage = "The selected directory contains files. Would you like to traverse the selected directory?"

type fileState struct {
	cursor     int
	files      []string
	all        []string
	query      string
	path       *string // the directory shown; none for the starting one
	confirming bool
}

type fileOptions struct {
	kind, message string // kind: file, directory or either
	starting      *string
	fallback      *string
	perPage       int
	filter        Value // a Flick `String -> Bool`, or nil
}

// resolvePath is Node's `path.resolve` of the directory and the entry.
func resolvePath(dir, entry string) string {
	out, err := filepath.Abs(filepath.Join(dir, entry))
	if err != nil {
		return filepath.Join(dir, entry)
	}
	return out
}

func (o *fileOptions) currentPath(s *fileState) string {
	if s != nil && s.path != nil {
		return *s.path
	}
	if o.starting != nil {
		if _, err := os.Stat(*o.starting); err != nil {
			panic(Defect{"The provided starting path '" + *o.starting + "' does not exist"})
		}
		return *o.starting
	}
	cwd, err := os.Getwd()
	if err != nil {
		panic(Defect{err.Error()})
	}
	return cwd
}

// list is the entries of the directory after "..", as Effect lists them:
// sorted as Node's `readdir`, then filtered.
func (o *fileOptions) list(dir string) []string {
	entries, err := os.ReadDir(dir)
	if err != nil {
		panic(Defect{nodeMessage(err, "scandir", dir)})
	}
	files := []string{".."}
	for _, e := range entries {
		files = append(files, e.Name())
	}
	out := []string{}
	for _, f := range files {
		if o.filter != nil && !Apply(o.filter, f).(bool) {
			continue
		}
		if o.kind == "directory" {
			info, err := os.Stat(filepath.Join(dir, f))
			if err != nil {
				panic(Defect{nodeMessage(err, "stat", filepath.Join(dir, f))})
			}
			if !info.IsDir() {
				continue
			}
		}
		out = append(out, f)
	}
	return out
}

func filterFiles(files []string, query string) []string {
	if query == "" {
		return files
	}
	q := ToLowerCase(query).(string)
	out := []string{}
	for _, f := range files {
		if strings.Contains(ToLowerCase(f).(string), q) {
			out = append(out, f)
		}
	}
	return out
}

func (s fileState) withQuery(query string) fileState {
	files := filterFiles(s.all, query)
	cursor := 0
	if len(files) > 0 && s.cursor < len(s.files) {
		cursor = max(indexOfString(files, s.files[s.cursor]), 0)
	}
	return fileState{cursor: cursor, files: files, all: s.all, query: query, path: s.path, confirming: s.confirming}
}

func indexOfString(xs []string, x string) int {
	for i, y := range xs {
		if y == x {
			return i
		}
	}
	return -1
}

func (o *fileOptions) selected(s fileState) string {
	current := o.currentPath(&s)
	if s.cursor >= len(s.files) {
		return current
	}
	return resolvePath(current, s.files[s.cursor])
}

func (o *fileOptions) filterText(s fileState, t *promptTheme, plain bool) string {
	value := s.query
	switch {
	case s.query == "" && plain:
		value = "type to filter"
	case s.query == "":
		value = annotate(t.colors, "type to filter", t.muted)
	case !plain:
		value = annotate(t.colors, s.query, ansiUnderline, t.primary)
	}
	return "[filter: " + value + "]"
}

func (o *fileOptions) fileLines(s fileState, t *promptTheme, plain bool) string {
	if len(s.files) == 0 {
		if plain {
			return "No matches"
		}
		return annotate(t.colors, "No matches", t.muted)
	}
	perPage := o.perPage
	start, end := pageOf(s.cursor, len(s.files), &perPage)
	lines := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		paging := pagingPrefix(t, i == start && start > 0, i == end-1 && end < len(s.files))
		prefix := paging + strings.Repeat(" ", jsLen(t.pointer))
		name := s.files[i]
		if s.cursor == i {
			if plain {
				prefix = t.pointer + paging
			} else {
				prefix = annotateSymbol(t, t.pointer, t.primary) + paging
				name = annotate(t.colors, name, ansiUnderline, t.primary)
			}
		}
		lines = append(lines, prefix+name)
	}
	return strings.Join(lines, "\n")
}

func (o *fileOptions) render(r *promptRunner, state Value, a promptAction) string {
	t := r.theme
	switch a.kind {
	case actBeep:
		return ansiBeep
	case actSubmit:
		s := state.(fileState)
		return renderPrompt(o.filterText(s, t, false), o.message, t.leading(true), t.trailing(true), t, false) +
			" " + annotate(t.colors, a.value.(string), t.submittedColor) + "\n"
	}
	s := a.state.(fileState)
	path := annotate(t.colors, separateSymbol(t.pointerSmall, o.selected(s)), t.muted)
	if s.confirming {
		hint := annotate(t.colors, "(Y/n)", t.muted)
		return ansiCursorHide + renderPrompt(hint, fileConfirmMessage, t.leading(false), t.trailing(false), t, false) +
			"\n" + path
	}
	return ansiCursorHide + renderPrompt(o.filterText(s, t, false), o.message, t.leading(true), t.trailing(true), t, false) +
		"\n" + path + "\n" + o.fileLines(s, t, false)
}

func (o *fileOptions) clear(r *promptRunner, state Value, _ promptAction) string {
	s, t := state.(fileState), r.theme
	path := separateSymbol(t.pointerSmall, o.selected(s))
	var text string
	if s.confirming {
		text = renderPrompt("(Y/n)", fileConfirmMessage, t.prefix, t.pointerSmall, t, true) + "\n" + path
	} else {
		text = renderPrompt(o.filterText(s, t, true), o.message, t.tick, t.ellipsis, t, true) + "\n" + path +
			"\n" + o.fileLines(s, t, true)
	}
	return eraseText(text, r.columns()) + promptResetLine
}

// enter is the listing of a directory chosen to walk into.
func (o *fileOptions) enter(dir string) promptAction {
	files := o.list(dir)
	return nextAction(fileState{files: files, all: files, path: &dir})
}

func (o *fileOptions) choose(s fileState) promptAction {
	if len(s.files) == 0 {
		return beepAction()
	}
	path := o.selected(s)
	info, err := os.Stat(path)
	if err != nil {
		panic(Defect{nodeMessage(err, "stat", path)})
	}
	if !info.IsDir() {
		return submitAction(path)
	}
	if o.kind == "directory" || o.kind == "either" {
		for _, f := range o.list(path) {
			if f != ".." {
				confirming := s
				confirming.confirming = true
				return nextAction(confirming)
			}
		}
		return submitAction(path)
	}
	return o.enter(path)
}

func (o *fileOptions) move(s fileState, step int) promptAction {
	if len(s.files) == 0 {
		return beepAction()
	}
	s.cursor = (s.cursor + step + len(s.files)) % len(s.files)
	return nextAction(s)
}

func (o *fileOptions) typed(s fileState, input string) promptAction {
	if input == "" {
		return beepAction()
	}
	return nextAction(s.withQuery(s.query + input))
}

// process steps on a key: navigation is on ctrl chords, so that every
// printable character reaches the filter.
func (o *fileOptions) process(_ *promptRunner, input, state Value) promptAction {
	s, k := state.(fileState), promptKeyOf(input)
	if k.ctrl {
		switch k.name {
		case "u":
			if s.confirming {
				return beepAction()
			}
			return nextAction(s.withQuery(""))
		case "p", "k":
			return o.move(s, -1)
		case "n":
			return o.move(s, 1)
		}
		return beepAction()
	}
	switch k.name {
	case "up":
		return o.move(s, -1)
	case "down", "tab":
		return o.move(s, 1)
	case "backspace":
		if s.confirming || s.query == "" {
			return beepAction()
		}
		return nextAction(s.withQuery(jsSlice(s.query, 0, jsLen(s.query)-1)))
	case "enter", "return":
		return o.choose(s)
	case "y", "t":
		if s.confirming {
			return o.enter(o.selected(s))
		}
		return o.typed(s, k.input)
	case "n", "f":
		if s.confirming {
			return submitAction(o.selected(s))
		}
		return o.typed(s, k.input)
	}
	if s.confirming {
		return beepAction()
	}
	return o.typed(s, k.input)
}

var pathTypes = map[string]string{
	Tag("Cli.Prompt.PathType.FilePath"):      "file",
	Tag("Cli.Prompt.PathType.DirectoryPath"): "directory",
	Tag("Cli.Prompt.PathType.EitherPath"):    "either",
}

// PromptFile is `Cli.Prompt.file`, on the file system of the process.
func PromptFile(options Value) Value {
	r := record(options)
	o := &fileOptions{kind: "file", message: optString(r, "message", "Choose a file"), perPage: 10}
	if v, ok := some(r.Get("type_")); ok {
		o.kind = pathTypes[v.(*Data).Tag]
	}
	if v, ok := some(r.Get("startingPath")); ok {
		text := v.(string)
		o.starting = &text
	}
	if v, ok := some(r.Get("default")); ok {
		text := v.(string)
		o.fallback = &text
	}
	if n := optInt(r, "maxPerPage"); n != nil {
		o.perPage = *n
	}
	if f, ok := some(r.Get("filter")); ok {
		o.filter = f
	}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value {
			current := o.currentPath(nil)
			dir := current
			var fallback string
			if o.fallback != nil {
				fallback = resolvePath(current, *o.fallback)
				dir = filepath.Dir(fallback)
			}
			files := o.list(dir)
			s := fileState{files: files, all: files}
			if o.fallback != nil {
				s.cursor = max(indexOfString(files, filepath.Base(fallback)), 0)
				s.path = &dir
			}
			return s
		},
		render: o.render, process: o.process, clear: o.clear,
	})
}
