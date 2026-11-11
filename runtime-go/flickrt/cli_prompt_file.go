package flickrt

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
)

// The file prompt: a path chosen by walking the file system, with the
// entries filtered by what the user types.

const promptConfirmTraverse = "The selected directory contains files. Would you like to traverse the selected directory?"

type promptFileState struct {
	cursor          int
	files, allFiles []string
	query           string
	path            string // the directory shown; "" before one is chosen
	confirm         bool
}

type promptFileOptions struct {
	kind, message, starting, initial string
	perPage                          int
	filter                           Value
}

var (
	promptDirectoryPath = Tag("Cli.Prompt.PathType.DirectoryPath")
	promptEitherPath    = Tag("Cli.Prompt.PathType.EitherPath")
)

func PromptFile(options Value) Value {
	o := options.(*Record)
	opts := &promptFileOptions{kind: "file", message: promptOr(o.Get("message"), "Choose a file"),
		starting: promptOr(o.Get("startingPath"), ""), initial: promptOr(o.Get("default"), ""),
		perPage: promptMaxPerPage(o.Get("maxPerPage")), filter: o.Get("filter")}
	if k, ok := some(o.Get("type_")); ok {
		switch k.(*Data).Tag {
		case promptDirectoryPath:
			opts.kind = "directory"
		case promptEitherPath:
			opts.kind = "either"
		}
	}
	return &promptLoop{
		initial: opts.start,
		render:  opts.render,
		process: opts.process,
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			return promptEraseText(opts.frame(c, state.(promptFileState), true), c.columns()) + promptResetLine
		},
	}
}

func (o *promptFileOptions) current(path string) string {
	switch {
	case path != "":
		return path
	case o.starting != "":
		if _, err := os.Stat(o.starting); err != nil {
			panic(Defect{"The provided starting path '" + o.starting + "' does not exist"})
		}
		return o.starting
	}
	cwd, err := os.Getwd()
	if err != nil {
		panic(Defect{err.Error()})
	}
	return cwd
}

func promptResolve(dir, name string) string {
	if filepath.IsAbs(name) {
		return filepath.Clean(name)
	}
	path, err := filepath.Abs(filepath.Join(dir, name))
	if err != nil {
		panic(Defect{err.Error()})
	}
	return path
}

// list is `..` and the entries of `dir` that pass the filter.
func (o *promptFileOptions) list(fb *Fiber, dir string) []string {
	entries, err := os.ReadDir(dir)
	if err != nil {
		panic(Defect{err.Error()})
	}
	names := []string{".."}
	for _, e := range entries {
		names = append(names, e.Name())
	}
	return slices.DeleteFunc(names, func(name string) bool {
		if f, ok := some(o.filter); ok && !Apply(fb, f, name).(bool) {
			return true
		}
		if o.kind != "directory" {
			return false
		}
		info, err := os.Stat(filepath.Join(dir, name))
		if err != nil {
			panic(Defect{err.Error()})
		}
		return !info.IsDir()
	})
}

func (o *promptFileOptions) start(c *promptCtx) Value {
	current := o.current("")
	if o.initial == "" {
		files := o.list(c.fb, current)
		return promptFileState{files: files, allFiles: files}
	}
	path := promptResolve(current, o.initial)
	dir := filepath.Dir(path)
	files := o.list(c.fb, dir)
	return promptFileState{cursor: max(slices.Index(files, filepath.Base(path)), 0), files: files, allFiles: files, path: dir}
}

// selected is the path under the cursor, or the directory without entries.
func (o *promptFileOptions) selected(s promptFileState) string {
	current := o.current(s.path)
	if s.cursor < len(s.files) {
		return promptResolve(current, s.files[s.cursor])
	}
	return current
}

func (o *promptFileOptions) filterText(t *promptTheme, s promptFileState, plain bool) string {
	value := s.query
	switch {
	case s.query == "" && plain:
		value = "type to filter"
	case s.query == "":
		value = ansiAnnotate(t.colors, "type to filter", t.muted)
	case !plain:
		value = ansiAnnotate(t.colors, s.query, ansiUnderline, t.primary)
	}
	return "[filter: " + value + "]"
}

// frame is the next frame, or plain the text a clear erases.
func (o *promptFileOptions) frame(c *promptCtx, s promptFileState, plain bool) string {
	t := c.theme
	resolved := promptSeparate(t.pointerSmall, o.selected(s))
	if !plain {
		resolved = ansiAnnotate(t.colors, resolved, t.muted)
	}
	leading, trailing, confirm := t.prefix, t.pointerSmall, "(Y/n)"
	if !plain {
		leading, trailing = promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted)
		confirm = ansiAnnotate(t.colors, confirm, t.muted)
	}
	if s.confirm {
		return promptRender(confirm, promptConfirmTraverse, leading, trailing, t, plain) + "\n" + resolved
	}
	leading, trailing = t.tick, t.ellipsis
	if !plain {
		leading, trailing = promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted)
	}
	return promptRender(o.filterText(t, s, plain), o.message, leading, trailing, t, plain) + "\n" + resolved + "\n" + o.entries(t, s, plain)
}

func (o *promptFileOptions) entries(t *promptTheme, s promptFileState, plain bool) string {
	if len(s.files) == 0 {
		if plain {
			return "No matches"
		}
		return ansiAnnotate(t.colors, "No matches", t.muted)
	}
	start, end := promptWindow(s.cursor, len(s.files), o.perPage)
	docs := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		paging := promptPaging(t, i == start && start > 0, i == end-1 && end < len(s.files))
		name, active := s.files[i], s.cursor == i
		if active && !plain {
			name = ansiAnnotate(t.colors, name, ansiUnderline, t.primary)
		}
		docs = append(docs, promptChoicePrefix(t, paging, active, false, plain)+name)
	}
	return strings.Join(docs, "\n")
}

func (o *promptFileOptions) render(c *promptCtx, state Value, action promptAction) string {
	t := c.theme
	switch action.kind {
	case 'b':
		return ansiBeep
	case 's':
		s := state.(promptFileState)
		output := promptRender(o.filterText(t, s, false), o.message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false)
		return output + " " + ansiAnnotate(t.colors, action.value.(string), t.submitted) + "\n"
	}
	return ansiCursorHide + o.frame(c, action.value.(promptFileState), false)
}

func promptFileQuery(s promptFileState, query string) promptAction {
	files := s.allFiles
	if query != "" {
		files = nil
		for _, f := range s.allFiles {
			if strings.Contains(strings.ToLower(f), strings.ToLower(query)) {
				files = append(files, f)
			}
		}
	}
	cursor := 0
	if len(files) > 0 && s.cursor < len(s.files) {
		cursor = max(slices.Index(files, s.files[s.cursor]), 0)
	}
	s.query, s.files, s.cursor = query, files, cursor
	return promptNext(s)
}

func promptFileMove(s promptFileState, by int) promptAction {
	if len(s.files) == 0 {
		return promptBeep
	}
	s.cursor = (s.cursor + by + len(s.files)) % len(s.files)
	return promptNext(s)
}

func (o *promptFileOptions) enter(fb *Fiber, dir string) promptAction {
	files := o.list(fb, dir)
	return promptNext(promptFileState{files: files, allFiles: files, path: dir})
}

func (o *promptFileOptions) process(c *promptCtx, input, state Value) promptAction {
	s := state.(promptFileState)
	name, ctrl := promptKeyName(input)
	typed := func() promptAction {
		text := promptOr(input.(*Record).Get("input"), "")
		if text == "" {
			return promptBeep
		}
		return promptFileQuery(s, s.query+text)
	}
	if ctrl {
		switch name {
		case "u":
			if s.confirm {
				return promptBeep
			}
			return promptFileQuery(s, "")
		case "p", "k":
			return promptFileMove(s, -1)
		case "n":
			return promptFileMove(s, 1)
		}
		return promptBeep
	}
	switch name {
	case "up":
		return promptFileMove(s, -1)
	case "down", "tab":
		return promptFileMove(s, 1)
	case "backspace":
		if s.confirm || s.query == "" {
			return promptBeep
		}
		query := []rune(s.query)
		return promptFileQuery(s, string(query[:len(query)-1]))
	case "enter", "return":
		if len(s.files) == 0 {
			return promptBeep
		}
		path := o.selected(s)
		info, err := os.Stat(path)
		if err != nil {
			panic(Defect{err.Error()})
		}
		if !info.IsDir() {
			return promptSubmit(path)
		}
		if o.kind == "file" {
			return o.enter(c.fb, path)
		}
		// a directory without entries is chosen at once, one with some is confirmed
		if !slices.ContainsFunc(o.list(c.fb, path), func(f string) bool { return f != ".." }) {
			return promptSubmit(path)
		}
		s.confirm = true
		return promptNext(s)
	case "y", "t":
		if s.confirm {
			return o.enter(c.fb, o.selected(s))
		}
		return typed()
	case "n", "f":
		if s.confirm {
			return promptSubmit(o.selected(s))
		}
		return typed()
	}
	if s.confirm {
		return promptBeep
	}
	return typed()
}
