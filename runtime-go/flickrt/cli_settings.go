package flickrt

// The built-in behaviors of `--wizard` and `--trace`.

// cliWizard is the wizard mode of Command.runWith.
// shortcut: the wizard is not ported; it fails with Effect's text for an unsupported flag until internal/wizard.ts is
func cliWizard(fb *Fiber, root *cliCommand, args []string, ctx *cliContext) *cliError {
	return cliUnrecognized("--wizard", ctx.path, []string{})
}

// cliTraced runs the handler of command `name` under `--trace`.
// shortcut: no OTLP exporter yet; the handler runs untraced
func cliTraced(fb *Fiber, name string, run func() Value) Value { return run() }
