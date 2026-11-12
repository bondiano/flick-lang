//! The TypeScript form a library gives a type of its own: the declarations
//! of the app name it, and the file it comes from ships beside the module.

mod common;

use std::fs;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const CLOCK: &str = "module Lib.Clock exposing (Moment, Stamp, now, stamp)

@ts \"$Moment\"
@ts import \"./clock.d.mts\" \"Moment\"
type Moment

@ts \"Date\"
type Stamp

now : Unit ->{} Moment
foreign now from \"./clock.mjs\" \"now\"
  arity 0

stamp : Moment ->{} Stamp
foreign stamp from \"./clock.mjs\" \"stamp\"
";

const CLOCK_JS: &str = "export const now = () => ({ at: Date.now() });
export const stamp = (moment) => new Date(moment.at);
";

const CLOCK_TYPES: &str = "export interface Moment { readonly at: number }
";

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"Main\"

[dependencies]
lib = { path = \"../lib\" }

[build]
emit-dts = true
";

const MAIN: &str = "module Main exposing (main, today, later)

import Lib.Clock

today : Unit ->{} Lib.Clock.Stamp
let today () = Lib.Clock.stamp (Lib.Clock.now ())

later : Unit ->{} Lib.Clock.Moment
let later = Lib.Clock.now

main : Unit ->{} Unit
let main () = ()
";

#[test]
fn the_form_a_library_gives_its_type_is_written_and_its_file_ships() {
    let dir = project(
        "package_ts_forms",
        &[
            ("lib/flick.toml", LIB),
            ("lib/src/Lib/Clock.fl", CLOCK),
            ("lib/src/Lib/clock.mjs", CLOCK_JS),
            ("lib/src/Lib/clock.d.mts", CLOCK_TYPES),
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
        ],
    );
    let app = dir.join("app");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let dist = app.join("dist");
    let main = fs::read_to_string(dist.join("Main.d.ts")).expect("the declarations");
    assert!(main.contains("Effect<Date, never, never>"), "{main}");
    assert!(
        main.contains("import type { Moment as $Moment$t_Lib$Clock } from \"./Lib/clock.d.mts\";"),
        "{main}"
    );
    assert!(
        main.contains("Effect<$Moment$t_Lib$Clock, never, never>"),
        "{main}"
    );
    // the library declares neither type: each is what its form says
    let clock = fs::read_to_string(dist.join("Lib/Clock.d.ts")).expect("the declarations");
    assert!(
        !clock.contains("interface Moment") && !clock.contains("interface Stamp"),
        "{clock}"
    );
    assert_eq!(
        fs::read_to_string(dist.join("Lib/clock.d.mts")).expect("shipped"),
        CLOCK_TYPES
    );
}

/// A library whose forms alone name a file, with no binding beside it,
/// ships the file too.
#[test]
fn a_file_only_a_form_names_ships() {
    let clock = "module Lib.Clock exposing (Moment)

@ts \"$Moment\"
@ts import \"./clock.d.mts\" \"Moment\"
type Moment
";
    let main = "module Main exposing (main, keep)

import Lib.Clock

keep : Lib.Clock.Moment -> Lib.Clock.Moment
let keep moment = moment

main : Unit ->{} Unit
let main () = ()
";
    let dir = project(
        "package_ts_only_forms",
        &[
            ("lib/flick.toml", LIB),
            ("lib/src/Lib/Clock.fl", clock),
            ("lib/src/Lib/clock.d.mts", CLOCK_TYPES),
            ("app/flick.toml", APP),
            ("app/src/Main.fl", main),
        ],
    );
    let app = dir.join("app");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    assert!(app.join("dist/Lib/clock.d.mts").is_file());
}
