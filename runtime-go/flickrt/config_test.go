package flickrt

import (
	"os"
	"strings"
	"testing"
)

// envLookup reads the process's environment, as the live `Host.Environment`.
var envLookup = Fn(func(_ *Fiber, name Value) Value {
	return Eff(func(*Fiber) Value { return PlatformEnv(name) })
})

func TestConfigStringReadsEnvironmentAtGet(t *testing.T) {
	const name = "FLICK_TEST_CONFIG_LAZY"
	t.Setenv(name, "constructed")
	config := ConfigString(name)
	t.Setenv(name, "first read")
	if got := ConfigGetWith(rootFiber(), envLookup, config); got != "first read" {
		t.Fatalf("got %v, want the environment at get", got)
	}
	t.Setenv(name, "second read")
	if got := ConfigGetWith(rootFiber(), envLookup, config); got != "second read" {
		t.Fatalf("cached a prior read: %v", got)
	}
}

func TestConfigStringMissingEmptyAndDefault(t *testing.T) {
	const name = "FLICK_TEST_CONFIG_MISSING"
	t.Setenv(name, "")
	if err := os.Unsetenv(name); err != nil {
		t.Fatal(err)
	}
	config := ConfigString(name)
	withDefault := ConfigWithDefault("fallback", config)
	_, cause := catch(func() Value { return ConfigGetWith(rootFiber(), envLookup, config) })
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("missing configuration did not fail: %v", cause)
	}
	error := cause.err.(*Data)
	if error.Tag != Tag("Config.ConfigError.ConfigError") || !strings.Contains(error.Fields()[0].(string), name) {
		t.Fatalf("not the named ConfigError: %v", error)
	}
	if got := ConfigGetWith(rootFiber(), envLookup, withDefault); got != "fallback" {
		t.Fatalf("missing value got %v", got)
	}
	if got := ConfigGetWith(rootFiber(), envLookup, ConfigWithDefault("outer", withDefault)); got != "fallback" {
		t.Fatalf("an outer default replaced a successful inner default: %v", got)
	}
	// an empty value is missing
	t.Setenv(name, "")
	if _, cause := catch(func() Value { return ConfigGetWith(rootFiber(), envLookup, config) }); cause == nil {
		t.Fatalf("an empty environment value was read")
	}
	if got := ConfigGetWith(rootFiber(), envLookup, withDefault); got != "fallback" {
		t.Fatalf("an empty value did not take the default: %v", got)
	}
	t.Setenv(name, "present")
	if got := ConfigGetWith(rootFiber(), envLookup, withDefault); got != "present" {
		t.Fatalf("default hid a present environment value: %v", got)
	}
}
