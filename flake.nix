{
  description = "Hypomnema — local Markdown indexer + MCP daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # In modern nixpkgs (2026) the Apple SDK frameworks ship with
        # stdenv on darwin, so we no longer list them explicitly. Keep
        # libiconv, which a handful of Rust crates still look up by name.
        darwinDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages =
            with pkgs;
            [
              rustToolchain

              cargo-watch
              cargo-nextest
              cargo-edit
              bacon

              just
              sqlite

              python3

              pkg-config
            ]
            ++ darwinDeps;

          env = {
            RUST_BACKTRACE = "1";
          };

          shellHook = ''
            echo "hypomnema dev shell — $(rustc --version)"
            echo "  just          — task runner ('just' alone lists targets)"
            echo "  cargo nextest — test runner"
            echo "  bacon         — background cargo check/clippy"
          '';
        };

        formatter = pkgs.nixfmt;
      }
    );
}
