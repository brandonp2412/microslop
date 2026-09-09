# Workspace navigation and prefetch design

## Intent

- A long press on Channels, Groups, or Direct messages offers to hide the whole section.
- Hidden sections are persisted and can be restored from Settings.
- The last selected conversation opens on launch, using cached messages before refreshing.
- The first navigation screen is warmed without issuing an unbounded burst of Teams requests.
- A self-chat is identified by the signed-in user's ID, not its display name.

## Decisions

- Keep workspace state in the existing `CachedWorkspace` JSON instead of introducing a database.
- Prefetch at most eight conversations in direct-message, group, then channel order.
- Use two workers and wait 200 ms after each request. Individual failures remain retryable and do
  not fail workspace loading.
- Treat the persisted Rust chat list as an offline fallback. Always attempt a network refresh so a
  newly created or newly active chat can appear.
- For a Graph `oneOnOne` chat whose members are all the current user, retain that user ID and use
  the current user's display name.

## Alternatives considered

- A general sync database and scheduler would support larger offline histories, but adds schema,
  migration, and invalidation work that is unnecessary for an eight-conversation warm cache.
- Prefetching every loaded chat would improve more cache hits but creates avoidable throttling risk.
