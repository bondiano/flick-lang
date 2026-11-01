package main

// What a Prometheus scrape of a route gets: its content type and body.

import (
	"io"
	"net/http"

	"flick.app/flickrt"
)

func scrape(_ flickrt.Context, url flickrt.Value) flickrt.Value {
	response, err := http.Get(url.(string))
	if err != nil {
		panic(err)
	}
	defer response.Body.Close()
	body, _ := io.ReadAll(response.Body)
	return response.Header.Get("content-type") + "\n" + string(body)
}
