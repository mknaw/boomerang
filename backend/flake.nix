{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
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
      in
      {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            protobuf
            cargo-udeps-wrapped

            (rust-bin.stable.latest.minimal.override {
              extensions = [ "clippy" "rust-analyzer" "rust-docs" "rust-src" ];
            })
            # We use nightly rustfmt features.
            (rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt))

            bacon
          ];
        };
      });
}
