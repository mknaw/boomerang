{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, git-hooks }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Get a nightly toolchain for cargo-udeps
        rust-nightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);

        # Stable Rust toolchain with clippy
        rustStable = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [ "clippy" "rust-analyzer" "rust-docs" "rust-src" ];
          targets = [ "aarch64-unknown-linux-musl" ];
        };

        # Nightly rustfmt
        rustfmtNightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt);

        # Create a wrapper for cargo-udeps that uses nightly
        cargo-udeps-wrapped = pkgs.writeShellScriptBin "cargo-udeps" ''
          export RUSTC="${rust-nightly}/bin/rustc"
          export CARGO="${rust-nightly}/bin/cargo"
          exec "${pkgs.cargo-udeps}/bin/cargo-udeps" "$@"
        '';

        # Restate server package
        restate-server = pkgs.stdenv.mkDerivation {
          pname = "restate-server";
          version = "latest";

          src = pkgs.fetchurl {
            # TODO: you'd want other URLs for other platforms, but this works for now.
            url = "https://restate.gateway.scarf.sh/latest/restate-server-aarch64-apple-darwin.tar.xz";
            sha256 = "sha256-mvofY1mqRMHy1DmgWgB/QiDr5dQy7OqnWpWV1DqwYfw=";
          };

          nativeBuildInputs = [ pkgs.installShellFiles ];

          unpackPhase = ''
            tar -xf $src --strip-components=1
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp restate-server $out/bin/
            chmod +x $out/bin/restate-server
          '';
        };

        # Restate CLI package
        restate-cli = pkgs.stdenv.mkDerivation {
          pname = "restate";
          version = "latest";

          src = pkgs.fetchurl {
            url = "https://restate.gateway.scarf.sh/latest/restate-cli-aarch64-apple-darwin.tar.xz";
            sha256 = "sha256-BOOVP9FzcsWmRQyIipGYMD1pb+1AhUoXlLX2b1Gsu7Q=";
          };

          nativeBuildInputs = [ pkgs.installShellFiles ];

          unpackPhase = ''
            tar -xf $src --strip-components=1
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp restate $out/bin/
            chmod +x $out/bin/restate
          '';
        };

        # Git hooks configuration
        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            rustfmt = {
              enable = true;
              package = rustfmtNightly;
            };
            clippy = {
              enable = true;
              package = rustStable;
            };
          };
        };
      in
      {
        devShells.default = pkgs.mkShell {
          shellHook = ''
            ${pre-commit-check.shellHook}
            ulimit -n 8192
          '';
          buildInputs = with pkgs; [
            protobuf
            cargo-udeps-wrapped
            cargo-zigbuild
            restate-server
            restate-cli
            just

            rustStable
            rustfmtNightly

            bacon
          ] ++ pre-commit-check.enabledPackages;
        };

        checks = {
          pre-commit-check = pre-commit-check;
        };
      });
}

