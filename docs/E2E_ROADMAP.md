# Microslop E2E Roadmap

Last updated: 2026-09-07

## Permanent test rules

- Read this file before each Microslop work session and update it with new evidence, failures, fixes, and remaining platform coverage.
- Exercise the real UI end to end. Prefer actual clicks/taps/typing over calling backend methods directly when validating user-visible actions.
- Outside the self-chat named `brandonp2412`, only perform read-only/navigation actions. Do not send messages, react, upload, call people, change membership, mark/delete/hide content, or make other creative/update/destructive changes.
- Creative/update/destructive testing is allowed only in the `brandonp2412` self-chat, except Microsoft Test Call Bot actions that are explicitly designed as a self-contained diagnostic.
- Never uninstall the production Android app or clear its data. Device integration tests must use an isolated/disposable application ID when a runner could uninstall the app.
- Do not E2E-test on brandonp2412's physical Android device. Android E2E runs on Waydroid only. The physical Android device may receive an explicitly requested release reinstall/update, but no test runner or exploratory action sweep.
- Record unavailable hardware/platform blockers instead of pretending they were tested.

## Definition of done

For every supported platform, exercise every reachable user action at least once, including navigation, search, chat/channel selection, message rendering, image/media rendering, menus, settings pages, account/profile surfaces, notifications surfaces, compose controls, attachments, reactions, call controls, keyboard/focus behavior, scrolling/pagination, back navigation, narrow/wide layouts where relevant, and error/empty/loading states. Mutating actions must obey the self-chat rule above.

Any defect found during exploration is fixed, re-tested on the affected platform, and regression-covered where practical. Platform-specific actions are tested on real platform code, not counted as covered by a widget fake on another platform.

## Platform matrix

| Platform | Target exists | Runtime available on development host | Live-account UI E2E | Full action sweep | Notes |
| --- | --- | --- | --- | --- | --- |
| Linux desktop | yes | yes | live exploration rerun complete | pending | Real cached account exercised across chats/channels; latest run found a 7.34 s 99-chat listing plus one remaining settings-harness false positive. |
| Android physical | yes | yes: physical Android device at `192.0.2.125:5555` | intentionally not tested | intentionally not tested | User directed all Android E2E to Waydroid. Release `1.0.0+1` was rebuilt and reinstalled in place without clearing data. |
| Android Waydroid | yes | yes: running at `192.0.2.112:5555` | pending | expanded fixture action sweep passed | Seventeen platform-device UI tests now pass in one whole-file run. They cover navigation, mentions, self-chat actions/reactions/media, settings, diagnostics, call controls, and expected no-camera paths. Live-account UI remains pending. |
| Windows | yes | yes: running QEMU Windows VM | peer call signaling/audio proven; bidirectional video pending | pending | Test Peer has received Microslop calls classified as video, but bidirectional video is not yet proven. Late Microsoft media renegotiation remains the active blocker. |
| Web/Chromium | yes | yes: `/usr/bin/chromium` | live smoke passed | pending | Live cached-session smoke passed at the bridge origin; exhaustive browser action sweep remains. |
| macOS | yes | no native macOS runtime known | blocked until runtime found | blocked until runtime found | Flutter target directory alone is not evidence of a runnable development host environment. |
| iOS | yes | no iOS runtime known | blocked until runtime found | blocked until runtime found | Requires macOS/Xcode or a connected compatible device/toolchain. |

## Action inventory

### Global/auth/workspace

- [ ] Restore cached session and verify identity/profile photo.
- [ ] Sign-in surface navigation without invalidating cached credentials.
- [ ] Account/profile menu and account switching surfaces.
- [ ] Workspace loading, retry/error state, and refresh paths.
- [ ] Wide desktop navigation.
- [ ] Narrow/mobile drawer navigation.
- [ ] Back/escape/system-back behavior.
- [ ] Search-as-you-type, clear search, no-match state, and result selection.
- [ ] Teams/channels expansion, selection, scrolling, and pagination.
- [ ] DMs/groups selection, scrolling, caching, and pagination.

### Conversation read-only coverage

- [ ] Read representative self-chat, DM, group chat, and channel.
- [ ] Sender avatars/profile images.
- [ ] Group/team/channel avatars.
- [ ] Quoted replies and reply previews.
- [ ] Reactions display.
- [ ] Image/GIF/media rendering.
- [ ] Links and other rich message content.
- [ ] Timestamps/date separators/read state presentation.
- [ ] Long histories/infinite scroll and cache revisit without spinner regressions.

### Self-chat mutation coverage (`brandonp2412` only)

- [ ] Send plain text.
- [ ] Send multiline text.
- [ ] Emoji/special characters.
- [ ] Reply/quote to a message.
- [ ] Add/change/remove every supported reaction.
- [ ] Attach/upload an image through each available input path.
- [ ] Paste an image/clipboard content where supported.
- [ ] Exercise keyboard GIF/image insertion where supported.
- [ ] Any edit/delete/copy/share/context-menu action exposed by Microslop, if available.
- [ ] Verify optimistic ordering/sidebar bump and round-trip persistence after each mutation.

### Calls/media

- [ ] Real Windows peer video call with bidirectional video proven.
- [ ] Microsoft Test Call from settings.
- [ ] Microphone mute/unmute.
- [ ] Speaker/earpiece/output toggle where supported.
- [ ] Camera enable/disable where supported.
- [ ] Hang up and post-call cleanup.
- [ ] Microphone test meter/input detection.
- [ ] Camera preview/test surface.
- [ ] Incoming-call UI can be inspected only with a safe test source; never call another person to provoke it.

### Settings/notifications

- [ ] Open every settings section and nested page.
- [ ] Exercise non-mutating selectors and previews.
- [ ] In a live account/runtime, persistent settings are inspect-only. Exercise setting mutations only with isolated/mock preferences and restore original state within the same test. Self-chat-scoped preferences may be toggled and restored because they affect only `brandonp2412`.
- [ ] Notification settings and grouped notification UI.
- [ ] Account-management UI without signing out/removing accounts.
- [ ] Diagnostics/log/about surfaces.

## Session log

### 2026-09-05 — Initial inventory

- Repository clean on `main` at start.
- Confirmed Flutter target directories: Android, iOS, Linux, macOS, web, Windows.
- `flutter devices` initially reported only Linux desktop, while direct ADB discovery found an physical Android device at `192.0.2.125:5555` plus a TV test device. The phone is the relevant physical Android target.
- Waydroid is installed but its session is currently stopped.
- Chromium is available at `/usr/bin/chromium`.
- Existing integration coverage found for Linux live UI exploration, live account backend flow, live DM sidebar, Waydroid fixture UI, web live-account smoke, and several Windows A/V/media probes.
- Existing Linux exploration already protects CUD by requiring the configured self-chat, but it samples only part of the UI and sends only one text message; it is not sufficient for the requested exhaustive action sweep.
- First live Linux exploration completed successfully against the cached account. It traversed representative chats/channels, search, settings entry, responsive layouts, and sent one permitted marker to `brandonp2412`.
- Linux findings: listing 99 chats took 5.77 seconds; duplicate display names were reported and need classification before treating them as defects; two settings findings were harness false positives (`App notifications` was renamed to `Enable notifications`, and `Make a test call` exists lower in the settings scroll).
- Windows runtime discovery: a running QEMU Windows VM is present on development host with VNC display `:0` and the existing `/home/brandonp2412/windows-share` staging area.
- Source action inventory now includes account menu, chat/channel context menus, hidden-items restore, data-sync controls, microphone/speaker/camera previews, debug/notification surfaces, attachment sheet, image gallery actions, and all six base reactions.
- Comprehensive fake-backed widget suite completed: 204 tests passed, covering destructive/update UI branches without touching live Teams data.
- Waydroid navigation was hardened to target stable conversation keys. The complete six-test device file now passes in one run: mobile navigation/mentions, long-press reaction, optimistic send, image/channel rendering, Android call controls/audio routing, and the expected missing-camera path. The Microsoft Test Call is correctly audio-only; the previous video expectation was stale test logic.
- User explicitly directed that the physical Android device not be used for testing. All further Android E2E is Waydroid-only.
- Built the current `app-release.apk` (versionCode 1, versionName 1.0.0) and reinstalled it in place on the physical Android device with `adb install -r`; installation succeeded without uninstalling or clearing app data.
- Web live-account smoke passed in Chromium at `http://127.0.0.1:18443`, covering cached session restore, read-only chat/channel access, self-chat text/reaction/image round trips, realtime event routing, and Microsoft Test Call media.
- Linux live exploration rerun completed successfully against the real cached account. It exercised the real chat/channel corpus through UI selection while restricting the only live write to `brandonp2412`. Findings were a 7.34 s 99-chat listing and a stale `Make a test call` harness miss caused by lazy settings construction.
- Windows remains blocked from this development host tool namespace despite the VM running. macOS and iOS remain unavailable on development host.
- Expanded the Waydroid device suite from six to seventeen tests. A whole-file run passed all 17 tests after exercising mobile navigation/mention completion, reaction long-press UI, optimistic send, image/channel rendering, Microsoft Test Call controls, self-chat favourite/mute/hide/restore, all six base reactions add/remove, attachment/gallery UI, settings toggles/hidden items, message-limit selector, message sync, microphone/speaker/camera selectors and previews, notification/debug diagnostics, account-menu inspection, and the expected missing-camera call path.
- The Waydroid gallery copy action uncovered a real Android crash in `ClipboardService`: image-only clipboard data could produce a null text item, and Microslop had not registered the `SuperClipboardDataProvider` required by `super_clipboard`. Added the provider using `${applicationId}.SuperClipboardDataProvider` and a plain-text fallback for copied images. Re-ran the gallery copy action on Waydroid successfully with no crash.
- Settings/media device coverage was split into isolated device tests so each selector and preview is clicked in a fresh state; the final whole-file Waydroid run remained green.
- No Android E2E runner has been used on the physical Android device. The physical Android device release must be rebuilt/reinstalled once more after the clipboard production fix so its installed release matches the tested code.
- Next: run the broader fake-backed action suite on Waydroid/web where supported, finish the remaining live Linux/web UI coverage, retry Windows control from any reachable guest path, then update the physical Android device release and commit the completed harness/product fixes.

### 2026-09-05 — Self-chat image CLS

- Reproduced the `brandonp2412` self-chat history path on live Linux and traced scrolling jumps to message image slots being inserted/resized after cached or remote bytes arrived.
- Message attachments now reserve a stable 3:2 layout slot before image bytes resolve, including text-plus-image messages and multiple attachments, so image decode/download completion does not change historical row height.
- Added widget regression coverage that asserts the message bubble is exactly the same size before and after delayed image completion.
- Live Linux self-chat integration coverage scrolls through historical messages and verifies visible message positions remain stable for one second after each upward history scroll while image data loads.
- The live run also exposed `WorkspaceDatabase.saveImages` binding five values into a six-column insert. Added `source_url` to the binding and regression coverage, eliminating repeated image-cache persistence failures.
- `flutter analyze`, the image-focused widget tests, the native database test, and the live Linux self-chat CLS integration test all pass after the fixes.
- Follow-up physical Android device feedback reported a smaller residual history jump. A 390×800 narrow-layout regression reproduced a 42 px shift when a live refresh inserted a new newest message while history was scrolled upward.
- Chat rows now have stable message keys plus `findChildIndexCallback`, and chat scroll physics preserves the visible history anchor when chronological history changes only at the newest edge. The regression now holds common visible messages within 0.5 px through that refresh.
- Background image hydration now persists downloaded image bytes without replacing the entire in-memory message list; visible rows already own their lazy image loading, so the redundant list-wide mutation only created rebuild churn.
- Re-ran the delayed-image height regression, the narrow live-refresh anchor regression, `flutter analyze`, and the live cached-account Linux self-chat CLS integration test. All pass, including repeated Teams `MessageLoss` reconcile events during the live run.

### 2026-09-05 — Scroll CLS and code blocks

- The remaining history shift was a same-length refresh case: a newer off-screen message could gain height, such as when its reaction tray appeared, while the previous anchor logic only compensated for newest-edge message count changes.
- Same-conversation message-list replacements now preserve the visible history anchor whenever the user is scrolled away from the newest edge. The narrow-layout regression covers both appending a newest message and adding a reaction to a newer off-screen row, holding shared visible rows within 0.5 px.
- Fenced triple-backtick code blocks now render as bordered monospace blocks with horizontal scrolling and an optional language label. URLs inside code remain code rather than links or link previews.
- Teams message serialization maps fenced code to `<pre><code>` while escaping its contents, and incoming `<pre>` blocks are retained as fenced content instead of being flattened into ordinary text.
- `cargo test -p ost-core` passes all 53 tests and `flutter analyze` reports no issues. The two focused Flutter regressions and the live cached-account Linux self-chat CLS integration test pass.
- The full 204-test widget run reached 202 passes with two unrelated account-switch/call failures in the concurrently modified `workspace_tests.dart`; focused reruns reproduce the missing expected decline call without touching the chat rendering paths changed here.

### 2026-09-05 — Migrated image-cache CLS follow-up

- development host work was committed, pushed to `main`, and mirrored into the clean secondary host worktree `/home/brandonp2412/microslop-development-main` before development host shut down. secondary host's older dirty `master` checkout remains untouched.
- Added a read-only live Waydroid CLS harness that drives the real `brandonp2412` self-chat through the development host web bridge and measures per-frame drag deviation plus post-scroll settling. Fixed its HTTP requests to send `Content-Length`; development host powered down before a successful real-data measurement could complete.
- Isolated a production-upgrade path missed by fresh-cache tests: the `image_urls_json` migration defaults existing message rows to `[]` while previously cached bytes remain in `message_images`. Those messages could initially render with no attachment slot, then grow when cached image bytes loaded asynchronously.
- `WorkspaceDatabase.loadMessages` now derives the number of cached image rows without eagerly loading their bytes. `MessageSummary` carries that count through reaction-normalization copies, and message rendering reserves the correct number of 3:2 attachment slots before bytes resolve.
- Added database coverage for cached images with no stored URLs, a bubble-height regression for the migrated-cache path, and a 390×800 history-scroll regression that keeps shared visible rows within 0.5 px when the cached image becomes visible.
- `flutter analyze` is clean, all 208 widget tests pass, and both native workspace-database tests pass on secondary host. A real-data Android rerun remains blocked while development host is off because secondary host has neither Waydroid nor a restorable Microslop Microsoft session.

### 2026-09-06 — First-fling micro-stall follow-up

- physical Android device feedback reported that some chats were visually stable but could briefly pause during the first upward history fling after opening.
- Text-only message rows were still calling `WorkspaceDatabase.loadImages` as each row became visible, causing synchronous SQLite lookups on the UI isolate even when the message had no image metadata. Image-cache lookup is now skipped for ordinary text rows while preserving the legacy empty-attachment retry path.
- Cached chats also replaced their entire visible message list when the immediate network refresh returned equivalent content. That redundant refresh rebuilt the chat mid-fling, rewrote the cached message table, and launched another recent-image hydration pass.
- Refresh reconciliation now compares all server-visible message content, including quotes, image URLs/bytes, reactions, and reaction users. Equivalent refreshes retain the existing list and skip `setState`, message-cache persistence, and image hydration; real message/image/reaction changes still apply normally.
- Added a 390×800 regression that delays an equivalent refresh until an active history fling and verifies the same reversed chat `ListView` instance remains mounted and no extra hydration pass starts.
- `flutter analyze` is clean, all 209 widget tests pass, and both native workspace-database tests pass on secondary host. Existing history-anchor and migrated-image CLS regressions remain green. Android E2E remains Waydroid-only and unavailable while development host is off.

### 2026-09-07 — Windows real-call failure and desktop call controls

- Windows feedback showed the desktop app had regressed to one combined call action and a real outgoing video call failed with `User invite failed (404): failure querying object from remote store`, while the Microsoft Test Call continued to prove microphone/camera/media setup was functional.
- Restored distinct desktop audio and video call buttons while retaining the compact call picker on narrow/mobile layouts.
- Real 1:1 calls now prefer the most recent non-self message sender ID, which comes from Microsoft's actual sender identity, then fall back to conversation membership and finally the chat thread ID. Native signaling preserves that explicit peer identity instead of always reconstructing it from the thread string.
- Outgoing call failures are now written to `AppLog`, so release builds retain the Microsoft signaling error under Settings → Debug log instead of only printing useful call diagnostics in debug mode.
- Real-user answer waiting is cancellable and allows up to 120 seconds; Microsoft Test Call keeps its 30-second acceptance window.
- Regression coverage requires both desktop call buttons, verifies a real message sender identity overrides conflicting conversation metadata, verifies explicit identity wins over a conflicting thread identity, and verifies backend invite failures reach the application log.
- Validation on development host: `flutter analyze` clean; all 213 Flutter widget tests passed; root Rust tests passed (97 library + 109 binary); app Rust tests passed (10/10). The development host tunnel still cannot drive the Windows VM, so no real person was called during automation and native Windows live-call confirmation remains manual.
- Follow-up Windows evidence still returned Microsoft subcode 5152 from the separate user-invite request. Microslop's captured outgoing-call signaling shape places the callee directly in the initial `epconv` `participants.to` list, so real 1:1 calls now do the same and no longer issue the failing post-creation user invite. The obsolete user-invite payload and signaling path were removed. Focused Microsoft payload tests, app Rust tests, analyzer, all 213 Flutter widget tests, and the full root Rust suite pass after this change.
- The next Windows attempt progressed past participant resolution and was rejected with `SdpParsingError` because line 6 was `b=CT:99980`. The video-offer generator emitted session-level fields as `c=`, `t=`, `b=`, but SDP requires bandwidth before timing and the captured Teams offer uses `c=`, `b=`, `t=`. Moved `b=CT:99980` ahead of `t=0 0` and added a regression asserting the complete session-level prefix order.
- The following Windows attempt progressed to the ringing state but the local self-video stayed frozen and Trouter eventually closed while waiting for acceptance. Real outgoing video calls now start camera capture before the acceptance wait, publish every captured frame to the local preview, and hand that same capture stream to RTP after acceptance instead of reopening the camera. Trouter keepalives now live in `TrouterSocket.recv_frame`, so every long-running consumer sends the established 30-second heartbeat instead of only the general notification loop doing so. Root Rust tests pass (97 library + 109 binary), app Rust tests pass (10/10), and targeted rustfmt is clean; live Windows confirmation remains manual from the user environment.
- User confirmation after that patch: Windows self-preview is live, but real audio/video calls remain in the local ringing state for a long period and then disappear, while the same real calls work on Android. Settings → Debug log retained no reason. The 1:1 thread identity is now authoritative over cached sender/profile identity when it contains the signed-in caller, preventing a stale Windows cache from dialing a different participant. Thread parsing no longer guesses a peer when the caller is absent. Flutter now retains a diagnostic when a backend call ends while still dialing/ringing, and when the native call-event stream closes while a call is active, so another silent disappearance cannot occur without evidence.
- Read-only inspection of the physical Android device's installed `app.microslop` package showed it has not been updated since 2026-09-06 13:43:40, before the Windows signaling changes. Pulling and inspecting its native `libost_frb.so` confirmed that the Android build known to place real calls uses the original two-step flow: create the 1:1 conversation with an empty initial `participants.to`, then `POST .../add` with the callee. Current Windows had been changed to put the callee in the initial `epconv` request after a `/add` 5152 error, but that direct-recipient flow only reached local ringing and timed out. Restored the Android-proven two-step protocol while retaining the corrected thread-canonical callee identity, SDP field order, Windows live preview, Trouter keepalive, cancellable wait, and retained failure diagnostics.
- The 1:1 `epconv` response parser was already retaining Microsoft's exact `links.addParticipant` URL, but real-user invitation ignored it and always reconstructed `conversationController + /add`. The Echo/Test Call path already prefers the returned link. Real-user calls now also try Microsoft's returned add-participant URL first and only fall back to the derived controller URL on 404, preserving the Android-proven two-step sequence while avoiding a wrong controller/shard route on Windows. Response-link parsing now accepts both root `links` and `conversationResponse.links`, matching the existing wrapped `conversationController` handling so a wrapped Microsoft response cannot silently discard the authoritative add-participant URL.
- The outgoing acceptance loop still only parsed Socket.IO event frames beginning `5::`/`5:::` even though Trouter's own socket layer supports acknowledged events shaped `5:ACK_ID::JSON`. That caused real-user call acceptance/media-answer events with an ACK ID to be discarded before JSON parsing, leaving the local UI in `Ringing` until timeout. Acceptance extraction now handles ACK IDs and recursively unwraps nested string, `cp` gzip/base64, and `gp` base64 payloads. Three focused acceptance regressions pass, the full Rust workspace passes, Flutter analysis is clean, and the direct-video-call widget regression passes. A live human-call confirmation remains intentionally manual because automated validation must not ring another person.
- Live Windows confirmation after the ACK parser fix separated the paths: a real audio-only call connected successfully end to end, while a video call showed a live local camera preview and ringback until it terminated without producing a usable remote call. The audio recipient also reported that the call appeared like a Teams/group call rather than a normal 1:1 call. This exposed a sequencing mistake in the previous fallback: the native-looking initial-`epconv` recipient flow had been abandoned because it stayed `Ringing`, but that trial predated the ACK-ID acceptance fix. Real 1:1 calls now put the canonical peer directly in the initial `epconv` `participants.to` and do not add the person afterward; the empty-recipient form remains only for the Test Call bot flow. This restores direct-call semantics while retaining the fixed acceptance parser.

### 2026-09-07 — Native Windows outgoing-video signaling investigation

- Recovered Windows VM control through `docker exec windows-dev python3 /shared/vnc_input.py` and `/shared/hmp.py`. Used an isolated guest source tree at `C:\Users\brandonp2412\microslop-video-investigation-20260907`; preserved the existing guest checkout and the host's pre-existing uncommitted changes.
- Reproduced a late-acceptance failure with local HTTP/WebSocket servers on both Linux and native Windows: after an initial media answer, the active-call loop discarded a later `callAcceptance` instead of posting its acknowledgement. The regression failed before the fix and passed afterward. Ordinary outgoing calls now continue reading Trouter throughout the active period, acknowledge late acceptance, maintain socket heartbeats, and retain remote termination or signaling failure reasons.
- Added a regression for media-answer control links. The parser previously accepted nested `mediaAnswer.mediaContent` but discarded links alongside that content. It now preserves nested acknowledgement/call-leg/end links while retaining the existing root-link response form.
- Removed the native ringback stop triggered merely by receiving SDP; the Flutter call-state handler already stops ringback on connection or termination.
- Five focused signaling tests pass in native Windows with `audio,video-capture-windows` enabled (exit 0), including the two new regressions. The Rust workspace suite passes and Flutter analysis is clean. The broader call widget run passes 31/32 tests; the pre-existing compact dark call avatar expectation fails (`AN` missing) in the already-modified UI. Strict Clippy is blocked by the existing unnecessary `u64` cast in `src/calling/audio.rs`, also present in HEAD.
- The native Windows self-chat UI probe restored the cached account, selected `brandonp2412` by its conversation key after verifying the signed-in user's identity, clicked the video action, reached dialing/ringing, and retained Microsoft's explicit `Caller cannot call self` rejection (400/10129). The native local preview produced a 320×240 frame using the isolated synthetic-camera setting. This is setup/error-path coverage, not a completed peer video call or physical-camera verification.
- The isolated Windows build also logged a missing `sqlcipher.dll` while loading cached messages/images; the UI probe completed, but database packaging remains a separate finding. Artifacts are in `/home/brandonp2412/windows-share/self-video-ui.log`, `video-investigation.log`, and the associated scripts.
- No other person was called and no Android device was used. Full peer video confirmation requires a willing test recipient and an explicit exception to the self-chat-only rule; requested that direction after Microsoft rejected the permitted self-call.

### 2026-09-07 — Windows incoming direct and group calls

- Targeted Windows at the user's direction. Incoming notifications now decode the gzip HTTP-style Trouter envelope as well as existing nested JSON, `cp`, and `gp` envelopes. Removed the raw invitation/path filter so compressed deliveries can reach the parser. Microsoft notification types and decoding live in `crates/ost-microsoft`; the existing invitation-only parser remains compatible with CLI consumers.
- Group invitations preserve `groupChat.threadId` instead of assigning the caller identity as the conversation. Direct audio/video invitations retain their caller fallback and video modality.
- Incoming call-end/conversation-end events now clear matching pending calls and stop matching active audio/video sessions. Unrelated call IDs are ignored. Remote termination waits for the answer operation to finish so a hang-up received during media setup cannot be lost between pending and active state.
- Regression failures were observed before fixes for gzip audio/video delivery, group conversation identity, remote cancellation, and termination during answer setup. The Rust workspace passes (318 tests), ten incoming-focused Flutter widget tests pass, Flutter analysis is clean, and targeted formatting and strict library Clippy checks pass. Broader strict Clippy remains blocked by pre-existing `items_after_test_module` in `crates/ost-microsoft/src/teams.rs` and an unnecessary cast in `src/calling/audio.rs`.
- Native Windows compilation and tests passed in the isolated guest tree `C:\Users\brandonp2412\microslop-incoming-calls-20260907`: 12 Microsoft-boundary tests and 10 application bridge tests, exit 0. Logs: `/home/brandonp2412/windows-share/incoming-calls.log` and `incoming-calls.err.log`. This is native Windows backend coverage; the Flutter fixture UI checks ran on Linux. Neither is a live peer media test.
- No other person was called, no live group was joined, and no Android device was used. The existing incoming video path remains receive-only; local camera transmission, meeting-link joining, and live stand-up media connectivity are not established by this session. A real incoming group/direct call remains necessary to confirm server invitation shape and media transport on the user's Windows network. No release app was installed.

### 2026-09-08 — Windows peer video signaling investigation

- With explicit user authorization to call Test Peer, the real Windows Microslop video-call path was exercised end to end.
- Earlier attempts reached ringing but timed out after 120 seconds because outgoing-call Trouter registration/authentication and terminal callback handling were incomplete; one surfaced Microsoft `409 / 5704` ownership resolution failure when the callee was placed directly in the initial conversation request.
- The working path authenticates and activates the outgoing Trouter session, parses terminal conversation callbacks immediately, uses the negotiated endpoint identity, and restores the two-step call sequence: create the conversation first, then invite the callee via Microsoft's returned `addParticipant` link.
- During the successful sequence Microsoft reported `addParticipantSuccess` for Test Peer and delivered media renegotiation callbacks.
- The user confirmed the 10:52 NZST call connected and was recognized as a video call, but Test Peer could not see Microslop camera frames and Microslop did not render Test Peer video. This is signaling/connectivity evidence only, not bidirectional video proof.
- Refresh-token operations are serialized so rotating Microsoft refresh tokens cannot race during the multi-token call setup path.
- The temporary exception to the self-chat-only testing rule is complete; `AGENTS.md` has been restored to its normal testing policy.

### 2026-09-08 — Late media renegotiation follow-up

- Real trace inspection confirmed Test Peer's endpoint advertises a `sendrecv` `main-video` stream and Microsoft later sends `call/mediaRenegotiation` with a video SDP and a `mediaAnswer` URL.
- The first renegotiation handler posted `mediaContent` at the root and Microsoft rejected it with HTTP 400: `The MediaAnswer field is required.` The response is now wrapped as `mediaAnswer` and preserves `mediaLegId`.
- A subsequent live attempt reached Microsoft's renegotiation path but was terminated with `410 / 301000 SdpParsingFailure`, proving the payload envelope was accepted far enough for SDP validation. The answer builder now intersects offered audio/video payload types, removes unsupported RTVC1 lines, and rejects Microsoft's extra `x-data` media section with port 0 while retaining its `mid`.
- Focused Rust regressions cover the late renegotiation callback, `mediaAnswer` envelope, media-leg preservation, codec intersection, and active-call callback handling.
- A later Windows call at 12:00 NZST ended with `408 / 10056 Call Controller timed out while waiting for acknowledgement` before any late media renegotiation arrived. Microsoft resent `callAcceptance`, so acceptance acknowledgement is the current earlier blocker on that attempt. Additional trace markers were added to distinguish callback parsing, acknowledgement POST status, and active-call acknowledgement handling.
- The QEMU guest remains available, but the current VNC Run-dialog helper began interpreting path-bearing PowerShell commands as Windows Settings URIs. No fresh call was successfully launched after 12:00 while correcting that guest-control issue; the 12:00 trace must not be mistaken for a newer validation run.
- Bidirectional video remains the definition of done: Test Peer must see Microslop camera frames and Microslop must render Test Peer camera frames, with RTP/frame counters where possible.

### 2026-09-08 — Renegotiation transport application

- Late video renegotiation now updates the active video transport after Microsoft accepts the renegotiation answer. The sender and SRTCP loops read a shared remote address, while their existing SRTP context is replaced with the newly negotiated keys after fresh video ICE checks.
- Call-acceptance acknowledgement is now sent before checking whether that callback also supplied SDP, so Microsoft receives the required acknowledgement even for an acceptance-without-SDP failure path.
- Focused acceptance tests pass and the root call backend compiles with audio/video capture enabled. A fresh Windows peer call is still required to prove the changed transport is used by both camera transmission and remote-video reception.
- The first Windows run of this commit reached the acceptance callback but still received `408 / 10056` after Microsoft resent it. Acknowledgement and callback-registration requests had reused the initial call-create message ID; they now each use a new request ID, while tests assert that acknowledgement does not reuse the original ID.
- The subsequent Windows peer call still received `408 / 10056`, but its repeated Trouter `5:ACK_ID::...` call-acceptance frames exposed the remaining defect: Microslop emitted Socket.IO acknowledgements as `6:ACK_ID::`; Trouter expects `6:::ACK_ID+[]`. The outgoing Trouter client now emits the protocol-correct acknowledgement before media setup.
