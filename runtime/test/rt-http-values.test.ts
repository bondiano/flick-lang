/* oxlint-disable typescript/no-unsafe-type-assertion -- the bindings take the values Flick code passes, built here by hand */
// Cookies, URLs, URL parameters, media types and entity tags on Flick's own
// runtime, against what Effect 4.1's `http` modules answer for the same input.
import assert from "node:assert/strict";
import { test } from "node:test";

import { none, some } from "../src/core/data.ts";
import { durationNanos } from "../src/rt/duration.ts";
import * as Cookies from "../src/rt/effect/http/Cookies.ts";
import { flickLayer, flickLayerWeak } from "../src/rt/effect/http/Etag.ts";
import { flickGetAllExtensions, getExtension, getType } from "../src/rt/effect/http/Mime.ts";
import * as Url from "../src/rt/effect/http/Url.ts";
import * as UrlParams from "../src/rt/effect/http/UrlParams.ts";
import { operation, service } from "../src/rt/kernel.ts";
import { layerRun } from "../src/rt/layer.ts";
import { redactedMake, redactedReveal } from "../src/rt/redacted.ts";
import { tag } from "../src/tags.ts";
import { root } from "./rt-support.ts";

const attributes = {
  domain: none, expires: none, maxAge: none, path: none, priority: none,
  httpOnly: false, secure: false, partitioned: false, sameSite: none,
} as unknown as Parameters<typeof Cookies.flickMakeCookie>[2];

const revealed = (pairs: ReadonlyArray<readonly [string, unknown]>) =>
  pairs.map(([k, v]) => [k, redactedReveal(v as never)]);

test("a Cookie header keeps the first value of a name, unquoted and decoded", () => {
  assert.deepEqual(revealed(Cookies.flickParseHeader('a=1; b="x%20y"; a=2; c; d=%E0%A4%A')), [["a", "1"], ["b", "x y"], ["d", "%E0%A4%A"]]);
});

test("cookies come out in the order of an object's keys, index names first", () => {
  assert.equal(Cookies.toCookieHeader(Cookies.fromSetCookie(["b=1", "2=x", "a=3", "__proto__=p"])), "2=x; b=1; a=3; __proto__=p");
});

test("a Set-Cookie header loses the attributes it cannot read", () => {
  const headers = [
    "s=v; Domain=.Example.com; Max-Age=-5; Path=nope; Priority=weird; SameSite=STRICT; Partitioned; Expires=nonsense",
    "  =v",
    "noeq",
    "t = a%20b ; HttpOnly; secure; priority=LOW; Path=/x",
  ];
  const read = Cookies.fromSetCookie(headers);
  assert.deepEqual(Cookies.toSetCookieHeaders(read), [
    "s=v; Max-Age=-5; Domain=Example.com; Partitioned; SameSite=Strict",
    "t=a%20b; Path=/x; Priority=Low; HttpOnly; Secure",
  ]);
  assert.deepEqual(revealed(Cookies.flickToRecord(read)), [["s", "v"], ["t", "a b"]]);
});

test("a cookie's value is encoded, its age counted in whole seconds", async () => {
  const exit = await root(() => {
    const encoded = Cookies.flickMakeCookie("n", redactedMake("a b;ü"), attributes);
    const empty = Cookies.flickMakeCookie("n", redactedMake(""), attributes);
    const nanos = durationNanos(2_999_999_999n);
    assert.equal(nanos._tag, "Success");
    const aged = Cookies.flickMakeCookie("n", redactedMake("v"), { ...attributes, maxAge: some(nanos.success) });
    return [encoded, empty, aged].map(Cookies.serializeCookie);
  });
  assert.equal(exit.cause, undefined);
  assert.deepEqual(exit.value, ["n=a%20b%3B%C3%BC", "n=", "n=v; Max-Age=2"]);
});

test("a path a header cannot carry is a CookiesError", async () => {
  const exit = await root(() => Cookies.flickMakeCookie("n", redactedMake("v"), { ...attributes, path: some("/a;b") }));
  assert.deepEqual(exit.cause?.reasons, [
    { _tag: "Fail", error: { _tag: tag("Http.Cookies.CookiesError.CookiesError"), _0: { _tag: tag("Http.Cookies.CookiesErrorReason.InvalidCookiePath") } } },
  ]);
});

test("a read cookie whose value no header carries does not serialize", () => {
  const read = Cookies.fromSetCookie(["a=ā"]);
  const cookie = Cookies.get("a")(read);
  assert.equal(cookie._tag, "Some");
  assert.throws(() => Cookies.serializeCookie(cookie.value), { name: "CookiesError", message: "InvalidCookieValue" });
});

test("to_record names each cookie by its own name, not the key it is under", async () => {
  const exit = await root(() => Cookies.flickMakeCookie("n", redactedMake("v"), attributes));
  const named = Cookies.flickFromReadonlyRecord([["x", exit.value as Cookies.Cookie]]);
  assert.deepEqual(revealed(Cookies.flickToRecord(named)), [["n", "v"]]);
});

test("parameters encode as a form does", () => {
  assert.equal(UrlParams.toString([["a b", "c&d=é"], ["x", "+"]]), "a+b=c%26d%3D%C3%A9&x=%2B");
  assert.deepEqual(UrlParams.setAll([["b", "9"], ["c", "8"]])([["a", "1"], ["b", "2"], ["a", "3"]]), [["b", "9"], ["c", "8"], ["a", "1"], ["a", "3"]]);
  assert.deepEqual(UrlParams.flickToUnknown([["a", "1"], ["a", "2"], ["b", "3"]]), { a: ["1", "2"], b: "3" });
});

test("redact_url masks the user info a parser finds and the named query keys", () => {
  const defaults = UrlParams.flickDefaultRedactedNames;
  assert.equal(UrlParams.flickRedactUrl(defaults, "//u:p@host/x?Sig=1&b=2#h?sig=3"), "//REDACTED:REDACTED@host/x?Sig=REDACTED&b=2#h?sig=3");
  assert.equal(UrlParams.flickRedactUrl(defaults, "https://h/p#frag?sig=1"), "https://h/p#frag?sig=1");
  assert.equal(UrlParams.flickRedactUrl(defaults, "http://a@b@host/path?q=1"), "http://REDACTED:REDACTED@host/path?q=1");
  assert.equal(UrlParams.flickRedactUrl(["sig", "x y"], "https://h/?si%67=1&x+y=2&bad=%E0"), "https://h/?si%67=REDACTED&x+y=REDACTED&bad=%E0");
  assert.equal(UrlParams.flickRedactUrl([], " \thttps://u@h/"), " \thttps://REDACTED:REDACTED@h/");
});

test("with_redacted_names masks a URL's show, while parameters show the default names", async () => {
  const url = Url.flickFromString("https://h/?token=t&sig=s");
  assert.equal(url._tag, "Success");
  const exit = await root(() =>
    UrlParams.flickWithRedactedNames(["token"], () => [Url.flickShow(url.success), UrlParams.flickShow(Url.urlParams(url.success))]),
  );
  assert.deepEqual(exit.value, ["https://h/?token=REDACTED&sig=s", 'UrlParams [("token", "t"), ("sig", "<redacted>")]']);
  assert.equal(Url.flickShow(url.success), "https://h/?token=t&sig=REDACTED");
});

test("a URL that does not parse fails with Effect's messages", () => {
  assert.deepEqual(Url.flickFromString("nope"), { _tag: "Failure", failure: { _tag: tag("Http.Url.UrlError.UrlError"), _0: 'Invalid URL: "nope"' } });
  assert.deepEqual(Url.flickMake("not a url", UrlParams.empty, none), { _tag: "Failure", failure: { _tag: tag("Http.Url.UrlError.UrlError"), _0: "Invalid URL" } });
});

test("each setter gives a changed copy", () => {
  const url = Url.flickFromString("https://h.example/a");
  assert.equal(url._tag, "Success");
  const u = url.success;
  assert.deepEqual(
    [Url.setPort(80.5)(u).href, Url.setPort(-1)(u).href, Url.setPassword(redactedMake("p w"))(u).href, Url.setProtocol("ftp")(u).href, u.href],
    ["https://h.example:80/a", "https://h.example/a", "https://:p%20w@h.example/a", "ftp://h.example/a", "https://h.example/a"],
  );
  const query = Url.flickFromString("https://h/?a=1&b=2");
  assert.equal(query._tag, "Success");
  assert.equal(Url.modifyUrlParams(UrlParams.remove("a"))(query.success).href, "https://h/?b=2");
});

test("media types follow the extension of the last part of a path", () => {
  assert.deepEqual(
    ["dir/noext", "html", "dir/.html", "a/b.HTML", "x.mp3", "a\\b.json"].map(getType),
    [none, some("text/html"), none, some("text/html"), some("audio/mpeg"), some("application/json")],
  );
  assert.deepEqual(["Text/HTML; charset=utf-8", "audio/mp3", "nope/nope"].map(getExtension), [some("html"), some("mp3"), none]);
  assert.deepEqual(flickGetAllExtensions("image/jpeg"), some(["jpe", "jpeg", "jpg"]));
});

test("an entity tag is the size and the whole milliseconds of the time, in hex", async () => {
  const Generator = service("Http.Etag.Generator");
  const from = operation(Generator, "from_file_info", 1) as (info: unknown) => unknown;
  const exit = await root(() =>
    layerRun(flickLayer(Generator), () => [from({ size: 255n, mtime: some(4096.9) }), from({ size: 0n, mtime: none })]),
  );
  assert.deepEqual(exit.value, [
    { _tag: tag("Http.Etag.Etag.Strong"), _0: "ff-1000" },
    { _tag: tag("Http.Etag.Etag.Strong"), _0: "0-0" },
  ]);
  const weak = await root(() => layerRun(flickLayerWeak(Generator), () => from({ size: 16n, mtime: none })));
  assert.deepEqual(weak.value, { _tag: tag("Http.Etag.Etag.Weak"), _0: "10-0" });
});
