package flickrt

import (
	"os"
	"strings"
)

// The primitives of Effect's CLI that read the file system: Path,
// FileText and FileParse. Paths resolve against the working directory as
// Node's `path.resolve` does.

func cliAbsolute(path string) string {
	if record(pathParse(path)).Get("root").(string) != "" {
		return path
	}
	return pathResolve(path)
}

// cliPathValue is Primitive.Path.
func cliPathValue(pathType string, mustExist bool, text string) (Value, string) {
	abs := cliAbsolute(text)
	info, err := os.Stat(abs)
	exists := err == nil
	if mustExist && !exists {
		return nil, "Path does not exist: " + abs
	}
	if exists && pathType == "file" && !info.Mode().IsRegular() {
		return nil, "Path is not a file: " + abs
	}
	if exists && pathType == "directory" && !info.IsDir() {
		return nil, "Path is not a directory: " + abs
	}
	return abs, ""
}

// cliFileText is Primitive.FileText.
func cliFileText(text string) (Value, string) {
	abs := cliAbsolute(text)
	info, err := os.Stat(abs)
	if err != nil {
		return nil, "File does not exist: " + abs
	}
	if !info.Mode().IsRegular() {
		return nil, "Path is not a file: " + abs
	}
	content, err := os.ReadFile(abs)
	if err != nil {
		return nil, "Failed to read file: " + err.Error()
	}
	return string(content), ""
}

// cliFileParse is Primitive.FileParse: the data of a file as a JSON value
// (`*Record` objects keep their key order).
func cliFileParse(format, text string) (Value, string) {
	if format == "" {
		format = text[strings.LastIndexByte(text, '.')+1:]
	}
	if format != "json" && format != "yaml" && format != "yml" && format != "ini" && format != "toml" {
		return nil, "Unsupported file format: " + format
	}
	content, issue := cliFileText(text)
	if issue != "" {
		return nil, issue
	}
	var v Value
	reason := ""
	switch format {
	case "json":
		v, reason = parseJSON(content.(string))
		if reason != "" {
			reason = "SyntaxError: " + reason
		}
	case "yaml", "yml":
		v, reason = parseYaml(content.(string))
	case "ini":
		v, reason = parseIni(content.(string))
	default:
		v, reason = parseToml(content.(string))
	}
	if reason != "" {
		return nil, "Failed to parse '." + format + "' file content: " + reason
	}
	return v, ""
}
