# Rule of Three

Repeat behavior twice when that keeps it local. On the third use, extract the shared behavior. Do not abstract a one-off or second use.

# Microsoft Compatibility Boundary

Keep Microsoft-defined endpoints, scopes, client identities, header values, protocol strings, and request shapes in `crates/ost-microsoft`. Domain and UI code should consume that boundary instead of duplicating Microsoft-controlled details.

# No commenting

Never write comments. Unless they are docstrings and they must explain
things the code cannot explain.

# Minimal

Accomplish things with as little code as possible.

# Completion

When you make changes to a Git repository, always create a Git commit for your completed work before handing it back to the user. Do not commit unrelated pre-existing changes; stage only the files relevant to the task and use a clear commit message.

# Testing

Any Create, Update, or Delete actions as a test ought to be performed in the
self-chat channel **only**.

# Device Installation Safety

Before installing an Android build on a user's device, compare the installed package version code with the candidate build. Never uninstall the existing app, clear its data, or use a downgrade path that can remove app data unless the user explicitly approves that data loss. If an in-place install is not possible, stop and preserve the existing app data.

Never run `flutter test`, `flutter drive`, or another Android test runner against the user's installed production package. Flutter's device test cleanup can uninstall the package and destroy its app data. Use an isolated application ID on a disposable test install instead.
