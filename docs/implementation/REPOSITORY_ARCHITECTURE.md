# IRIS Repository Architecture

## Monorepo Decision

IRIS uses a single monorepo for all components: Rust core, Android app, iOS app, desktop app, simulation tools, and documentation.

### Rationale

**Protocol changes require atomic updates**: When the CBOR wire format changes (e.g., adding a new field to the message envelope), the change must be made in `iris-proto`, and all platform applications must be updated simultaneously. In a polyrepo, coordinating this across 4+ repositories with separate CI pipelines is error-prone and requires careful version pinning. In a monorepo, the change is one commit that updates `iris-proto` and all dependent apps together.

**Dependency graph is explicit**: All internal dependencies are at known, local versions. No crates.io publishing required for internal crates. No version pinning nightmares.

**Single CI pipeline**: One GitHub Actions workflow orchestrates all build/test steps. Build matrix handles platform-specific builds.

**Shared tooling**: `justfile` (or `Makefile`) contains all developer commands. New contributor runs `just setup` and everything is configured.

**Simpler code review**: A protocol change, its Android implementation, its iOS implementation, and its tests are all visible in one pull request.

## Repository Structure

```
iris/
├── Cargo.toml                         # Workspace root — lists all crates
├── Cargo.lock                         # Locked dependency versions (committed)
├── rust-toolchain.toml                # Pinned Rust toolchain version
├── .cargo/
│   └── config.toml                    # Target-specific linker config, aliases
├── justfile                           # Developer task runner (just)
│
├── crates/
│   ├── iris-core/                     # Core protocol, routing, message engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── message_engine.rs
│   │       ├── routing_engine.rs
│   │       ├── transport_manager.rs
│   │       ├── discovery.rs
│   │       ├── emergency.rs
│   │       └── identity.rs
│   ├── iris-transport/                # Transport abstraction traits
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── adapter.rs             # TransportAdapter trait
│   │       ├── manager.rs             # TransportManager
│   │       └── types.rs               # TransportType, TransportCapabilities
│   ├── iris-storage/                  # SQLite storage layer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── db.rs
│   │       ├── schema.rs              # Table definitions
│   │       ├── migrations/            # rusqlite_migration files
│   │       └── queries.rs
│   ├── iris-crypto/                   # Crypto primitives wrapper
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── identity.rs            # Key generation, Ed25519
│   │       ├── encryption.rs          # X25519 + ChaCha20-Poly1305
│   │       └── hash.rs                # BLAKE3
│   ├── iris-sim/                      # Simulation engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── node.rs                # Virtual node
│   │       ├── network.rs             # Virtual network graph
│   │       ├── transport.rs           # Simulated transport
│   │       └── time.rs                # Time control
│   └── iris-proto/                    # CBOR schema definitions
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── envelope.rs            # Top-level message envelope
│           ├── message.rs             # Message types
│           ├── routing.rs             # Routing protocol messages
│           └── emergency.rs           # Emergency message types
│
├── binaries/
│   ├── iris-relay/                    # Linux headless relay binary
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── iris-ctl/                      # CLI management tool
│       ├── Cargo.toml
│       └── src/main.rs
│
├── ffi/
│   └── iris-ffi/                      # UniFFI bindings
│       ├── Cargo.toml
│       ├── src/
│       │   └── lib.rs                 # pub extern "C" functions
│       └── iris.udl                   # UniFFI interface definition
│
├── android/                           # Android application
│   ├── build.gradle.kts               # Root build file
│   ├── settings.gradle.kts
│   ├── gradle.properties
│   └── app/
│       ├── build.gradle.kts
│       └── src/main/
│           ├── java/app/iris/         # Kotlin source
│           │   ├── ui/
│           │   ├── viewmodel/
│           │   ├── usecase/
│           │   ├── repository/
│           │   ├── service/
│           │   └── di/
│           ├── jniLibs/               # Compiled Rust .so (committed or CI artifact)
│           │   ├── arm64-v8a/libiriscode.so
│           │   ├── armeabi-v7a/libiriscode.so
│           │   └── x86_64/libiriscode.so
│           └── AndroidManifest.xml
│
├── ios/                               # iOS application
│   ├── IRIS.xcodeproj
│   ├── IRIS/
│   │   ├── App/
│   │   ├── UI/
│   │   ├── ViewModel/
│   │   ├── Services/
│   │   └── RustFFI/                   # UniFFI-generated Swift (committed)
│   │       ├── IrisCore.swift
│   │       └── IrisCoreFFI.h
│   └── IrisFramework/                 # XCFramework (CI artifact)
│
├── desktop/                           # Tauri desktop application
│   ├── package.json
│   ├── src-tauri/
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   │       ├── main.rs
│   │       └── commands/
│   └── src/                           # TypeScript/React
│       ├── App.tsx
│       └── components/
│
├── tools/
│   ├── simulation/                    # Python simulation scripts (The ONE compatible)
│   │   ├── scenarios/                 # Scenario definitions
│   │   ├── analysis/                  # Result analysis notebooks
│   │   └── requirements.txt
│   └── analysis/                      # Data analysis tools
│       └── routing_analysis.py
│
├── docs/                              # All documentation (this tree)
│   ├── architecture/
│   ├── platforms/
│   ├── implementation/
│   ├── testing/
│   ├── performance/
│   ├── operations/
│   ├── research/
│   ├── legal/
│   ├── product/
│   ├── decisions/
│   ├── requirements/
│   ├── experiments/
│   └── benchmarks/
│
├── engineering/                       # Control plane (YAML specifications)
│   ├── message_format.yaml
│   ├── routing_policy.yaml
│   └── emergency_broadcast.yaml
│
└── tests/                             # Cross-platform integration tests
    ├── integration/                   # In-process integration tests (Rust)
    └── e2e/                           # End-to-end tests (physical device required)
```

## Cargo Workspace

`Cargo.toml` at root:

```toml
[workspace]
resolver = "2"
members = [
    "crates/iris-core",
    "crates/iris-transport",
    "crates/iris-storage",
    "crates/iris-crypto",
    "crates/iris-sim",
    "crates/iris-proto",
    "binaries/iris-relay",
    "binaries/iris-ctl",
    "ffi/iris-ffi",
    "desktop/src-tauri",
]

[workspace.dependencies]
# Pinned versions for all workspace crates
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
ciborium = "0.2"
ed25519-dalek = { version = "2", features = ["serde"] }
x25519-dalek = { version = "2", features = ["serde"] }
chacha20poly1305 = "0.10"
hkdf = "0.12"
blake3 = "1"
uuid = { version = "1", features = ["v7"] }
rusqlite = { version = "0.31", features = ["bundled"] }
rusqlite_migration = "1"
tracing = "0.1"
thiserror = "1"
anyhow = "1"
```

All crates reference `workspace.dependencies` via `tokio.workspace = true`. No version drift between crates.

## CI Pipeline

### GitHub Actions Build Matrix

```yaml
# .github/workflows/ci.yml
strategy:
  matrix:
    include:
      - os: ubuntu-22.04
        target: x86_64-unknown-linux-gnu
        test: unit+integration
      - os: ubuntu-22.04
        target: aarch64-unknown-linux-gnu
        test: build-only
      - os: macos-14
        target: aarch64-apple-darwin
        test: unit+integration
      - os: macos-14
        target: aarch64-apple-ios
        test: build-only
      - os: windows-2022
        target: x86_64-pc-windows-msvc
        test: unit+integration
      - os: ubuntu-22.04
        target: aarch64-linux-android
        test: build-only
```

### Pipeline Stages

1. **Format check**: `cargo fmt --check`, `ktlint`, `swiftformat --lint`
2. **Lint**: `cargo clippy -- -D warnings`
3. **Rust unit tests**: `cargo test --workspace` (all targets that compile natively)
4. **Android build**: `./gradlew assembleDebug` (no device tests in CI)
5. **iOS build**: `xcodebuild build` (macOS runners only)
6. **Desktop build**: `npm run tauri build`
7. **Simulation tests**: nightly, `cargo run --bin iris-sim -- scenarios/ahmedabad_100.yaml`
8. **Coverage**: `cargo llvm-cov --workspace --lcov --output-path coverage.lcov`

### Release Pipeline

Triggered by `v*` tag push:
1. All CI stages pass
2. Changelog generated from conventional commits
3. Android: signed APK + AAB (using Keystore secret)
4. iOS: signed IPA (using Xcode Cloud or Fastlane)
5. Desktop: signed MSI (Windows), signed .dmg (macOS), .deb + AppImage (Linux)
6. GitHub Release created with all artifacts
7. Version bump PR opened automatically

## Developer Setup

```bash
# Clone
git clone https://github.com/iris-project/iris.git
cd iris

# Install just (task runner)
cargo install just

# Install all tooling
just setup
# Installs: Rust targets, cargo-ndk, cargo-deb, uniffi-bindgen,
#           Node.js deps, Android SDK check, Xcode check

# Run all Rust tests
just test

# Build Android debug APK (requires Android SDK)
just android-debug

# Run desktop app in dev mode
just desktop-dev

# Run simulation
just sim scenarios/basic_routing.yaml
```
