package flickrt

// configLookup runs a `Config a` against the current provider as Effect's
// `Config.option` does: the value when found, neither when the data is
// missing, or the message of Effect's `ConfigError` when it failed.
func configLookup(fb *Fiber, config Value) (value Value, found bool, message string) {
	panic(Defect{"Config is not ported yet"})
}
