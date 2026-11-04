{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "flick";
  version = (lib.importTOML ../flick/Cargo.toml).package.version;

  # Build scripts embed runtime-go/ (every .go file, go.mod and go.sum) and
  # tests read the .fl corpus outside the crate, so the whole checkout ships.
  # Only build products and installed package trees are dropped; the
  # `node_modules` under tests/check/projects are checked-in fixtures.
  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: _type:
      let
        name = baseNameOf path;
        fixture = builtins.match ".*/tests/check/projects/[^/]+/node_modules(/.*)?" (toString path) != null;
      in
      (name != "node_modules" || fixture)
      && !(builtins.elem name [
        ".direnv"
        ".git"
        ".lake"
        "dist"
        "result"
        "target"
      ]);
  };

  cargoLock.lockFile = ../Cargo.lock;

  # The workspace also holds the wasm bindings, the reference model and the
  # pretty printer; only the CLI is installed.
  cargoBuildFlags = [
    "--package"
    "flick"
  ];
  # plugins built as WebAssembly components, as in the released binaries;
  # the tests below are of flick-core, which has no such feature
  buildFeatures = [ "wasm-plugins" ];
  checkFeatures = [ ];

  # The full suite needs node, pnpm, Docker and network access (CI runs it).
  # These targets are self-contained: the checker fixtures (including their
  # checked-in node_modules), the syntax corpus, stdlib signatures, builds and
  # fuzzing. consteval_node is left out: it shells out to node. scaling is left
  # out: its wall-clock ratios are not stable inside the build sandbox.
  doCheck = true;
  cargoTestFlags = [
    "--package"
    "flick-core"
    "--lib"
    "--test"
    "check"
    "--test"
    "corpus"
    "--test"
    "effect_surface"
    "--test"
    "features"
    "--test"
    "fuzz"
    "--test"
    "projection"
    "--test"
    "stdlib_signatures"
    "--test"
    "transformer_captures"
  ];

  meta = {
    description = "Compiler and language server for the Flick language";
    homepage = "https://github.com/bondiano/flick-lang";
    license = lib.licenses.mit;
    mainProgram = "flick";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
  };
}
