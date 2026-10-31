package main

// A local OTLP collector for the observability run tests: it keeps the
// body of every request it gets, by path, ungzipped, and what its headers
// say of it.

import (
	"bytes"
	"compress/gzip"
	"io"
	"net"
	"net/http"
	"strings"
	"sync"

	"flick.app/flickrt"
)

var (
	receiverMu     sync.Mutex
	receiverServer *http.Server
	receiverBodies []string
	receiverMetas  []string
)

// latin1 is Node's `toString("latin1")`: one character per byte.
func latin1(raw []byte) string {
	runes := make([]rune, len(raw))
	for i, b := range raw {
		runes[i] = rune(b)
	}
	return string(runes)
}

func receive(w http.ResponseWriter, r *http.Request) {
	raw, _ := io.ReadAll(r.Body)
	encoding := r.Header.Get("content-encoding")
	if encoding == "" {
		encoding = "identity"
	}
	if encoding == "gzip" {
		if z, err := gzip.NewReader(bytes.NewReader(raw)); err == nil {
			raw, _ = io.ReadAll(z)
		}
	}
	key := r.Header.Get("x-api-key")
	if key == "" {
		key = "-"
	}
	receiverMu.Lock()
	receiverBodies = append(receiverBodies, r.URL.RequestURI()+" "+latin1(raw))
	receiverMetas = append(receiverMetas, r.URL.RequestURI()+" "+r.Header.Get("content-type")+" "+encoding+" "+key)
	receiverMu.Unlock()
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(200)
	w.Write([]byte("{}"))
}

// start starts the collector on a free port and gives the port.
func start(_ flickrt.Context) flickrt.Value {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	receiverServer = &http.Server{Handler: http.HandlerFunc(receive)}
	go receiverServer.Serve(listener)
	return float64(listener.Addr().(*net.TCPAddr).Port)
}

// received is everything the collector got, one request per line.
func received(_ *flickrt.Fiber) flickrt.Value {
	receiverMu.Lock()
	defer receiverMu.Unlock()
	return strings.Join(receiverBodies, "\n")
}

// meta is each request's path, content type, content encoding and
// `x-api-key`, one a line.
func meta(_ *flickrt.Fiber) flickrt.Value {
	receiverMu.Lock()
	defer receiverMu.Unlock()
	return strings.Join(receiverMetas, "\n")
}

func stop(_ flickrt.Context) flickrt.Value {
	receiverServer.Close()
	return flickrt.Unit
}
