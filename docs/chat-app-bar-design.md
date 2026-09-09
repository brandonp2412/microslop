# Chat app bar design

## Intent

- Show the selected conversation's avatar and name in the workspace app bar.
- Put audio and video call controls in the app bar for direct chats.
- Hide the signed-in user's account avatar while a conversation is selected.
- Remove the duplicate conversation header above the message list.
- Preserve the existing navigation, diagnostics, avatar caching, and call state.

## Assumptions

- This is a presentation-only change with no new performance, scale, privacy, or
  reliability requirements.
- Both call controls use the existing Teams call-start operation because the
  current call gateway does not expose a separate video-call mode.
- Account settings remain available from the app bar when no chat is selected.

## Decision log

- Reuse `_ConversationAvatar` in the app bar so its existing direct, group, and
  channel avatar behavior remains consistent with navigation.
- Keep the application-log action visible in all states.
- Limit call controls to one-to-one chats, matching the existing call support.
- Remove the entire in-pane header to avoid duplicated identity and controls.
