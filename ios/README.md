# Swift iOS Development with Nix

This project uses a Nix flake to provide a reproducible Swift iOS development environment.

## What's Included

The development environment provides:

- **Swift 5.8** - Swift compiler and toolchain
- **SwiftLint** - Swift linting tool
- **Cocoapods** - iOS dependency manager (macOS only)
- **Fastlane** - iOS automation toolkit (macOS only)
- **Swift-format** - Swift code formatter (if available)
- **Git** - Version control

## Quick Start

### Prerequisites

- [Nix](https://nixos.org/download.html) with flakes enabled
- Xcode (for iOS development on macOS)

### Using the Environment

1. **Enter the development shell:**
   ```bash
   nix develop
   ```

2. **Or with direnv (automatic):**
   ```bash
   # Install direnv first: https://direnv.net/docs/installation.html
   direnv allow
   ```

3. **Build your project:**
   ```bash
   # Open in Xcode
   open boomerang.xcodeproj
   
   # Or build from command line
   xcodebuild -project boomerang.xcodeproj -scheme boomerang build
   ```

4. **Run SwiftLint:**
   ```bash
   swiftlint
   ```

## Development Tools Available

Once in the development shell, you have access to:

- `swift` - Swift compiler and REPL
- `swift package` - Swift Package Manager
- `swiftlint` - Swift code linting
- `cocoapods` - iOS dependency management
- `fastlane` - iOS automation and deployment
- `swift-format` - Code formatting (if available)

## Project Structure

```
.
├── flake.nix              # Nix flake configuration
├── flake.lock             # Locked dependency versions
├── .envrc                 # direnv configuration
├── boomerang.xcodeproj/   # Xcode project
└── boomerang/             # Swift source files
    ├── BoomerangApp.swift
    └── ContentView.swift
```

## Customization

You can modify `flake.nix` to:

- Add more development tools
- Change Swift version (when available)
- Add environment variables
- Customize the shell hook message

## Troubleshooting

### Flake not working?
Make sure you have Nix with flakes enabled:
```bash
nix --version  # Should be 2.4+
```

### Missing tools?
Some tools are only available on specific platforms. The flake automatically handles platform-specific dependencies.

### Git tracking
The flake needs to be tracked by git to work:
```bash
git add flake.nix flake.lock
```
