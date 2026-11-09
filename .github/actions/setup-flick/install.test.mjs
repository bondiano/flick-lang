// The installer's tests, against a local server: what it accepts and refuses,
// which archive each runner gets, that a wrong sha256 installs nothing, that a
// second run reuses what is there, and that PATH and outputs are written.
//
//   node --test .github/actions/setup-flick/install.test.mjs

import assert from "node:assert/strict";
import { execFile as execFileCallback, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { asset, install, latest, matches, recorded, redacts, repository, tag } from "./install.mjs";

const execFile = promisify(execFileCallback);

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** A tar.gz of a `flick` script that answers `--version` as a release does. */
const archive = (version) => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-action-fixture-"));
  const binary = path.join(dir, "flick");
  writeFileSync(binary, `#!/bin/sh\necho "flick ${version}"\n`, { mode: 0o755 });
  const file = path.join(dir, "flick.tar.gz");
  execFileSync("tar", ["-czf", file, "-C", dir, "flick"], { stdio: "ignore" });
  return readFileSync(file);
};

const TARGET = "x86_64-unknown-linux-gnu";
const FILE = `flick-${TARGET}.tar.gz`;

const good = archive("0.1.0");
const corrupt = archive("0.1.1");

/** A tar.gz whose `flick` is a symlink, which no release makes. */
const symlinked = (() => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-action-fixture-"));
  execFileSync("ln", ["-s", "/etc/passwd", path.join(dir, "flick")], { stdio: "ignore" });
  const file = path.join(dir, "archive.tar.gz");
  execFileSync("tar", ["-czf", file, "-C", dir, "flick"], { stdio: "ignore" });
  return readFileSync(file);
})();

/** A tar.gz that also names `../escape`, which must never be written. */
const escaping = (() => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-action-fixture-"));
  mkdirSync(path.join(dir, "inner"));
  writeFileSync(path.join(dir, "inner", "flick"), '#!/bin/sh\necho "flick 0.1.3"\n', {
    mode: 0o755,
  });
  writeFileSync(path.join(dir, "escape"), "escaped\n");
  const file = path.join(dir, "archive.tar.gz");
  execFileSync(
    "tar",
    ["-czf", file, "-P", "-C", path.join(dir, "inner"), "flick", "../escape"],
    { stdio: "ignore" },
  );
  return readFileSync(file);
})();

/** The members of the archive `bytes`. */
const members = (bytes) => {
  const file = path.join(mkdtempSync(path.join(tmpdir(), "flick-action-fixture-")), "archive.tar.gz");
  writeFileSync(file, bytes);
  return execFileSync("tar", ["-tf", file], { encoding: "utf8" });
};

/** What the fixture server answers, by URL path: a body, or a redirect. */
const routes = {
  [`/owner/repo/releases/download/v0.1.0/${FILE}`]: { body: good },
  [`/owner/repo/releases/download/v0.1.0/${FILE}.sha256`]: {
    body: Buffer.from(`${sha(good)}  ${FILE}\n`),
  },
  // a release whose sha256 does not describe its archive
  [`/owner/repo/releases/download/v0.1.1/${FILE}`]: { body: corrupt },
  [`/owner/repo/releases/download/v0.1.1/${FILE}.sha256`]: {
    body: Buffer.from(`${sha(good)}  ${FILE}\n`),
  },
  // the newest release may be a package's, `std-v0.1.0`, which is no compiler
  "/api/owner/repo/releases?per_page=30": {
    body: Buffer.from(
      JSON.stringify([
        { tag_name: "std-v0.1.0", draft: false, prerelease: false },
        { tag_name: "v0.2.0-rc.1", draft: false, prerelease: true },
        { tag_name: "v0.1.0", draft: false, prerelease: false },
      ]),
    ),
    type: "application/json",
  },
  "/api-empty/owner/repo/releases?per_page=30": {
    body: Buffer.from(JSON.stringify([{ tag_name: "std-v0.1.0", draft: false }])),
    type: "application/json",
  },
  "/api-paged/owner/repo/releases?per_page=30": {
    body: Buffer.from(JSON.stringify(Array.from({ length: 30 }, (_, i) => ({ tag_name: `std-v0.1.${i}` })))),
    type: "application/json",
  },
  "/api-paged/owner/repo/releases?per_page=30&page=2": {
    body: Buffer.from(JSON.stringify([
      { tag_name: "v0.1.9" },
      { tag_name: "v0.2.0" },
    ])),
    type: "application/json",
  },
  "/api-unsorted/owner/repo/releases?per_page=30": {
    body: Buffer.from(JSON.stringify([
      { tag_name: "v0.1.9" },
      { tag_name: "v0.2.0" },
    ])),
    type: "application/json",
  },
  // the API refuses, as it does over its rate limit; the download names the tag
  "/api-limited/owner/repo/releases?per_page=30": { status: 403 },
  [`/owner/repo/releases/latest/download/${FILE}`]: {
    redirect: `/owner/repo/releases/download/v0.1.0/${FILE}`,
  },
  // an archive whose `flick` is a symlink, and one that names a file outside
  // the directory it is unpacked into
  [`/owner/repo/releases/download/v0.1.2/${FILE}`]: { body: symlinked },
  [`/owner/repo/releases/download/v0.1.2/${FILE}.sha256`]: { body: Buffer.from(`${sha(symlinked)}  ${FILE}\n`) },
  [`/owner/repo/releases/download/v0.1.3/${FILE}`]: { body: escaping },
  [`/owner/repo/releases/download/v0.1.3/${FILE}.sha256`]: { body: Buffer.from(`${sha(escaping)}  ${FILE}\n`) },
  // a redirect the download must not follow: it leaves the release host
  [`/owner/repo/releases/download/v0.1.4/${FILE}`]: { redirect: "http://elsewhere.invalid/flick.tar.gz" },
};

const requests = { count: 0, archives: 0, paths: [] };
let server;
let base;

before(async () => {
  server = createServer((request, response) => {
    requests.count += 1;
    const url = request.url ?? "";
    requests.paths.push(url);
    const route = routes[url];
    if (route === undefined) {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("not found");
      return;
    }
    if (route.redirect !== undefined) {
      response.writeHead(302, { location: route.redirect });
      response.end();
      return;
    }
    if (url.endsWith(".tar.gz")) requests.archives += 1;
    response.writeHead(route.status ?? 200, { "content-type": route.type ?? "application/octet-stream" });
    response.end(route.body);
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  base = `http://127.0.0.1:${server.address().port}`;
});

after(() => server.close());

/** The environment an action step gives the installer, with the server in place of GitHub. */
const environment = (overrides = {}) => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-action-"));
  return {
    dir,
    env: {
      INPUT_VERSION: "latest",
      INPUT_REPOSITORY: "owner/repo",
      INPUT_INSTALL_DIR: path.join(dir, "tool-cache"),
      INPUT_TOKEN: "",
      GITHUB_PATH: path.join(dir, "github_path"),
      GITHUB_OUTPUT: path.join(dir, "github_output"),
      FLICK_ACTION_BASE: base,
      FLICK_ACTION_API: `${base}/api`,
      FLICK_ACTION_PLATFORM: "linux",
      FLICK_ACTION_ARCH: "x64",
      ...overrides,
    },
  };
};

const quiet = () => {};

test("every runner the releases cover has an archive, and others have a message", () => {
  const targets = [
    ["darwin", "x64", "x86_64-apple-darwin", "tar.gz", "flick"],
    ["darwin", "arm64", "aarch64-apple-darwin", "tar.gz", "flick"],
    ["linux", "x64", "x86_64-unknown-linux-gnu", "tar.gz", "flick"],
    ["linux", "arm64", "aarch64-unknown-linux-gnu", "tar.gz", "flick"],
    ["win32", "x64", "x86_64-pc-windows-msvc", "zip", "flick.exe"],
    ["win32", "arm64", "aarch64-pc-windows-msvc", "zip", "flick.exe"],
  ];
  for (const [platform, arch, triple, type, binary] of targets) {
    const found = asset(platform, arch);
    assert.equal(found.target, triple);
    assert.equal(found.archive, type);
    assert.equal(found.binary, binary);
    assert.equal(found.file, `flick-${triple}.${type}`);
  }
  assert.throws(() => asset("linux", "ia32"), /no flick release for linux\/ia32/);
  assert.throws(() => asset("freebsd", "x64"), /no flick release/);
});

test("a version is a tag, a version, or latest, and nothing else", () => {
  assert.equal(tag("latest"), "latest");
  assert.equal(tag("v0.1.0"), "v0.1.0");
  assert.equal(tag("0.1.0"), "v0.1.0");
  assert.equal(tag("  v1.2.3  "), "v1.2.3");
  assert.equal(tag("v1.2.3\n"), "v1.2.3", "a YAML input may carry a trailing newline");
  for (const refused of ["", "v1", "1.2", "v1.2.3.4", "latest; rm -rf /", "../../etc", "nightly"]) {
    assert.throws(() => tag(refused), /`version` must be/, refused);
  }
});

test("a repository is owner/name, and nothing else", () => {
  assert.equal(repository("bondiano/flick-lang"), "bondiano/flick-lang");
  assert.equal(repository("owner/repo.js_2-x"), "owner/repo.js_2-x");
  for (const refused of ["", "flick-lang", "a/b/c", "../x", "own er/repo", "-bad/repo", "owner/"]) {
    assert.throws(() => repository(refused), /`repository` must be/, refused);
  }
});

test("a sha256 file names its archive, and a digest matches only its bytes", () => {
  assert.equal(recorded(`${sha(good)}  ${FILE}\n`, FILE), sha(good));
  assert.equal(recorded(`  ${sha(good).toUpperCase()} *${FILE}  \n`, FILE), sha(good));
  assert.throws(() => recorded("", FILE), /names no sha256 digest/);
  assert.throws(() => recorded("abc  x\n", FILE), /names no sha256 digest/);
  assert.throws(() => recorded(`${sha(good)}  other.tar.gz\n`, FILE), /is for `other.tar.gz`/);
  assert.equal(matches(good, sha(good)), true);
  assert.equal(matches(corrupt, sha(good)), false);
});

test("the latest release installs, is reused, and lands on PATH", async () => {
  const { env } = environment();
  const first = await install({ env, log: quiet });
  assert.equal(first.version, "v0.1.0");
  const binary = path.join(env.INPUT_INSTALL_DIR, "v0.1.0", TARGET, "flick");
  assert.equal(first.directory, path.dirname(binary));
  assert.ok(existsSync(binary), binary);
  assert.ok(readFileSync(binary, "utf8").startsWith("#!/bin/sh"));
  assert.equal(readFileSync(env.GITHUB_PATH, "utf8").trim(), first.directory);
  const outputs = readFileSync(env.GITHUB_OUTPUT, "utf8");
  assert.match(outputs, /^version=v0\.1\.0$/m);
  assert.ok(outputs.includes(`path=${first.directory}`));
  assert.ok(requests.paths.includes(`/owner/repo/releases/download/v0.1.0/${FILE}`));

  // the second run asks for the tag, finds the binary and downloads nothing
  const archives = requests.archives;
  const second = await install({ env, log: quiet });
  assert.equal(second.version, "v0.1.0");
  assert.equal(requests.archives, archives, "a cached binary is not downloaded again");
  assert.equal(readFileSync(env.GITHUB_PATH, "utf8").trim(), first.directory, "one PATH line");
});

test("a pinned version needs no lookup", async () => {
  const { env } = environment({ INPUT_VERSION: "0.1.0" });
  const before = requests.count;
  const found = await install({ env, log: quiet });
  assert.equal(found.version, "v0.1.0");
  assert.equal(requests.count - before, 2, "the archive and its sha256, and no API call");
});

test("a rate-limited API still names the tag, from where the download leads", async () => {
  const { env, dir } = environment({ FLICK_ACTION_API: `${base}/api-limited` });
  const found = await install({ env, log: quiet });
  assert.equal(found.version, "v0.1.0");
  assert.ok(existsSync(path.join(dir, "tool-cache", "v0.1.0", TARGET, "flick")));
});

test("latest finds compiler releases past a full page and chooses the highest version", async () => {
  const lookup = (api) => latest({ fetch, api, base, repo: "owner/repo", file: FILE, token: "" });
  assert.equal(await lookup(`${base}/api-paged`), "v0.2.0");
  assert.equal(await lookup(`${base}/api-unsorted`), "v0.2.0");
  assert.equal(await lookup(`${base}/api-empty`), "v0.1.0", "the download supplies the fallback tag");
});

test("missing GITHUB_PATH warns even when GITHUB_OUTPUT is set", async () => {
  const { env } = environment({ INPUT_VERSION: "v0.1.0", GITHUB_PATH: undefined });
  const lines = [];
  await install({ env, log: (line) => lines.push(line) });
  assert.ok(lines.some((line) => line.includes("::warning::GITHUB_PATH is unset")));
  assert.match(readFileSync(env.GITHUB_OUTPUT, "utf8"), /^version=v0\.1\.0$/m);
});

test("a wrong sha256 installs nothing", async () => {
  const { env } = environment({ INPUT_VERSION: "v0.1.1" });
  await assert.rejects(install({ env, log: quiet }), /nothing was installed/);
  assert.equal(existsSync(path.join(env.INPUT_INSTALL_DIR, "v0.1.1", TARGET, "flick")), false);
  assert.equal(existsSync(env.GITHUB_PATH), false, "no PATH line is written");
  assert.equal(existsSync(env.GITHUB_OUTPUT), false, "no output is written");
});

test("a release that is not there says so", async () => {
  const { env } = environment({ INPUT_VERSION: "v9.9.9" });
  await assert.rejects(install({ env, log: quiet }), /downloading flick-.* failed: HTTP 404/);
});

test("an unsupported runner is refused before anything is downloaded", async () => {
  const { env } = environment({ FLICK_ACTION_PLATFORM: "aix", FLICK_ACTION_ARCH: "ppc64" });
  const before = requests.count;
  await assert.rejects(install({ env, log: quiet }), /no flick release for aix\/ppc64/);
  assert.equal(requests.count, before);
});

test("a redirect that leaves the release hosts is refused", async () => {
  const { env } = environment({ INPUT_VERSION: "v0.1.4" });
  await assert.rejects(install({ env, log: quiet }), /redirects to elsewhere\.invalid/);
  assert.equal(existsSync(env.GITHUB_PATH), false, "no PATH line is written");
});

test("an archive's symlink is not installed", async () => {
  const { env } = environment({ INPUT_VERSION: "v0.1.2" });
  await assert.rejects(install({ env, log: quiet }), /`flick` is not a regular file/);
  assert.equal(existsSync(path.join(env.INPUT_INSTALL_DIR, "v0.1.2", TARGET, "flick")), false);
  assert.equal(existsSync(env.GITHUB_PATH), false, "no PATH line is written");
});

test("an archive that names a file above itself writes nothing but the binary", async () => {
  assert.match(members(escaping), /\.\.\/escape/, "the fixture names `../escape`");
  const { env } = environment({ INPUT_VERSION: "v0.1.3" });
  const { directory } = await install({ env, log: quiet });
  assert.deepEqual(readdirSync(env.INPUT_INSTALL_DIR), ["v0.1.3"], "only the release's directory");
  assert.deepEqual(readdirSync(directory), ["flick"], "only the binary");
});

test("a failed step prints no token", async () => {
  const { env } = environment({
    INPUT_VERSION: "v9.9.9",
    INPUT_TOKEN: "ghs_the_runner_token",
  });
  // async, so this process keeps answering the server the child asks
  const finished = await execFile(process.execPath, [
    fileURLToPath(new URL("./install.mjs", import.meta.url)),
  ], { env, encoding: "utf8" }).catch((error) => error);
  assert.equal(finished.code, 1, finished.stderr);
  assert.match(finished.stderr, /HTTP 404/);
  assert.equal(finished.stderr.includes("ghs_the_runner_token"), false, finished.stderr);
});

test("a message that repeats the token loses it", () => {
  assert.equal(redacts("GET https://x/ghs_t?token=ghs_t", "ghs_t"), "GET https://x/***?token=***");
  assert.equal(redacts("no credential here", ""), "no credential here");
});
