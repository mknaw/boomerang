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

        # Create a wrapper for cargo-udeps that uses nightly
        cargo-udeps-wrapped = pkgs.writeShellScriptBin "cargo-udeps" ''
          export RUSTC="${rust-nightly}/bin/rustc"
          export CARGO="${rust-nightly}/bin/cargo"
          exec "${pkgs.cargo-udeps}/bin/cargo-udeps" "$@"
        '';

        # Restate server package
        restate-server = pkgs.stdenv.mkDerivation rec {
          pname = "restate-server";
          version = "latest";

          src = pkgs.fetchurl {
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
        restate-cli = pkgs.stdenv.mkDerivation rec {
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
            rustfmt.enable = true;
          };
        };
      in
      {
        devShells.default = pkgs.mkShell {
          inherit (pre-commit-check) shellHook;
          buildInputs = with pkgs; [
            protobuf
            cargo-udeps-wrapped
            restate-server
            restate-cli

            (rust-bin.stable.latest.minimal.override {
              extensions = [ "clippy" "rust-analyzer" "rust-docs" "rust-src" ];
            })
            # We use nightly rustfmt features.
            (rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt))

            bacon
          ] ++ pre-commit-check.enabledPackages;
        };

        checks = {
          pre-commit-check = pre-commit-check;
        };
      });
}

