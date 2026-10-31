package flickrt

// `ChildProcess`: commands as Effect describes them, run on `os/exec`. A
// command is immutable; each setter gives a new one, and a pipeline
// applies a setting to each of its commands as Effect's does.

import (
	"strings"
)

type procCommand struct {
	program     string
	args        []string
	cwd         *string
	env         [][2]string
	stdin       *string
	inherit     bool
	left, right *procCommand // a pipeline: the stdout of left is the stdin of right
}

func procOf(v Value) *procCommand { return v.(*procCommand) }

func (c *procCommand) piped() bool { return c.left != nil }

// each is the command with `f` applied to it, or to each command of a
// pipeline.
func (c *procCommand) each(f func(*procCommand)) *procCommand {
	if c.piped() {
		return &procCommand{left: c.left.each(f), right: c.right.each(f)}
	}
	out := *c
	f(&out)
	return &out
}

// first is the command with `f` applied to it, or to the first command of
// a pipeline; last to the last one.
func (c *procCommand) first(f func(*procCommand)) *procCommand {
	if c.piped() {
		return &procCommand{left: c.left.first(f), right: c.right}
	}
	return c.each(f)
}

func (c *procCommand) last(f func(*procCommand)) *procCommand {
	if c.piped() {
		return &procCommand{left: c.left, right: c.right.last(f)}
	}
	return c.each(f)
}

// commands is the commands of a pipeline in order.
func (c *procCommand) commands() []*procCommand {
	if c.piped() {
		return append(c.left.commands(), c.right.commands()...)
	}
	return []*procCommand{c}
}

// text is the command line, as Effect names a command in its errors.
func (c *procCommand) text() string {
	parts := make([]string, 0, 2)
	for _, cmd := range c.commands() {
		parts = append(parts, cmd.program+" "+strings.Join(cmd.args, " "))
	}
	return strings.Join(parts, " | ")
}

func ChildProcessMake(program, args Value) Value {
	return &procCommand{program: program.(string), args: stringsOf(args)}
}

func ChildProcessSetCwd(cwd, self Value) Value {
	dir := cwd.(string)
	return procOf(self).each(func(c *procCommand) { c.cwd = &dir })
}

func ChildProcessSetEnv(env, self Value) Value {
	var pairs [][2]string
	for _, p := range List(env) {
		kv := List(p)
		pairs = append(pairs, [2]string{kv[0].(string), kv[1].(string)})
	}
	return procOf(self).each(func(c *procCommand) { c.env = append(append([][2]string{}, c.env...), pairs...) })
}

func ChildProcessPrefix(program, args, self Value) Value {
	return procOf(self).first(func(c *procCommand) {
		c.args = append(append(stringsOf(args), c.program), c.args...)
		c.program = program.(string)
	})
}

func ChildProcessPipeTo(that, self Value) Value {
	return &procCommand{left: procOf(self), right: procOf(that)}
}

func ChildProcessSetStdin(text, self Value) Value {
	t := text.(string)
	return procOf(self).first(func(c *procCommand) { c.stdin = &t })
}

func ChildProcessInheritOutput(self Value) Value {
	return procOf(self).last(func(c *procCommand) { c.inherit = true })
}

// processError is the PlatformError of a failed call of `method` of
// ChildProcess.
func processError(tag, method string, description Value, cmd *procCommand) Value {
	return NewData(platformErrorTag, NewData(systemReasonTag, Rec(
		"tag", Nullary(Tag("PlatformError.SystemErrorTag."+tag)),
		"module_", "ChildProcess", "method", method, "description", description,
		"syscall", None, "pathOrDescriptor", Some(cmd.text()))))
}

// runProcess spawns the command in a scope of its own, gives `use` its
// process, and closes the scope after.
func runProcess(fb *Fiber, command Value, use func(fb *Fiber, p *process) Value) Value {
	return owned(fb, func(fb *Fiber, _ *Owner) Value {
		return use(fb, ChildProcessSpawn(fb, command).(*process))
	})
}

func ChildProcessString(fb *Fiber, command Value) Value {
	return runProcess(fb, command, func(fb *Fiber, p *process) Value { return decodeUTF8(p.out.all(fb)) })
}

func ChildProcessLines(fb *Fiber, command Value) Value {
	return runProcess(fb, command, func(fb *Fiber, p *process) Value {
		return StreamRunCollect(fb, StreamSplitLines(StreamSucceed(decodeUTF8(p.out.all(fb)))))
	})
}

func ChildProcessExitCode(fb *Fiber, command Value) Value {
	return runProcess(fb, command, func(fb *Fiber, p *process) Value { return p.wait(fb) })
}
