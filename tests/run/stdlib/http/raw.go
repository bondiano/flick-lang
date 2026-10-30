package main

import (
	"context"
	"io"
	"net/http"
	"strconv"
	"strings"

	"flick.app/flickrt"
)

// Requests the client of HttpApi would never send.

func fetch(ctx flickrt.Context, method, url, body string) (*http.Response, string, error) {
	var reader io.Reader
	if body != "" {
		reader = strings.NewReader(body)
	}
	req, err := http.NewRequestWithContext(ctx, method, url, reader)
	if err != nil {
		return nil, "", err
	}
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()
	text, err := io.ReadAll(resp.Body)
	return resp, string(text), err
}

func post(ctx flickrt.Context, url, body flickrt.Value) flickrt.Value {
	resp, text, _ := fetch(ctx, "POST", url.(string), body.(string))
	return strconv.Itoa(resp.StatusCode) + " " + text
}

func get(ctx flickrt.Context, url flickrt.Value) flickrt.Value {
	resp, text, _ := fetch(ctx, "GET", url.(string), "")
	contentType := "null"
	if t := resp.Header.Values("Content-Type"); len(t) > 0 {
		contentType = t[0]
	}
	return strconv.Itoa(resp.StatusCode) + " " + contentType + " " + strconv.FormatBool(len(text) > 0)
}

func text(ctx flickrt.Context, url flickrt.Value) flickrt.Value {
	_, body, _ := fetch(ctx, "GET", url.(string), "")
	return body
}

// start starts a request without waiting for it; `wait` gives its outcome.
func start(url flickrt.Value) flickrt.Value {
	done := make(chan string, 1)
	go func() {
		resp, body, err := fetch(context.Background(), "GET", url.(string), "")
		if err != nil {
			done <- "failed: " + err.Error()
			return
		}
		done <- strconv.Itoa(resp.StatusCode) + " " + body
	}()
	return done
}

func wait(ctx flickrt.Context, pending flickrt.Value) flickrt.Value {
	select {
	case s := <-pending.(chan string):
		return s
	case <-ctx.Done():
		return "cancelled"
	}
}

func head(ctx flickrt.Context, url flickrt.Value) flickrt.Value {
	resp, _, _ := fetch(ctx, "HEAD", url.(string), "")
	return strconv.Itoa(resp.StatusCode)
}
