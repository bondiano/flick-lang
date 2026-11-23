// Extract and instrument the actual pruning method at two revisions.
// Counts path comparisons (old), or inserts + lookups (new), not hash probes.
// The standalone allocator reports bytes requested only during retain_loaded.
// Usage: node tools/perf/module-cache-counts.mjs <before-ref> [after-ref=HEAD]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const [before, after = "HEAD"] = process.argv.slice(2);
if (!before) throw new Error("usage: module-cache-counts.mjs <before-ref> [after-ref=HEAD]");
const directory = mkdtempSync(path.join(tmpdir(), "flick-prune-counts-"));
try {
  for (const ref of [before, after]) {
    const sha = execFileSync("git", ["rev-parse", "--verify", `${ref}^{commit}`], { cwd: root, encoding: "utf8" }).trim();
    const source = execFileSync("git", ["show", `${sha}:flick-core/src/project/load/checking.rs`], { cwd: root, encoding: "utf8" });
    const method = source.indexOf("fn retain_loaded(");
    assert.ok(method >= 0, "retain_loaded exists at the requested revision");
    const start = source.indexOf("{", method);
    let end = start + 1;
    let depth = 1;
    while (depth > 0 && end < source.length) {
      if (source[end] === "{") depth++;
      if (source[end] === "}") depth--;
      end++;
    }
    assert.equal(depth, 0);
    let body = source.slice(start, end);
    if (body.includes("modules.iter().any(")) {
      assert.equal(body.match(/m\.path == \*path/g)?.length, 2);
      body = body.replaceAll("m.path == *path", "{ OPERATIONS.fetch_add(1, Ordering::Relaxed); m.path == *path }");
    } else {
      assert.equal(body.match(/map\(\|m\| m\.path\.as_path\(\)\)/g)?.length, 1);
      assert.equal(body.match(/paths\.contains\(path\.as_path\(\)\)/g)?.length, 2);
      body = body.replace("map(|m| m.path.as_path())", "map(|m| { OPERATIONS.fetch_add(1, Ordering::Relaxed); m.path.as_path() })");
      body = body.replaceAll("paths.contains(path.as_path())", "{ OPERATIONS.fetch_add(1, Ordering::Relaxed); paths.contains(path.as_path()) }");
    }
    const rust = `
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static OPERATIONS: AtomicUsize = AtomicUsize::new(0);
struct Allocator;
// This instrumentation is isolated from the compiler and enabled only while
// pruning. Forward every allocation/deallocation to Rust's system allocator.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) { BYTES.fetch_add(layout.size(), Ordering::Relaxed); }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
struct Loaded { path: PathBuf }
struct ModuleCache { entries: HashMap<PathBuf, ()>, parses: HashMap<PathBuf, ()> }
impl ModuleCache { fn retain_loaded(&mut self, modules: &[Loaded]) ${body} }
fn main() {
    println!("case,modules,cached_entries,operations,temporary_requested_bytes");
    for n in [0usize, 10, 100, 1000] {
        for case in ["warm", "removed", "empty-caches"] {
            let loaded = if case == "removed" { n.saturating_sub(1) } else { n };
            let modules: Vec<_> = (0..loaded).map(|i| Loaded { path: format!("/p/M{i}.fl").into() }).collect();
            let entries: HashMap<_, _> = (0..if case == "empty-caches" { 0 } else { n }).map(|i| (PathBuf::from(format!("/p/M{i}.fl")), ())).collect();
            let mut cache = ModuleCache { parses: entries.clone(), entries };
            let cached = cache.entries.len() + cache.parses.len();
            OPERATIONS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            ACTIVE.store(true, Ordering::Relaxed);
            cache.retain_loaded(&modules);
            ACTIVE.store(false, Ordering::Relaxed);
            let survivors = if case == "empty-caches" { 0 } else { loaded };
            assert_eq!(cache.entries.len(), survivors);
            assert_eq!(cache.parses.len(), survivors);
            println!("{case},{loaded},{cached},{},{}", OPERATIONS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
        }
    }
}
`;
    const input = path.join(directory, "probe.rs");
    const binary = path.join(directory, "probe");
    writeFileSync(input, rust);
    execFileSync("rustc", ["--edition=2024", "-O", "-A", "unused-imports", input, "-o", binary], { stdio: "inherit" });
    console.log(`revision ${sha}`);
    process.stdout.write(execFileSync(binary, [], { encoding: "utf8" }));
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}
