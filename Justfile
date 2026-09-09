# Teams CLI - Justfile

# List available recipes
default:
    @just --list

# --- Build ---

# Build teams-cli (debug)
build:
    cargo build

# Build teams-cli (release)
build-release:
    cargo build --release

# Build with audio support (microphone/speaker)
build-audio:
    cargo build --features audio

# Build with video capture support (camera/display)
build-video:
    cargo build --features video-capture

# Build with full A/V support
build-full:
    cargo build --features "audio,video-capture"

# --- Quality ---

# Run clippy lints
lint:
    cargo clippy --all-targets --all-features

# Format code
fmt:
    cargo fmt

# Check formatting without changes
fmt-check:
    cargo fmt -- --check

# Run all quality checks
check: fmt-check lint
    cargo test --no-run

# --- Test ---

# Run unit tests
test:
    cargo test

# Run the maintained live-account suite (requires an unlocked login session).
# MICROSLOP_E2E_SELF_CHAT must name the 1:1 chat used for safe CUD checks.
e2e:
    test -n "$MICROSLOP_E2E_SELF_CHAT"
    cd app && DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" flutter test integration_test/live_account_e2e_test.dart -d linux --dart-define="MICROSLOP_E2E_SELF_CHAT=$MICROSLOP_E2E_SELF_CHAT" --dart-define=MICROSLOP_E2E_PLATFORM=linux

screenshots:
    app/tool/capture_ui_screenshots.sh

# Run deterministic Echo audio transport test (requires valid login)
e2e-call: build-audio
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" ./target/debug/teams-cli call-test --echo --tone --duration 20

# Run live Echo audio/video test (requires camera, display, and valid login)
e2e-call-video: build-full
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" ./target/debug/teams-cli call-test --echo --camera --display --duration 20

# --- Run ---

# Launch Flutter against the real desktop session bus. This avoids isolated
# D-Bus/keyring sessions when invoked by automation or editor terminals.
app-linux:
    cd app && DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" flutter run -d linux

# Show CLI help
help:
    cargo run -- --help

# Login with device code flow
login:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- login

# Show authentication status
status:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- status

# Show current user info
whoami:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- whoami

# List recent chats
chats:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- chats

# List joined teams and channels
teams:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- teams

# Connect to Trouter for real-time notifications
trouter:
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" cargo run -- trouter

# --- Audio/Video ---

# Test microphone (record 3s, playback)
mic-test: build-audio
    ./target/debug/teams-cli mic-test

# Test camera (capture 3s, display)
cam-test: build-video
    ./target/debug/teams-cli cam-test

# Place audio call to Echo bot (20s)
call-echo: build-audio
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" ./target/debug/teams-cli call-test --echo --duration 20

# --- TUI ---

# Launch the terminal UI
tui: build
    ./target/debug/teams-cli tui

# Place A/V call to Echo bot with camera and display (20s)
call-echo-video: build-full
    DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR}/bus" ./target/debug/teams-cli call-test --echo --camera --display --duration 20
