// Installs a released `flick` binary for the runner and puts it on `PATH`.
// `action.yml` at the repository root runs this file as a Node action:
//
//   - uses: bondiano/flick-lang@main
//     with:
//       version: latest        # or v0.1.0, or 0.1.0
//
// It downloads the archive and its `.sha256` from the release, checks the
// digest before unpacking anything, unpacks into `<install-dir>/<tag>/<target>`
// (reused when it is already there, so a cached directory costs no download),
// runs `flick --version` to prove the binary starts, and appends the
// directory to `GITHUB_PATH`. Nothing is written outside the install
// directory and the runner's temporary directory, and no input reaches a
// shell: the archive name, the URL and the version are validated and passed
// to `fetch` and `tar` as arguments.
//
// The tests in `install.test.mjs` run it against a local server:
//
//   node --test .github/actions/setup-flick/install.test.mjs

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  appendFileSync,
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** How many redirects a download may follow, and the hosts they may lead to. */
const REDIRECTS = 5;
const RELEASE_HOSTS = ["github.com", "githubusercontent.com"];

/** The repository whose releases carry the binaries, unless `repository` says another. */
export const DEFAULT_REPOSITORY = "bondiano/flick-lang";

/** The releases this action installs, by `process.platform` and `process.arch`. */
const TARGETS = {
  "darwin/x64": { target: "x86_64-apple-darwin", archive: "tar.gz", binary: "flick" },
  "darwin/arm64": { target: "aarch64-apple-darwin", archive: "tar.gz", binary: "flick" },
  "linux/x64": { target: "x86_64-unknown-linux-gnu", archive: "tar.gz", binary: "flick" },
  "linux/arm64": { target: "aarch64-unknown-linux-gnu", archive: "tar.gz", binary: "flick" },
  "win32/x64": { target: "x86_64-pc-windows-msvc", archive: "zip", binary: "flick.exe" },
  "win32/arm64": { target: "aarch64-pc-windows-msvc", archive: "zip", binary: "flick.exe" },
};

/** What this runner installs: its target triple, archive type and binary name. */
export const asset = (platform = process.platform, arch = process.arch) => {
  const found = TARGETS[`${platform}/${arch}`];
  if (found === undefined) {
    throw new Error(
      `no flick release for ${platform}/${arch}: the releases cover ` +
        `${Object.keys(TARGETS).join(", ")}; build it from source instead`,
    );
  }
  return {
    ...found,
    file: `flick-${found.target}.${found.archive}`,
  };
};

/**
 * The release tag `version` names: `latest` as is, a version with its `v`.
 * Anything else is refused before it reaches a URL or a path.
 */
export const tag = (version) => {
  const wanted = String(version ?? "").trim();
  if (wanted === "latest") return "latest";
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(wanted);
  if (match === null) {
    throw new Error(
      `\`version\` must be \`latest\`, a tag such as \`v0.1.0\` or a version such as ` +
        `\`0.1.0\`, not \`${wanted}\``,
    );
  }
  return `v${match[1]}.${match[2]}.${match[3]}`;
};

/** `owner/name`, as the release pages spell it. */
export const repository = (name) => {
  const wanted = String(name ?? "").trim();
  if (!/^[A-Za-z0-9][A-Za-z0-9-_.]*\/[A-Za-z0-9][A-Za-z0-9-_.]*$/.test(wanted)) {
    throw new Error(`\`repository\` must be \`owner/name\`, not \`${wanted}\``);
  }
  return wanted;
};

/** The digest a `.sha256` file records, as `sha256sum` writes it. */
export const recorded = (text, file) => {
  const line = String(text)
    .split("\n")
    .find((candidate) => candidate.trim() !== "");
  const match = /^([0-9a-fA-F]{64})(?:\s+\*?(.+))?$/.exec((line ?? "").trim());
  if (match === null) throw new Error(`\`${file}.sha256\` names no sha256 digest`);
  const named = match[2]?.trim();
  if (named !== undefined && named !== "" && named !== file) {
    throw new Error(`\`${file}.sha256\` is for \`${named}\`, not \`${file}\``);
  }
  return match[1].toLowerCase();
};

/** Whether `bytes` hash to `digest`. */
export const matches = (bytes, digest) =>
  createHash("sha256").update(bytes).digest("hex") === digest.toLowerCase();

/** The URL of an asset of the release `tag` of `repo`. */
export const assetUrl = (base, repo, tagName, file) =>
  `${base.replace(/\/+$/, "")}/${repo}/releases/download/${tagName}/${file}`;

/** The URL GitHub redirects to the asset of the latest release. */
export const latestUrl = (base, repo, file) =>
  `${base.replace(/\/+$/, "")}/${repo}/releases/latest/download/${file}`;

/** The message of a failed response, without its body's noise. */
const failure = (what, response) =>
  new Error(`${what} failed: HTTP ${response.status} ${response.statusText}`.trim());

/** Whether the release and its archive may be served from `address`. */
const serves = (address, base) => {
  const { hostname } = new URL(address);
  if (hostname === new URL(base).hostname) return true;
  return RELEASE_HOSTS.some((host) => hostname === host || hostname.endsWith(`.${host}`));
};

/**
 * `fetch`, following at most `REDIRECTS` redirects, and only to a release
 * host: the archive is downloaded over a redirect whose target decides whether
 * its bytes are trusted. Returns the response and the address it came from,
 * which names the release when `latest` led here.
 */
const follow = async (fetch, address, { headers, base }) => {
  let current = address;
  for (let hop = 0; hop <= REDIRECTS; hop += 1) {
    const response = await fetch(current, { headers, redirect: "manual" });
    const next = response.headers.get("location");
    if (response.status < 300 || response.status > 399 || next === null) {
      return { response, address: current };
    }
    const target = new URL(next, current).href;
    if (!serves(target, base)) {
      throw new Error(
        `the download redirects to ${new URL(target).hostname}, which is neither ` +
          `${new URL(base).hostname} nor a GitHub releases host`,
      );
    }
    current = target;
    // credentials belong to the API host: no header survives a redirect
    headers = undefined;
  }
  throw new Error(`the download redirects more than ${REDIRECTS} times`);
};

/**
 * The tag `latest` names: the newest compiler release, `v<major>.<minor>.<patch>`.
 * A package release, such as `std-v0.1.0`, is a release of its own, and
 * `releases/latest` can name one of them, so the list is filtered rather than
 * trusted. Without the API (a rate limit), the download of `latest` still
 * names the tag it leads to.
 */
const latest = async ({ fetch, api, base, repo, file, token }) => {
  if (api !== "") {
    const { response } = await follow(fetch, `${api.replace(/\/+$/, "")}/${repo}/releases?per_page=30`, {
      base: api,
      headers: {
        accept: "application/vnd.github+json",
        "user-agent": "flick-action",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
    });
    if (response.ok) {
      const releases = await response.json();
      const compiler = releases
        .filter((release) => release.draft !== true && release.prerelease !== true)
        .map((release) => release.tag_name ?? "")
        .filter((name) => /^v\d+\.\d+\.\d+$/.test(name));
      if (compiler.length === 0) {
        throw new Error(`\`${repo}\` publishes no compiler release (\`v<major>.<minor>.<patch>\`)`);
      }
      return tag(compiler[0]);
    }
    // a rate limit is not a reason to fail: the download names the tag too
    if (response.status !== 403 && response.status !== 429) {
      throw failure("listing the releases", response);
    }
  }
  const { response, address } = await follow(fetch, latestUrl(base, repo, file), { base });
  if (!response.ok) throw failure("looking up the latest release", response);
  const seen = /\/releases\/download\/([^/]+)\//.exec(address);
  if (seen === null) throw new Error(`the download of \`${file}\` names no release tag`);
  await response.arrayBuffer().catch(() => undefined);
  return tag(seen[1]);
};

/** Downloads and checks the archive of `tagName`; returns its bytes. */
const download = async ({ fetch, base, repo, tagName, file, log }) => {
  // no credentials: a release archive is public, and nothing that leaves the
  // API host carries the token
  const archive = await follow(fetch, assetUrl(base, repo, tagName, file), { base });
  if (!archive.response.ok) throw failure(`downloading ${file}`, archive.response);
  const bytes = Buffer.from(await archive.response.arrayBuffer());
  const sums = await follow(fetch, `${assetUrl(base, repo, tagName, file)}.sha256`, { base });
  if (!sums.response.ok) throw failure(`downloading ${file}.sha256`, sums.response);
  const digest = recorded(await sums.response.text(), file);
  if (!matches(bytes, digest)) {
    throw new Error(
      `the sha256 of ${file} is ${createHash("sha256").update(bytes).digest("hex")}, ` +
        `but the release records ${digest}: nothing was installed`,
    );
  }
  log(`${file}: sha256 ${digest}`);
  return bytes;
};

/**
 * Unpacks `bytes` into a directory of its own and returns the file `binary`,
 * leaving the caller to remove the directory. Only that member is extracted,
 * so nothing else an archive names, such as `../x`, is ever written; a member
 * that is not a regular file, such as a symlink, is refused.
 */
const unpack = (bytes, file, binary) => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-action-"));
  try {
    const archive = path.join(dir, file);
    writeFileSync(archive, bytes, { mode: 0o600 });
    const flags = file.endsWith(".zip") ? ["-xf"] : ["-xzf"];
    execFileSync("tar", [...flags, archive, "-C", dir, binary], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    const unpacked = path.join(dir, binary);
    const info = lstatSync(unpacked, { throwIfNoEntry: false });
    if (info === undefined) {
      throw new Error(`the archive holds no \`${binary}\`: nothing was installed`);
    }
    if (!info.isFile()) {
      throw new Error(`the archive's \`${binary}\` is not a regular file: nothing was installed`);
    }
    // an archive carries no usable mode for the runner, and a zip carries none
    chmodSync(unpacked, 0o755);
    return { unpacked, dir };
  } catch (error) {
    rmSync(dir, { recursive: true, force: true });
    throw error;
  }
};

/** The line `flick --version` prints, checked against the release it came from. */
export const confirms = (binary, tagName) => {
  const printed = execFileSync(binary, ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  const match = /^flick (\d+\.\d+\.\d+)$/.exec(printed);
  if (match === null) throw new Error(`\`${binary} --version\` printed \`${printed}\``);
  if (tagName !== "latest" && tagName !== `v${match[1]}`) {
    throw new Error(
      `the release ${tagName} gave flick ${match[1]}: the archive is not the release's`,
    );
  }
  return printed;
};

/** Writes `line` at the end of the runner's `file` once; returns whether it did. */
const append = (file, line) => {
  if (file === undefined || file === "") return false;
  const existing = existsSync(file) ? readFileSync(file, "utf8") : "";
  if (existing.split("\n").includes(line)) return true;
  appendFileSync(file, `${existing === "" || existing.endsWith("\n") ? "" : "\n"}${line}\n`);
  return true;
};

/**
 * Installs and returns `{ version, directory, printed }`. Every input is read
 * from the environment, as an action does; the tests pass `env`, `fetch` and
 * `log` instead.
 */
export const install = async ({ env = process.env, fetch = globalThis.fetch, log = console.log } = {}) => {
  const wanted = tag(env.INPUT_VERSION ?? "latest");
  const repo = repository(env.INPUT_REPOSITORY ?? DEFAULT_REPOSITORY);
  const platform = env.FLICK_ACTION_PLATFORM ?? process.platform;
  const arch = env.FLICK_ACTION_ARCH ?? process.arch;
  const found = asset(platform, arch);
  const base = env.FLICK_ACTION_BASE ?? "https://github.com";
  const api = env.FLICK_ACTION_API ?? "https://api.github.com/repos";
  const root =
    (env.INPUT_INSTALL_DIR ?? "") !== ""
      ? path.resolve(env.INPUT_INSTALL_DIR)
      : path.join(env.RUNNER_TOOL_CACHE ?? tmpdir(), "flick");
  const tagName =
    wanted === "latest"
      ? await latest({ fetch, api, base, repo, file: found.file, token: env.INPUT_TOKEN })
      : wanted;
  log(`flick ${tagName} for ${found.target}`);

  const directory = path.join(root, tagName, found.target);
  const binary = path.join(directory, found.binary);
  let printed;
  if (existsSync(binary)) {
    log(`${binary} is already installed`);
    printed = confirms(binary, tagName);
  } else {
    const bytes = await download({ fetch, base, repo, tagName, file: found.file, log });
    const { unpacked, dir } = unpack(bytes, found.file, found.binary);
    try {
      // a binary that does not start, or that is another release, is never
      // installed: it is run from the temporary directory first
      printed = confirms(unpacked, tagName);
      mkdirSync(directory, { recursive: true });
      const staged = `${binary}.${process.pid}.part`;
      try {
        copyFileSync(unpacked, staged);
        chmodSync(staged, 0o755);
        // `flick` appears whole, or not at all
        renameSync(staged, binary);
      } catch (error) {
        rmSync(staged, { force: true });
        throw error;
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
  log(printed);

  append(env.GITHUB_PATH, directory);
  append(env.GITHUB_OUTPUT, `version=${tagName}`);
  append(env.GITHUB_OUTPUT, `path=${directory}`);
  if (env.GITHUB_OUTPUT === undefined || env.GITHUB_OUTPUT === "") {
    log(`::warning::GITHUB_PATH is unset: add ${directory} to PATH yourself`);
  }
  return { version: tagName, directory, printed };
};

/** `message` without `token` in it: a credential is never printed. */
export const redacts = (message, token) =>
  token === undefined || token === "" ? message : String(message).split(token).join("***");

/** Runs the action: a failure is the step's failure, with a message the log shows. */
const invoked =
  process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invoked) {
  const token = process.env.INPUT_TOKEN ?? "";
  try {
    await install();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`::error::${redacts(message, token)}`);
    process.exit(1);
  }
}
