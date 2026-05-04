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
              cargo-release
              bacon

              just
              sqlite

              python312
              uv

              pkg-config

              git-cliff
            ]
            ++ darwinDeps;

          env = {
            RUST_BACKTRACE = "1";
          };

          shellHook = ''
            export PROJECT_ROOT="$(pwd)"

            . "${./.}/scripts/activate.sh"
          '';
        };

        formatter = pkgs.nixfmt;
      }
    );
}
