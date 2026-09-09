# Microslop

A Microsoft Teams client built with Flutter and Rust, with native desktop and Android integrations.

Microslop is a fork and continuation of [eisbaw/ost](https://github.com/eisbaw/ost), with Git ancestry preserved from OST commit `089214470ce0cea1c30d57ede54da07121ef71ec`.

## Flutter client

The Flutter application lives in `app`. It exposes a work/school account device-code login and a messaging workspace for teams, channels, and chats, backed by Rust through `flutter_rust_bridge`.

- Rust owns the Microsoft device-code request, token exchange, and secret persistence.
- Flutter receives only the verification address, user code, and safe sign-in state.
- The Microsoft refresh token is written to platform secure storage; Teams, Skype, Graph, and IC3 access tokens stay in Rust memory and are never returned to Dart.
- After sign-in, Flutter loads joined teams, channels, chats, and message history through Rust. It can send messages/files, react, mark chats read, receive live updates, and place or receive one-to-one calls.
- Android integrates native call audio routing, camera capture, and remote H.264 video rendering. Incoming caller video is currently receive-only, so answering does not automatically enable the local camera.

Run the application from `app` after enabling platform prerequisites:

```bash
flutter pub get
flutter run -d windows
```

The bridge generator is version 2.13.0. Regenerate bindings from `app` after changing the Rust bridge API:

```bash
flutter_rust_bridge_codegen generate --rust-root rust --rust-input crate::api --dart-output lib/src/rust
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the application boundaries and current scope.

![Microslop logo](app/assets/icon/app_icon.png)

![TUI Screenshot](docs/tui-screenshot.svg)

## Features

- **TUI**: Interactive terminal interface with teams, channels, chats, and per-user message colors
- **Authentication**: OAuth2 device code flow for work/school and personal accounts
- **Messaging**: List chats, read messages, send messages (stable)
- **Teams**: List joined teams and channels (stable)
- **Real-time**: WebSocket connection for push notifications (Trouter)
- **Calling**: One-to-one audio/video signaling with ICE, RTP/RTCP, and SRTP media
- **Audio** (optional): Microphone capture and speaker playback
- **Video** (optional): H.264 codec path, Android native camera/rendering, and Linux V4L2/SDL2 tooling

## Status

| Feature | Status |
|---------|--------|
| TUI | Working |
| Authentication | Stable |
| Chat / Messaging | Stable |
| Teams / Channels | Stable |
| Trouter (push) | Stable |
| Audio calls | Working; Microsoft Test Call covered |
| Video calls | Partial; outgoing/test paths covered, incoming receive path still needs real-person E2E |

**Note**: Calling uses reverse-engineered Teams signaling. TURN relay support and some ICE edge cases remain incomplete, so restrictive networks can still expose failures not seen in local or Test Call validation.

## Requirements

- Rust 1.85+ (the workspace includes Rust 2024 edition crates)
- Linux for the CLI V4L2/SDL2 media tools; Android for the Flutter native camera and remote-video surface
- Nix (recommended) or manual dependency installation
- [just](https://github.com/casey/just) command runner (optional, for convenience recipes)

### Dependencies

- **Audio**: ALSA development libraries
- **Video**: V4L2, SDL2, OpenH264

## Installation

### Using Nix (recommended)

The `shell.nix` provides all required dependencies. The `just` command runner executes recipes from the `Justfile`.

```bash
nix-shell
just build
```

### Manual

Install dependencies, then:

```bash
cargo build
```

For audio support:
```bash
cargo build --features audio
```

For video support:
```bash
cargo build --features video-capture
```

For full A/V support:
```bash
cargo build --features "audio,video-capture"
```

## Usage

### TUI (Terminal User Interface)

Launch the interactive TUI for browsing teams, channels, and chats:

```bash
teams-cli tui
```

### Authentication

Login with device code flow:

```bash
teams-cli login
```

Force re-authentication (ignores cached token):

```bash
teams-cli login --force
```

Check authentication status:

```bash
teams-cli status
teams-cli whoami
```

### Messaging

List recent chats:

```bash
teams-cli chats
```

Read messages from a chat:

```bash
teams-cli read <chat-id> --limit 20
```

Send a message:

```bash
teams-cli send --to <chat-id> "Hello from CLI!"
```

### Teams

List joined teams and channels:

```bash
teams-cli teams
```

### Real-time Notifications

Connect to Trouter for push notifications:

```bash
teams-cli trouter
```

### Calling

Test microphone (requires `--features audio`):

```bash
teams-cli mic-test
```

Test camera (requires `--features video-capture`):

```bash
teams-cli cam-test
```

Place a test call to Echo bot:

```bash
teams-cli call-test --echo --duration 20
```

## CLI Reference

```
teams-cli [OPTIONS] <COMMAND>

Options:
  -v, --verbose  Enable debug logging

Commands:
  login      OAuth2 device code authentication
             --force    Force re-login even if cached token exists
  logout     Clear stored credentials
  status     Show token expiry status
  whoami     Verify authentication
  chats      List recent chats
             --limit N  Number of chats to show
  read       Read messages from a chat
             --limit N  Number of messages to show
  send       Send a message
             --to ID    Chat ID to send to
  teams      List joined teams and channels
  tui        Launch interactive terminal user interface
  trouter    Connect to push notification service
  call-test  Place a test call
             --echo       Call the Echo bot (call quality tester)
             --duration N Call duration in seconds (default: 30)
             --thread ID  1:1 chat thread ID to call
             --record     Enable call recording
             --camera     Enable camera capture (video-capture feature)
             --display    Enable video display window (video-capture feature)
             --tone       Use test tone instead of microphone
  mic-test   Test microphone (audio feature)
  cam-test   Test camera (video-capture feature)
```

## Testing

### Unit Tests

Run the test suite:

```bash
just test
# or
cargo test
```

### End-to-End Tests

Live audio and video calling tests are documented in
[docs/calling-e2e.md](docs/calling-e2e.md). Workspace persistence and bounded
message prefetch decisions are documented in
[docs/workspace-navigation-and-prefetch.md](docs/workspace-navigation-and-prefetch.md).

E2E tests require a valid login session. Run all e2e tests:

```bash
just e2e
```

Individual e2e tests:

| Command | Description |
|---------|-------------|
| `just e2e-call` | Deterministic audio transport test with the Echo bot |
| `just e2e-call-video` | Live camera/display test with the Echo bot |
| `just call-echo` | Interactive microphone/speaker Echo call |
| `just mic-test` | Local microphone capture and playback |
| `just cam-test` | Local camera capture and display |

### Quality Checks

```bash
just check    # Run fmt-check, lint, and compile tests
just lint     # Run clippy lints
just fmt      # Format code
```

## Configuration

Tokens are stored in `~/.config/teams-cli/config.toml` with restricted permissions (0600).

## Documentation

- [Architecture Diagrams](docs/architecture.md) - Visual diagrams of authentication, messaging, calling, and media flows
- [Terminology Index](docs/terminology_index.md) - Glossary of protocols and terms (RTP, SRTP, ICE, SDP, etc.)
- [GUIDs Reference](docs/GUIDs.md) - Known Microsoft GUIDs (OAuth client IDs, tenant IDs, SEI UUIDs, bot MRIs)

## License

MIT License - see LICENSE file.

## Disclaimer

This is an unofficial client. Use at your own risk. Not affiliated with Microsoft.

## Related Projects

- [purple-teams](https://github.com/EionRobb/purple-teams/) - Teams plugin for libpurple (Pidgin, Finch, etc.)
