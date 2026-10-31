package flickrt

import "os"

// A descriptor reads the environment only when Config.get runs.
type stringConfig struct {
	name       string
	fallback   Value
	hasDefault bool
}

func ConfigString(name Value) Value { return &stringConfig{name: name.(string)} }

func ConfigWithDefault(value, config Value) Value {
	self := config.(*stringConfig)
	if self.hasDefault {
		return self
	}
	out := *self
	out.fallback, out.hasDefault = value, true
	return &out
}

func ConfigGet(fb *Fiber, config Value) Value {
	Check(fb)
	self := config.(*stringConfig)
	if value, exists := os.LookupEnv(self.name); exists {
		return value
	}
	if self.hasDefault {
		return self.fallback
	}
	return Fail(NewData(Tag("Config.ConfigError.ConfigError"), "Missing environment variable: "+self.name))
}
