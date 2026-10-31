package flickrt

import (
	"net/http"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// The combinators of Http.Client, as Effect's: each makes a client of
// another's preparation and sending.

func (c *Client) withPre(pre func(fb *Fiber, request Value) Value) *Client {
	return &Client{pre: pre, post: c.post}
}

// transformResponse is the client with the task of each response changed
// by `f`.
func (c *Client) transformResponse(f func(fb *Fiber, task Value) Value) *Client {
	return &Client{pre: c.pre, post: func(fb *Fiber, prepared Value) Value { return f(fb, c.post(fb, prepared)) }}
}

// mapTask is a task of `f` of the task's value.
func mapTask(task Value, f func(fb *Fiber, x Value) Value) Value {
	return Eff(func(fb *Fiber) Value { return f(fb, Run(fb, task)) })
}

// typedError is the first typed failure of a cause without defects, as
// Effect's `catch` takes it.
func typedError(c *Cause) (Value, bool) {
	if c.has(causeDie) || !c.has(causeFail) {
		return nil, false
	}
	return c.firstOf(causeFail).err, true
}

// recovering is a task of `task`, its typed failure given to `f`.
func recovering(task Value, f func(fb *Fiber, err Value, cause *Cause) Value) Value {
	return Eff(func(fb *Fiber) Value {
		out, cause := catch(func() Value { return Run(fb, task) })
		if cause == nil {
			return out
		}
		if err, ok := typedError(cause); ok {
			return f(fb, err, cause)
		}
		panic(&Failure{cause})
	})
}

func HttpClientTransform(f, client Value) Value {
	c := httpClientOf(client)
	return &Client{pre: c.pre, post: func(fb *Fiber, prepared Value) Value {
		return mapTask(prepared, func(fb *Fiber, request Value) Value {
			return Run(fb, Apply(fb, Apply(fb, f, c.post(fb, Succeed(request))), request))
		})
	}}
}

func HttpClientTransformResponse(f, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value { return Apply(fb, f, task) })
}

func HttpClientCatch(f, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return recovering(task, func(fb *Fiber, err Value, _ *Cause) Value { return Run(fb, Apply(fb, f, err)) })
	})
}

func HttpClientTapError(f, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return recovering(task, func(fb *Fiber, err Value, cause *Cause) Value {
			Run(fb, Apply(fb, f, err))
			panic(&Failure{cause})
		})
	})
}

func HttpClientFilterOrElse(f, orElse, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, func(fb *Fiber, r Value) Value {
			if Apply(fb, f, r).(bool) {
				return r
			}
			return Run(fb, Apply(fb, orElse, r))
		})
	})
}

func HttpClientFilterOrFail(f, orFail, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, func(fb *Fiber, r Value) Value {
			if Apply(fb, f, r).(bool) {
				return r
			}
			return Fail(Apply(fb, orFail, r))
		})
	})
}

func HttpClientFilterStatus(f, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, func(fb *Fiber, r Value) Value { return ClientResponseFilterStatus(fb, f, r) })
	})
}

func HttpClientFilterStatusOk(client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, ClientResponseFilterStatusOk)
	})
}

func HttpClientTap(f, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, func(fb *Fiber, r Value) Value {
			Run(fb, Apply(fb, f, r))
			return r
		})
	})
}

func HttpClientMapRequest(f, client Value) Value {
	c := httpClientOf(client)
	return c.withPre(func(fb *Fiber, request Value) Value {
		return mapTask(c.pre(fb, request), func(fb *Fiber, r Value) Value { return Apply(fb, f, r) })
	})
}

func HttpClientMapRequestEffect(f, client Value) Value {
	c := httpClientOf(client)
	return c.withPre(func(fb *Fiber, request Value) Value {
		return mapTask(c.pre(fb, request), func(fb *Fiber, r Value) Value { return Run(fb, Apply(fb, f, r)) })
	})
}

func HttpClientMapRequestInput(f, client Value) Value {
	c := httpClientOf(client)
	return c.withPre(func(fb *Fiber, request Value) Value { return c.pre(fb, Apply(fb, f, request)) })
}

func HttpClientMapRequestInputEffect(f, client Value) Value {
	c := httpClientOf(client)
	return c.withPre(func(fb *Fiber, request Value) Value {
		return mapTask(Apply(fb, f, request), func(fb *Fiber, r Value) Value { return Run(fb, c.pre(fb, r)) })
	})
}

func HttpClientTapRequest(f, client Value) Value {
	c := httpClientOf(client)
	return c.withPre(func(fb *Fiber, request Value) Value {
		return mapTask(c.pre(fb, request), func(fb *Fiber, r Value) Value {
			Run(fb, Apply(fb, f, r))
			return r
		})
	})
}

// HttpClientWithCookiesRef sends the cookies of the cell with each request
// and keeps those of each response in it.
func HttpClientWithCookiesRef(cell, client Value) Value {
	withCookies := HttpClientMapRequestInputEffect(Fn(func(fb *Fiber, request Value) Value {
		return Eff(func(*Fiber) Value {
			if header := CookiesToCookieHeader(RefGet(cell)).(string); header != "" {
				return ClientRequestSetHeader("cookie", header, request)
			}
			return request
		})
	}), client)
	return httpClientOf(withCookies).transformResponse(func(fb *Fiber, task Value) Value {
		return mapTask(task, func(_ *Fiber, r Value) Value {
			RefSet(cell, CookiesMerge(clientResponseOf(r).cookies, RefGet(cell)))
			return r
		})
	})
}

func HttpClientRetry(policy, client Value) Value {
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		return Eff(func(fb *Fiber) Value { return retrying(fb, scheduleOf(policy), task, nil) })
	})
}

// retrying runs `task` again after each typed failure the schedule, given
// the error, continues for, waiting the schedule's delay or the one
// `delayOf` the error gives, whichever is longer.
func retrying(fb *Fiber, s *Schedule, task Value, delayOf func(Value) time.Duration) Value {
	next := s.start()
	for {
		out, cause := catch(func() Value { return Run(fb, task) })
		if cause == nil {
			return out
		}
		err, ok := typedError(cause)
		if !ok {
			panic(&Failure{cause})
		}
		d, again := next(err)
		if !again {
			panic(&Failure{cause})
		}
		if delayOf != nil {
			d = max(d, delayOf(err))
		}
		sleep(fb, d)
	}
}

var transientStatus = map[int32]bool{408: true, 429: true, 500: true, 502: true, 503: true, 504: true}

// HttpClientRetryTransient repeats a request answered with a transient
// status, and retries one that failed in transport or with such a status,
// at most `times` more times each, a `Retry-After` header of the response
// delaying the next try.
func HttpClientRetryTransient(on, times, client Value) Value {
	mode := ConstructorName(on.(*Data).Tag)
	limited := ScheduleRecurs(times).(*Schedule)
	return httpClientOf(client).transformResponse(func(fb *Fiber, task Value) Value {
		if mode != "ErrorsOnly" {
			task = repeatingTransient(task, limited)
		}
		if mode != "ResponseOnly" {
			retried := task
			task = Eff(func(fb *Fiber) Value {
				return retrying(fb, limited.while(isTransientFailure), retried, nil)
			})
		}
		return task
	})
}

func repeatingTransient(task Value, s *Schedule) Value {
	return Eff(func(fb *Fiber) Value {
		next := s.start()
		for {
			r := clientResponseOf(Run(fb, task))
			if !transientStatus[r.status] {
				return r
			}
			d, again := next(r)
			if !again {
				return r
			}
			sleep(fb, max(d, retryAfter(recordField(r.headers, "retry-after"))))
		}
	})
}

// isTransientFailure is whether a Flick HttpClientError is of the
// transport or of a transient status.
func isTransientFailure(err Value) bool {
	reason, ok := failureReason(err)
	if !ok {
		return false
	}
	return reason.Tag == transportErrorTag ||
		reason.Tag == statusErrorTag && transientStatus[record(reason.Fields[0]).Get("status").(int32)]
}

var retryAfterSeconds = regexp.MustCompile(`^\d+(?:\.\d+)?$`)

// retryAfter is the delay a `Retry-After` value asks for, at least a
// millisecond: seconds or a date.
func retryAfter(header Value) time.Duration {
	v, ok := header.(string)
	if !ok {
		return 0
	}
	v = strings.TrimSpace(v)
	if retryAfterSeconds.MatchString(v) {
		s, _ := strconv.ParseFloat(v, 64)
		return max(time.Duration(s*float64(time.Second)), time.Millisecond)
	}
	at, err := http.ParseTime(v)
	if err != nil {
		return 0
	}
	return max(time.Until(at), time.Millisecond)
}

// HttpClientFollowRedirects follows a 3xx answer with a `Location`, at
// most `limit` (by default 10) times: a POST after 301 or 302, and any
// method but GET and HEAD after 303, becomes a GET without a body, and
// credentials stay behind when the origin changes.
func HttpClientFollowRedirects(limit, client Value) Value {
	most := int32(10)
	if n, ok := some(limit); ok {
		most = n.(int32)
	}
	c := httpClientOf(client)
	return &Client{pre: c.pre, post: func(fb *Fiber, prepared Value) Value {
		return Eff(func(fb *Fiber) Value {
			for redirects := int32(0); ; redirects++ {
				var current *ClientRequest
				r := clientResponseOf(Run(fb, c.post(fb, mapTask(prepared, func(_ *Fiber, q Value) Value {
					current = clientOf(q)
					return q
				}))))
				location, _ := recordField(r.headers, "location").(string)
				if r.status < 300 || r.status >= 400 || location == "" || redirects >= most {
					return r
				}
				target, ok := resolveAgainst(location, r.request.url)
				if !ok {
					return Die("TypeError: Invalid URL")
				}
				next := current
				if next == nil {
					next = r.request
				}
				if (r.status == 301 || r.status == 302) && next.method == "POST" ||
					r.status == 303 && next.method != "GET" && next.method != "HEAD" {
					next = clientOf(ClientRequestSetBody(emptyBody, makeClient("GET", next.url, next.params, next.hash, next.headers, next.body)))
				}
				if base, ok := parseURL(r.request.url); !ok || base.origin() != target.origin() {
					h := next.headers.without("authorization").without("proxy-authorization").without("cookie")
					next = next.with(h, next.body)
				}
				prepared = Succeed(next.withURL(*target))
			}
		})
	}}
}

// resolveAgainst is `new URL(ref, base)`.
func resolveAgainst(ref, base string) (*webURL, bool) {
	if u, ok := parseURL(ref); ok {
		return u, true
	}
	b, err := url.Parse(base)
	if err != nil {
		return nil, false
	}
	r, err := url.Parse(ref)
	if err != nil {
		return nil, false
	}
	return parseURL(b.ResolveReference(r).String())
}

// origin is the scheme, host and port of the URL.
func (u webURL) origin() string {
	w := u
	w.path, w.query, w.hasQuery, w.fragment, w.hasFragment = "", "", false, "", false
	w.username, w.password = "", ""
	return w.href()
}

// withURL is the request of the URL, its query as the parameters and its
// fragment as the hash, as Effect's `setUrl` of a URL.
func (c *ClientRequest) withURL(u webURL) *ClientRequest {
	params, hash := u.searchParams(), None
	if u.hasFragment && u.fragment != "" {
		hash = Some(u.fragment)
	}
	u.query, u.hasQuery, u.fragment, u.hasFragment = "", false, "", false
	return makeClient(c.method, u.href(), params, hash, c.headers, c.body)
}
