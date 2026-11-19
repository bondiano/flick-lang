package flickrt

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

// The options of `flick test`, and the cases a run selects by them: by the
// marks of Test.skip and Test.only, which a test inherits from the groups it
// is in, and by --filter; and the runs that select and print without
// running, --list and --forbid-only.

// testOptions are the options of `flick test`, from the runner's arguments.
type testOptions struct {
	filter, reporter string
	jobs             int
	seed             int64
	seeded, watched  bool
	color, progress  bool
	list, forbidOnly bool
	// the milliseconds of each test without a Test.timeout of its own
	timeout float64
	timed   bool
}

func parseTestArgs(args []string) testOptions {
	o := testOptions{reporter: "human", jobs: 1}
	for i := 0; i < len(args); i++ {
		value := ""
		if i+1 < len(args) {
			value = args[i+1]
		}
		switch args[i] {
		case "--filter":
			o.filter = value
		case "--jobs":
			if n, err := strconv.Atoi(value); err == nil && n > 0 {
				o.jobs = n
			}
		case "--seed":
			if n, err := strconv.ParseInt(value, 10, 64); err == nil {
				o.seed, o.seeded = n, true
			}
		case "--reporter":
			o.reporter = value
		case "--color":
			o.color = value == "always"
		case "--progress":
			o.progress = value == "live"
		case "--timeout":
			if n, err := strconv.ParseFloat(value, 64); err == nil {
				o.timeout, o.timed = n, true
			}
		case "--watched":
			o.watched = true
			continue
		case "--list":
			o.list = true
			continue
		case "--forbid-only":
			o.forbidOnly = true
			continue
		default:
			continue
		}
		i++
	}
	return o
}

// TestSkip is Test.skip: the test, reported as skipped and never run.
func TestSkip(t Value) Value { return marked(t, "skip") }

// TestOnly is Test.only: the test, marked for a run of only such tests.
func TestOnly(t Value) Value { return marked(t, "only") }

func marked(t Value, mode string) Value {
	copied := *t.(*test)
	copied.mode = mode
	return &copied
}

// selectTests is the tests of suites the run runs, and those it reports as
// skipped. With --forbid-only and a test marked only, or with --list, it
// prints and exits without running any.
func selectTests(suites []Suite, o testOptions) (selected []named, skipped []testResult) {
	var all, marked []named
	for _, s := range suites {
		all = append(all, flatten(s.Module, List(s.Tests), nil, nil, marks{timeout: o.timeout, timed: o.timed})...)
	}
	// a test marked only in the selected files keeps the run to such tests
	onlyMarked := false
	for _, t := range all {
		if t.only {
			marked = append(marked, t)
		}
		onlyMarked = onlyMarked || t.only && !t.skip
	}
	if o.forbidOnly && len(marked) > 0 {
		are := "s are"
		if len(marked) == 1 {
			are = " is"
		}
		fmt.Fprintf(stdout, "--forbid-only: %d test%s marked only\n", len(marked), are)
		for _, t := range marked {
			fmt.Fprintf(stdout, "  %s\n", t.name)
		}
		stdout.Flush()
		os.Exit(1)
	}
	for _, t := range all {
		switch {
		case !strings.Contains(t.name, o.filter):
		case t.skip || onlyMarked && !t.only:
			skipped = append(skipped, testResult{Name: t.name, Status: "skipped"})
		default:
			selected = append(selected, t)
		}
	}
	if o.list {
		for _, t := range selected {
			fmt.Fprintln(stdout, t.name)
		}
		stdout.Flush()
		os.Exit(0)
	}
	return selected, skipped
}
