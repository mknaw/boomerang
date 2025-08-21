{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pre-commit-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, pre-commit-hooks }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        
        pre-commit-check = pre-commit-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            rustfmt = {
              enable = true;
              packageOverrides.rustfmt = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt);
              files = "backend/.*\\.rs$";
            };
          };
        };
      in
      {
        checks = {
          inherit pre-commit-check;
        };
        
        devShell = pkgs.mkShell {
          inherit (pre-commit-check) shellHook;
          buildInputs = with pkgs; [
            openssl
            protobuf

            (rust-bin.stable.latest.minimal.override {
              extensions = [ "clippy" "rust-analyzer" "rust-docs" "rust-src" ];
            })
            (rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt))

            bacon
          ] ++ pre-commit-check.enabledPackages;
        };
      });
}