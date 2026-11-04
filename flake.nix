{
  description = "Flick — an ML-family language that compiles to JavaScript and Go";

  # Usage:
  #   nix build .#flick              # ./result/bin/flick
  #   nix profile install .#flick
  #   nix develop                    # toolchain, node, pnpm, go, just
  #
  # On NixOS, add the package without the flake:
  #   nixpkgs.overlays = [ inputs.flick.overlays.default ];
  #   environment.systemPackages = [ pkgs.flick ];
  #
  # The pinned Rust toolchain is the one rust-toolchain.toml names, so a
  # `nix build` and a checkout compile with the same rustc. Node, pnpm and Go
  # are runtime tools of generated projects, not of the compiler itself: the
  # packaged binary only embeds the Go runtime sources from runtime-go/.

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      lib = nixpkgs.lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = lib.genAttrs systems;

      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      # rust-analyzer and rust-src are editor aids, not build inputs.
      rustDevToolchainFor =
        pkgs:
        (rustToolchainFor pkgs).override (toolchain: {
          extensions = toolchain.extensions ++ [
            "rust-analyzer"
            "rust-src"
          ];
        });

      rustPlatformFor =
        pkgs:
        pkgs.makeRustPlatform {
          cargo = rustToolchainFor pkgs;
          rustc = rustToolchainFor pkgs;
        };

    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          flick = pkgs.callPackage ./nix/package.nix {
            rustPlatform = rustPlatformFor pkgs;
          };
        in
        {
          inherit flick;
          default = flick;
        }
      );

      overlays.default = lib.composeExtensions rust-overlay.overlays.default (
        final: _prev: {
          flick = final.callPackage ./nix/package.nix {
            rustPlatform = rustPlatformFor final;
          };
        }
      );

      checks = forAllSystems (system: {
        flick = self.packages.${system}.flick;
        default = self.checks.${system}.flick;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          rustToolchain = rustDevToolchainFor pkgs;
        in
        {
          default = pkgs.mkShell {
            name = "flick-dev";

            packages = with pkgs; [
              rustToolchain
              nodejs_24
              # pnpm 10+ reads the packageManager pin from package.json and
              # provisions that exact version on first use.
              pnpm
              go
              just
              cargo-machete
              cargo-nextest
              jq
            ];

            env.RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

            shellHook = ''
              echo "flick dev shell: $(${rustToolchain}/bin/rustc --version), node $(node --version)"
              echo "bootstrap: pnpm install --frozen-lockfile && just check"
            '';
          };
        }
      );

      formatter = forAllSystems (system: (pkgsFor system).nixfmt);
    };
}
