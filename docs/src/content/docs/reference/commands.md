---
title: "Contributed commands"
---

Built-in `std`, a project's backend and the plugins of its dependencies may add
commands to `flick`, beside its own. `flick --help` lists them, grouped by the
contributor that adds them. Built-in `std` provides `schema export`
(`std:schema export`) in memory without loading a WebAssembly component or
touching the descriptor cache.

## Running one

A contributed command runs under its name, `flick pairs-hello you`, when
no other contributor adds the same name. The qualified form
`flick <contributor>:<command>` works for every command the contributor
may add. The contributor's part is the
package of a plugin or the last part of a backend's id, so `test/literal`
becomes `literal`:

```sh
flick literal:literal-hello you --out greetings
```

- When two contributors add the same name, the flat name is
  `FLICK-P053`, which lists the qualified names.
- A contributor cannot take a name of `flick` itself, as `build`. Such a
  declaration is ignored, with the warning `FLICK-P054`.
- A name nobody adds is `FLICK-P052`. When an official backend or a
  contributor the store has seen adds it, the message names that one.

Outside a project the commands of the official backends run, with the
working directory as their root. Only `--allow-tool` grants them host
tools there.

## Help without loading anything

The first time a command of a contributor runs, the compiler keeps the
contributor's descriptors in the package store, keyed by the digest of its
component. `flick --help` reads only that cache. It never instantiates a
component and never reaches the network. A contributor whose cache entry is
missing appears after its first command. Outside a project, help lists
every contributor the store has seen.

## Writing one

A plugin implements the command methods of `flick_plugin::Plugin`, and a
backend those of `flick_backend::Backend`. The methods are the same. For a
plugin they come with the `commands` feature of `flick-plugin`, which its
`guest` feature, the one that builds the component, turns on:

```rust
use flick_plugin::command::{Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Style, ValueKind};

impl Plugin for Notes {
    fn commands(&self) -> Vec<Descriptor> {
        vec![Descriptor {
            name: "notes add".into(),
            summary: "add a note to notes.txt".into(),
            positionals: vec![Parameter {
                name: "text".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: true,
                help: "what to note".into(),
            }],
            options: vec![],
            writes: vec!["notes.txt".into()],
            style: Style::Plan,
        }]
    }

    fn run(&self, invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
        let text = &invocation.arguments[0].values[0];
        let mut plan = Plan::empty();
        plan.files.push(File { path: "notes.txt".into(), bytes: text.clone().into_bytes(), executable: false });
        Ok(plan)
    }
    // …
}
```

A descriptor declares:

- `name`: one or more words, as `ffi import`.
- `positionals` and `options`: each a flag, a text or a path, possibly
  repeated. The host parses the command line by them and gives each path
  relative to the project's root, `/`-separated. A path outside the
  project is refused before the command runs.
- `inputs`: the files the host reads for the command, relative to the
  project's root, and passes in `invocation.inputs`. A directory stands for
  the regular files under it, outside `node_modules` and hidden
  directories. A path that does not exist gives nothing.
- `reads`: what the plan's tool steps may read in place.
- `writes`: the paths the command may write.
- `style`: `plan` or `session`.

In `inputs`, `reads` and `writes`, `$name` stands for the paths the
parameter `name` gives and `$sources` for the project's source root, which
`invocation.sources` also holds.

## Plans

A command is a pure call. It never writes a file or starts a process
itself: it returns a plan, which the host checks and carries out.

- **Files** are written under the declared paths.
- **Tool steps** run a host tool from `invocation.tools`. These are the
  tools the project allows the contributor in `[tools]`, by plugin package
  or backend id, and the host pins each one by digest and version. A
  step's `reads`, within the descriptor's, appear in its directory at the
  same relative path, linked to the project's files rather than copied.
- **`run-flick` steps** compile and run a Flick program through the project's
  selected backend in an isolated temporary generation, without writing to the
  project tree or the disk module cache. A step may overlay in-memory
  `FlickModule` sources on `$sources`, pass `arguments` to the program, capture
  stdout into `output` (handed to `resume` when set, otherwise published under
  `descriptor.writes`), and attach an `origin` span in `invocation.inputs` so
  diagnostics originating in synthetic overlay modules point back to user
  source. Only the selected backend's own `[tools]` grant applies (for example
  `"flick/go" = ["go"]` on Go, and none on `flick/js`); runtime failures,
  timeouts, or output-limit violations report `FLICK-P061`.
- **Diagnostics** are reported as the compiler's own. A `span` points into
  a file of `invocation.inputs`, and `help` tells the user what to do.
- **`stdout`** is what the command prints.
- **`resume`**, when set, is the state the host hands back after the
  steps have run. The host calls `resume(state, outputs)` with the step
  outputs instead of writing them. The plan it returns goes on the same
  way, up to 16 plans in all.

The host refuses a plan with `FLICK-P055` before anything happens when it
writes outside its declared paths or the project, runs a tool the host did
not pin, reads a file the plan does not have, or includes an invalid
`run-flick` step (invalid or duplicate module names, duplicate step ids,
oversized overlays, escaping or undeclared output paths, or an out-of-bounds
`origin` span). Steps run in declared order, each tool step in a directory
of its own with only its inputs, as a build step does (see
[project.md](../project/)). The files of every plan and the outputs of the
last plan's steps are then published together in one transaction: a
failure leaves the project as it was.

## Sessions

A session command serves requests on stdio for as long as its caller
keeps it open, as an editor integration does. The host owns stdio:

- It reads and writes messages with `Content-Length` framing, as the
  language server protocol does.
- `open` gives the state of a new session, or a plan the command runs once
  instead, as `content-mapper --declarations` does.
- `handle` receives each message with the current state and returns the
  answer, if any, and the next state.
- A `handle` that needs something the host computes, the TypeScript
  projection of a module, returns `needs` instead of an answer. The host
  answers them and calls `handle` again with the same message, the state
  it returned and the answers. A plan asks the same way, for `resume`.
- A trap in `handle` becomes the JSON-RPC error `-32603`, and the session
  keeps its last state.
- `$/cancelRequest` stops the request it names: its call is interrupted
  and the answer is the error `-32800`.
- `exit`, or the end of stdin, calls `close`, whose plan the host carries
  out. What that plan prints goes to stderr, since stdout belongs to the
  protocol.

## Templates

Backends and library plugins also contribute project templates for `flick new`
and `flick init` on the same `flick:command/command` interface:

- `templates()` returns a list of `TemplateDescriptor { name, summary }`.
- `scaffold(request)` receives a `TemplateRequest { template, name, compiler, init }`
  (with the normalized package name and the running compiler's version) and
  returns a `Plan` of all template files — including `flick.toml` — plus
  optional next-step guidance in `stdout`.

Template plans are pure file plans: the host refuses any template plan that
includes host `steps` (`tool` or `run-flick`), `resume`, `needs`, or escaping
paths with `FLICK-P055` before creating directories or writing files. Unknown
templates return `FLICK-P060` listing the contributor's available templates.

Users select an official backend's templates with `--target <name>` (`js` by
default, or `go`), or load a third-party plugin or backend template package
with `--from <path|url|git>` (verified like `flick add` and pinned under
`[[template]]` in `flick.lock`).
