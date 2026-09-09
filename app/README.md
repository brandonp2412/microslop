# Microslop Flutter app

The native Flutter shell for Microslop. Messaging, authentication, Trouter events, and call signaling/media are backed by Rust through `flutter_rust_bridge`.

## Run

```bash
flutter pub get
flutter run -d linux
```

Android uses the same Flutter UI with native Kotlin bridges for camera capture, call audio routing, foreground message watching, and remote video rendering.

## Validate

```bash
flutter analyze
flutter test
flutter build linux --release
flutter build apk --release
```

Run `app/tool/capture_ui_screenshots.sh` from the repository root to regenerate deterministic desktop, mobile, settings, chat, sign-in, and call screenshots under `app/build/ui-screenshots/`. If `just` is installed, `just screenshots` runs the same command.

## Rust bridge

The bridge crate is `rust/`. After changing its public bridge API, regenerate bindings with:

```bash
flutter_rust_bridge_codegen generate --rust-root rust --rust-input crate::api --dart-output lib/src/rust
```
