package flickrt

import (
	"os"
	"strings"
	"testing"
)

func TestConfigStringReadsEnvironmentAtGet(t *testing.T) {
	const name = "FLICK_TEST_CONFIG_LAZY"
	t.Setenv(name, "constructed")
	config := ConfigString(name)
	t.Setenv(name, "first read")
	if got := ConfigGet(rootFiber(), config); got != "first read" {
		t.Fatalf("got %v, want the environment at get", got)
	}
	t.Setenv(name, "second read")
	if got := ConfigGet(rootFiber(), config); got != "second read" {
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
	_, cause := catch(func() Value { return ConfigGet(rootFiber(), config) })
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("missing configuration did not fail: %v", cause)
	}
	error := cause.err.(*Data)
	if error.Tag != Tag("Config.ConfigError.ConfigError") || !strings.Contains(error.Fields[0].(string), name) {
		t.Fatalf("not the named ConfigError: %v", error)
	}
	if got := ConfigGet(rootFiber(), withDefault); got != "fallback" {
		t.Fatalf("missing value got %v", got)
	}
	if got := ConfigGet(rootFiber(), ConfigWithDefault("outer", withDefault)); got != "fallback" {
		t.Fatalf("an outer default replaced a successful inner default: %v", got)
	}
	// an empty value is missing
	t.Setenv(name, "")
	if _, cause := catch(func() Value { return ConfigGet(rootFiber(), config) }); cause == nil {
		t.Fatalf("an empty environment value was read")
	}
	if got := ConfigGet(rootFiber(), withDefault); got != "fallback" {
		t.Fatalf("an empty value did not take the default: %v", got)
	}
	t.Setenv(name, "present")
	if got := ConfigGet(rootFiber(), withDefault); got != "present" {
		t.Fatalf("default hid a present environment value: %v", got)
	}
}
