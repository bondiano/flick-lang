package main

import (
	"io"
	"net/http"
	"strconv"
	"strings"

	"flick.app/flickrt"
)

// call is a request: its status and body, or `refused` when nothing listens.
func call(ctx flickrt.Context, method, url, body flickrt.Value) flickrt.Value {
	req, err := http.NewRequestWithContext(ctx, method.(string), url.(string), strings.NewReader(body.(string)))
	if err != nil {
		return "refused"
	}
	if body.(string) != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "refused"
	}
	defer resp.Body.Close()
	text, _ := io.ReadAll(resp.Body)
	return strconv.Itoa(resp.StatusCode) + " " + string(text)
}
