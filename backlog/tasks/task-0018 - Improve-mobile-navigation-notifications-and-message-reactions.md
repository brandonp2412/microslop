---
id: TASK-0018
title: 'Improve mobile navigation, notifications, and message reactions'
status: Done
assignee:
  - '@codex'
created_date: '2026-08-29 04:09'
updated_date: '2026-08-29 04:19'
labels:
  - flutter
  - mobile
  - android
  - ux
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring the focused conversation layout, mobile navigation, notification settings and delivery, app bar actions, and mobile reaction interaction in line with the cross-platform product experience, with evidence from a real Android device.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Focused channels, DMs, and other conversations do not repeat their title in the content area
- [x] #2 Mobile exposes the same navigation categories available on desktop
- [x] #3 Notification settings use cross-platform wording rather than desktop-specific wording
- [x] #4 Android test notifications visibly deliver or report an actionable failure without a false success claim
- [x] #5 The app bar has no refresh button
- [x] #6 Long-pressing a message on mobile opens the reaction bar
- [x] #7 Debug logs record sufficient notification and interaction state to validate the requested behaviors on a connected Android phone
- [x] #8 A test notification is visibly observed in the connected Android phone notification shade and corroborated by app and Android system logs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add failing widget and notification service tests for app-bar titles, adaptive mobile categories, platform-neutral settings, Android permission/delivery outcomes, refresh removal, long-press reactions, and privacy-safe diagnostics.
2. Refactor notification abstractions and Android initialization to request/check runtime permission, configure an Android channel, supply Android notification details, and return truthful delivery outcomes.
3. Update the workspace app bar, adaptive navigation categories, settings feedback, and mobile long-press reaction overlay while preserving desktop hover behavior.
4. Add structured privacy-safe logs for responsive navigation, selected app-bar state, reaction overlay activation, and notification initialization, permission, channel, request, and result stages.
5. Run focused tests after each change, then Flutter formatting, static analysis, the complete Flutter test suite, and relevant Rust checks if touched.
6. Run the debug app on Android device RFGYC3VTPAP, exercise all requested flows, visibly verify the test notification in the phone notification shade, and corroborate results with Flutter and adb system logs.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Understanding confirmed: show “Chats” only when no conversation is selected; selected conversations have no app-bar title.
- Design decision: reuse adaptive navigation and reaction components instead of creating mobile-only duplicates.
- Notification E2E requires visible phone notification-shade evidence plus app and Android system logs; plugin completion alone is insufficient.

- Added regression tests and observed the required failures before implementation.
- Targeted `dart analyze lib test` completed with no issues; all 43 Flutter tests pass.
- Real-device E2E on SM S931B / Android 16 via `flutter run -d RFGYC3VTPAP`: semantics confirmed Channels, Groups, and Direct messages in the mobile drawer; selected chat had no title or Refresh action; adb long-press exposed the reaction menu.
- Android notification E2E: runtime POST_NOTIFICATIONS prompt was shown and allowed; app logs recorded channel creation, permission false→true, submission, and platform acceptance. `dumpsys notification` reported one enqueued/posted notification at importance 4 on `microslop_messages`; expanded System UI notification shade visibly contained “Microslop test notification” and “Notifications are working.”
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Improved the Flutter mobile workspace and repaired Android local notification delivery.

Changes:
- Removed focused conversation titles from the app bar/content header while retaining “Chats” before selection, and removed the manual app-bar refresh action.
- Exposed Channels, Groups, Direct messages, and Hidden through the same categorized navigation on mobile.
- Reworded notification settings for a cross-platform app and added truthful success/failure feedback.
- Added Android notification permission handling, a high-importance notification channel, Android notification details, and privacy-safe stage diagnostics.
- Added mobile long-press reaction-bar behavior while preserving desktop hover reactions and selectable desktop message text.
- Added widget regression coverage for the requested UI behavior.

Validation:
- `dart analyze lib test`
- `flutter test` (43 tests passed)
- `flutter run -d RFGYC3VTPAP` on SM S931B / Android 16
- Verified app semantics/logs for mobile categories, title/refresh removal, and long-press reactions.
- Verified notification permission, channel, post record, System UI shade title/body, and Android NotificationManager statistics using adb.
<!-- SECTION:FINAL_SUMMARY:END -->
