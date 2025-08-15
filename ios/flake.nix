{
  description = "Swift iOS Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Core Swift toolchain
            swift
            swift-format
            swiftlint
            
            # Development tools
            git
            
            # General development tools (if available)
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # macOS-specific development tools
            cocoapods
            fastlane
          ] ++ lib.optionals (builtins.hasAttr "swift-format" pkgs) [
            # Optional Swift formatting (if available)
            pkgs."swift-format"
          ];

          # Environment variables
          SWIFT_VERSION = "5.8";
          DEVELOPER_DIR = "/Applications/Xcode.app/Contents/Developer";
          
          # Swift Package Manager configuration
          SWIFTPM_DISABLE_SANDBOX_SHOULD_NOT_BE_USED = "1";
        };

        # Optional: Package for building the project
        packages.default = pkgs.stdenv.mkDerivation {
          pname = "boomerang";
          version = "0.1.0";
          
          src = ./.;
          
          buildInputs = with pkgs; [
            swift
            swiftpm
          ];
          
          buildPhase = ''
            # This would need to be customized based on your build process
            # For iOS apps, typically you'd use xcodebuild
            echo "Building Swift iOS project..."
          '';
          
          installPhase = ''
            mkdir -p $out
            echo "iOS app build would be installed here"
          '';
        };

        # Formatter for the flake
        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
