---
title: "Contributed commands"
---

A project's backend and the plugins of its dependencies may add commands
to `flick`, beside its own. `flick --help` lists them, grouped by the
backend or plugin that adds them.

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
- A name nobody adds is `FLICK-P052`. When a contributor the store has
  seen adds it, the message names that contributor.

Outside a project only the core commands run, since a contributed command
belongs to the project whose backend or dependency adds it.

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
  repeated. The host parses the command line by them.
- `writes`: the paths the command may write, relative to the project's
  root. `$name` stands for the path the parameter `name` gives.
- `style`: `plan` or `session`.

## Plans

A command is a pure call. It never writes a file or starts a process
itself: it returns a plan, which the host checks and carries out.

- **Files** are written under the declared paths.
- **Tool steps** run a host tool from `invocation.tools`. These are the
  tools the project allows the contributor in `[tools]`, by plugin package
  or backend id, and the host pins each one by digest and version.
- **Diagnostics** are reported as the compiler's own.
- **`stdout`** is what the command prints.

The host refuses a plan with `FLICK-P055` before anything happens when it
writes outside its declared paths or the project, runs a tool the host did
not pin, or reads a file the plan does not have. A step runs in a directory
of its own with only its inputs, as a build step does (see
[project.md](../project/)). The files and step outputs are then published
together in one transaction: a failure leaves the project as it was.

## Sessions

A session command serves requests on stdio for as long as its caller
keeps it open, as an editor integration does. The host owns stdio:

- It reads and writes messages with `Content-Length` framing, as the
  language server protocol does.
- `open` gives the state of a new session.
- `handle` receives each message with the current state and returns the
  answer, if any, and the next state.
- A trap in `handle` becomes the JSON-RPC error `-32603`, and the session
  keeps its last state.
- `$/cancelRequest` stops the request it names: its call is interrupted
  and the answer is the error `-32800`.
- `exit`, or the end of stdin, calls `close`, whose plan the host carries
  out. What that plan prints goes to stderr, since stdout belongs to the
  protocol.
