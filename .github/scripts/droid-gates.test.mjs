// Exercise the actual workflow shell gates without GitHub credentials or Droid.
//   node --test .github/scripts/droid-gates.test.mjs
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

const wiki = readFileSync(new URL("../workflows/droid-wiki-refresh.yml", import.meta.url), "utf8");
const review = readFileSync(new URL("../workflows/droid-review.yml", import.meta.url), "utf8");

// These workflows use named steps with literal run blocks. Keep the tests on
// those blocks rather than a second implementation of the scheduling policy.
function step(workflow, name) {
  const start = workflow.indexOf(`      - name: ${name}\n`);
  assert.notEqual(start, -1, `missing step: ${name}`);
  const rest = workflow.slice(start);
  const end = rest.slice(1).search(/^      - |^  \S/m);
  return end < 0 ? rest : rest.slice(0, end + 1);
}

function script(workflow, name) {
  const match = step(workflow, name).match(/        run: \|\n((?:          .*\n|\n)+)/);
  assert.ok(match, `missing shell script: ${name}`);
  return match[1].replace(/^          /gm, "");
}

function fixture(t) {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-droid-gates-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const output = path.join(dir, "output");
  const summary = path.join(dir, "summary");
  writeFileSync(output, "");
  writeFileSync(summary, "");
  return {
    dir,
    output,
    summary,
    env: { ...process.env, GITHUB_OUTPUT: output, GITHUB_STEP_SUMMARY: summary },
  };
}

function run(f, source, env = {}) {
  const result = spawnSync("bash", ["-euo", "pipefail", "-c", source], {
    cwd: f.dir,
    env: { ...f.env, ...env },
    encoding: "utf8",
  });
  return {
    ...result,
    output: readFileSync(f.output, "utf8"),
    summary: readFileSync(f.summary, "utf8"),
  };
}

function reviewFixture(t, pr, status = 0) {
  const f = fixture(t);
  const payload = path.join(f.dir, "pr.json");
  writeFileSync(payload, JSON.stringify(pr));
  writeFileSync(path.join(f.dir, "gh"), `#!/bin/sh
printf '%s\\n' "$@" > "$DROID_GH_ARGS"
cat "$DROID_PR_FIXTURE"
exit "$DROID_GH_STATUS"
`, { mode: 0o755 });
  f.env = {
    ...f.env,
    PATH: `${f.dir}${path.delimiter}${process.env.PATH}`,
    GITHUB_REPOSITORY: "owner/repo",
    PR_NUMBER: "42",
    DROID_GH_ARGS: path.join(f.dir, "gh-args"),
    DROID_PR_FIXTURE: payload,
    DROID_GH_STATUS: String(status),
  };
  return f;
}

test("review counts additions plus deletions: 999 and 1000 pass, 1001 skips", (t) => {
  for (const [deletions, allowed] of [[499, true], [500, true], [501, false]]) {
    const f = reviewFixture(t, { additions: 500, deletions, draft: false });
    const result = run(f, script(review, "Check review size"));
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.output, new RegExp(`should_review=${allowed}\\n$`));
    assert.match(result.summary, new RegExp(String(500 + deletions)));
    assert.equal(readFileSync(f.env.DROID_GH_ARGS, "utf8"), "api\nrepos/owner/repo/pulls/42\n");
  }
});

test("a PR that is now a draft is not reviewed", (t) => {
  const result = run(reviewFixture(t, { additions: 1, deletions: 0, draft: true }), script(review, "Check review size"));
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.output, /should_review=true/);
  assert.match(result.summary, /draft/i);
});

test("very large sizes cannot overflow the shell comparison and allow a review", (t) => {
  const result = run(reviewFixture(t, { additions: 1e20, deletions: 0, draft: false }), script(review, "Check review size"));
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.output, /should_review=true/);
  assert.match(result.summary, /exceeds the 1000-line limit/);
});

test("API failure and missing or invalid size fail closed", (t) => {
  for (const [pr, status] of [
    [{ additions: 1, deletions: 0, draft: false }, 1],
    [{ draft: false }, 0],
    [{ additions: null, deletions: 0, draft: false }, 0],
    [{ additions: "10", deletions: 0, draft: false }, 0],
    [{ additions: -1, deletions: 0, draft: false }, 0],
    [{ additions: 0.5, deletions: 0, draft: false }, 0],
    [{ additions: 1, deletions: 0 }, 0],
  ]) {
    const result = run(reviewFixture(t, pr, status), script(review, "Check review size"));
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.output, /should_review=true/);
    assert.match(result.summary, /could not determine/i);
  }
});

function git(f, args, env = {}) {
  return execFileSync("git", args, { cwd: f.dir, env: { ...f.env, ...env }, encoding: "utf8" });
}

function commit(f, when) {
  git(f, ["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false",
    "-c", "core.hooksPath=/dev/null", "commit", "--allow-empty", "-qm", "fixture"], {
    GIT_AUTHOR_DATE: when,
    GIT_COMMITTER_DATE: when,
  });
}

function wikiFixture(t, now, commits = [now]) {
  const f = fixture(t);
  git(f, ["init", "-q"]);
  for (const when of commits) commit(f, when);
  // Freeze only the wall clock; the gate still queries a real Git history.
  writeFileSync(path.join(f.dir, "date"), `#!/bin/sh
test "$*" = '-u +%s' || exit 1
echo "$DROID_NOW"
`, { mode: 0o755 });
  f.env.PATH = `${f.dir}${path.delimiter}${process.env.PATH}`;
  f.env.DROID_NOW = String(Date.parse(now) / 1000);
  return f;
}

test("wiki alternates days across month, year and leap-day boundaries", (t) => {
  for (const [day, allowed] of [
    ["2026-11-13", true], ["2026-11-14", false], ["2026-11-15", true],
    ["2026-11-30", false], ["2026-12-01", true],
    ["2026-12-31", true], ["2027-01-01", false],
    ["2028-02-28", true], ["2028-02-29", false], ["2028-03-01", true],
  ]) {
    const f = wikiFixture(t, `${day}T03:17:00Z`);
    const result = run(f, script(wiki, "Check refresh eligibility"));
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.output, new RegExp(`should_refresh=${allowed}\\n$`), day);
    assert.ok(result.summary.trim());
  }
});

test("wiki requires a commit in the last 48 hours on the checked-out branch", (t) => {
  for (const [when, allowed] of [
    ["2026-11-11T03:17:01Z", true],
    ["2026-11-11T03:17:00Z", true],
    ["2026-11-11T03:16:59Z", false],
  ]) {
    const f = wikiFixture(t, "2026-11-13T03:17:00Z", [when]);
    const result = run(f, script(wiki, "Check refresh eligibility"));
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.output, new RegExp(`should_refresh=${allowed}\\n$`));
    if (!allowed) assert.match(result.summary, /no commits/i);
  }
});

test("wiki does not miss a recent ancestor behind a backdated commit", (t) => {
  const f = wikiFixture(t, "2026-11-13T03:17:00Z", ["2026-11-12T10:00:00Z", "2026-11-01T00:00:00Z"]);
  const result = run(f, script(wiki, "Check refresh eligibility"));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.output, /should_refresh=true\n$/);
});

test("wiki does not count recent commits on other branches", (t) => {
  const f = wikiFixture(t, "2026-11-13T03:17:00Z", ["2026-11-01T00:00:00Z"]);
  const head = git(f, ["rev-parse", "HEAD"]).trim();
  git(f, ["checkout", "-qb", "unmerged"]);
  commit(f, "2026-11-13T00:00:00Z");
  git(f, ["checkout", "-q", "--detach", head]);
  const result = run(f, script(wiki, "Check refresh eligibility"));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.output, /should_refresh=false\n$/);
});

test("wiki skips Git queries on off-days and fails closed on Git errors", (t) => {
  for (const [day, status] of [["2026-11-14", 0], ["2026-11-13", 2]]) {
    const f = wikiFixture(t, `${day}T03:17:00Z`);
    writeFileSync(path.join(f.dir, "git"), "#!/bin/sh\nexit 2\n", { mode: 0o755 });
    const result = run(f, script(wiki, "Check refresh eligibility"));
    assert.equal(result.status, status, result.stderr);
    assert.doesNotMatch(result.output, /should_refresh=true/);
  }
});

test("workflow wiring gates all paid work and artifact retention", () => {
  assert.match(wiki, /schedule:\n    - cron: '17 3 \* \* \*'/);
  assert.doesNotMatch(wiki, /^  (push|pull_request|workflow_dispatch):/m);
  for (const name of ["Install Factory Droid", "Generate wiki", "Keep the generated wiki as a workflow artifact"]) {
    assert.match(step(wiki, name), /if: steps\.gate\.outputs\.should_refresh == 'true'/);
  }
  assert.match(review, /if: github\.event\.pull_request\.draft == false/);
  for (const name of ["Checkout repository", "Run Droid Auto Review"]) {
    assert.match(step(review, name), /if: steps\.gate\.outputs\.should_review == 'true'/);
  }
  assert.ok(review.indexOf("name: Check review size") < review.indexOf("name: Checkout repository"));
});

// The upstream local-only wiki policy must survive restoration of scheduling.
test("scheduled wiki does not restore repository publication", () => {
  assert.match(wiki, /^permissions:\n  contents: read\n/m);
  assert.doesNotMatch(wiki, /^  wiki-publish:/m);
  assert.doesNotMatch(wiki, /contents: write|pull-requests: write|create-pull-request/);
  assert.doesNotMatch(wiki, /WIKI_BRANCH|git checkout FETCH_HEAD/);
});
