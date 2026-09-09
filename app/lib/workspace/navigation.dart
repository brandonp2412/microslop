part of '../workspace.dart';

class _Navigation extends StatefulWidget {
  const _Navigation({
    required this.chats,
    required this.teams,
    required this.messagePreviews,
    required this.unreadConversationIds,
    required this.favoriteConversationIds,
    required this.mutedConversationIds,
    required this.presenceByUserId,
    required this.selected,
    required this.onSelect,
    required this.searchQuery,
    required this.searchFocus,
    required this.onSearchChanged,
    required this.splitGroups,
    required this.groupsExpanded,
    required this.directMessagesExpanded,
    required this.onGroupsExpandedChanged,
    required this.onDirectMessagesExpandedChanged,
    required this.groupsHeightFraction,
    required this.onGroupsHeightChanged,
    required this.gateway,
    required this.loading,
    required this.onLoadMoreChats,
    required this.canLoadMoreChats,
    required this.loadingMoreChats,
    required this.onHide,
    required this.hiddenSectionIds,
    required this.onHideSection,
    required this.onSettings,
  });
  final List<Conversation> chats;
  final List<TeamSummary> teams;
  final Map<String, String> messagePreviews;
  final Set<String> unreadConversationIds;
  final Set<String> favoriteConversationIds;
  final Set<String> mutedConversationIds;
  final Map<String, PresenceSummary> presenceByUserId;
  final Conversation? selected;
  final ValueChanged<Conversation> onSelect;
  final String searchQuery;
  final FocusNode searchFocus;
  final ValueChanged<String> onSearchChanged;
  final bool splitGroups;
  final bool groupsExpanded;
  final bool directMessagesExpanded;
  final ValueChanged<bool> onGroupsExpandedChanged;
  final ValueChanged<bool> onDirectMessagesExpandedChanged;
  final double groupsHeightFraction;
  final ValueChanged<double> onGroupsHeightChanged;
  final TeamsGateway gateway;
  final bool loading;
  final VoidCallback onLoadMoreChats;
  final bool canLoadMoreChats;
  final bool loadingMoreChats;
  final ValueChanged<Conversation> onHide;
  final Set<String> hiddenSectionIds;
  final void Function(String id, String title) onHideSection;
  final VoidCallback onSettings;

  @override
  State<_Navigation> createState() => _NavigationState();
}

class _NavigationState extends State<_Navigation> {
  late final TextEditingController _search = TextEditingController(
    text: widget.searchQuery,
  );
  bool _channelsExpanded = false;
  bool? _searchGroupsExpanded;
  bool? _searchDirectMessagesExpanded;
  List<Conversation>? _filterSourceChats;
  List<TeamSummary>? _filterSourceTeams;
  String? _filterQuery;
  bool? _filterSplitGroups;
  List<Conversation> _filteredChats = const [];
  List<Conversation> _filteredGroups = const [];
  List<Conversation> _filteredDirectMessages = const [];
  List<TeamSummary> _filteredTeams = const [];

  void _filterNavigation(String query) {
    if (identical(_filterSourceChats, widget.chats) &&
        identical(_filterSourceTeams, widget.teams) &&
        _filterQuery == query &&
        _filterSplitGroups == widget.splitGroups) {
      return;
    }
    _filterSourceChats = widget.chats;
    _filterSourceTeams = widget.teams;
    _filterQuery = query;
    _filterSplitGroups = widget.splitGroups;
    if (query.isEmpty) {
      _filteredChats = widget.chats;
      if (widget.splitGroups) {
        final groups = <Conversation>[];
        final directMessages = <Conversation>[];
        for (final chat in widget.chats) {
          (chat.isGroup ? groups : directMessages).add(chat);
        }
        _filteredGroups = groups;
        _filteredDirectMessages = directMessages;
      } else {
        _filteredGroups = _filteredDirectMessages = const [];
      }
      _filteredTeams = widget.teams;
      return;
    }
    _filteredChats = widget.chats.where((chat) {
      return chat.name.toLowerCase().contains(query) ||
          (chat.preview?.toLowerCase().contains(query) ?? false);
    }).toList();
    if (widget.splitGroups) {
      _filteredGroups = _filteredChats.where((chat) => chat.isGroup).toList();
      _filteredDirectMessages = _filteredChats
          .where((chat) => !chat.isGroup)
          .toList();
    } else {
      _filteredGroups = _filteredDirectMessages = const [];
    }
    _filteredTeams = widget.teams
        .map((team) {
          final teamMatches = team.name.toLowerCase().contains(query);
          final channels = team.channels
              .where(
                (channel) =>
                    teamMatches || channel.name.toLowerCase().contains(query),
              )
              .toList();
          return TeamSummary(id: team.id, name: team.name, channels: channels);
        })
        .where((team) => team.channels.isNotEmpty)
        .toList();
  }

  @override
  void didUpdateWidget(covariant _Navigation oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.searchQuery != _search.text) _search.text = widget.searchQuery;
    if (oldWidget.searchQuery.trim().isEmpty !=
        widget.searchQuery.trim().isEmpty) {
      _searchGroupsExpanded = null;
      _searchDirectMessagesExpanded = null;
    }
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final query = widget.searchQuery.trim().toLowerCase();
    _filterNavigation(query);
    final chats = _filteredChats;
    final groups = _filteredGroups;
    final directMessages = _filteredDirectMessages;
    final searching = query.isNotEmpty;
    final groupsExpanded = searching
        ? _searchGroupsExpanded ?? true
        : widget.groupsExpanded;
    final directMessagesExpanded = searching
        ? _searchDirectMessagesExpanded ?? true
        : widget.directMessagesExpanded;
    final teams = _filteredTeams;
    final visibleSectionCount = 3 - widget.hiddenSectionIds.length;
    final showSectionTitles = visibleSectionCount != 1;
    return LayoutBuilder(
      builder: (context, constraints) {
        final channelsMaxHeight = (constraints.maxHeight - 280)
            .clamp(48.0, 220.0)
            .toDouble();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 12, 24, 12),
              child: TextField(
                controller: _search,
                focusNode: widget.searchFocus,
                onChanged: widget.onSearchChanged,
                decoration: InputDecoration(
                  hintText: 'Search...',
                  prefixIcon: const Icon(Icons.search),
                  suffixIcon: widget.searchQuery.isEmpty
                      ? null
                      : IconButton(
                          tooltip: 'Clear search',
                          icon: const Icon(Icons.clear),
                          onPressed: () => widget.onSearchChanged(''),
                        ),
                ),
              ),
            ),
            if (teams.isNotEmpty &&
                !widget.hiddenSectionIds.contains(
                  _TeamsWorkspaceState._channelsSectionId,
                ))
              _ChannelsSection(
                teams: teams,
                selected: widget.selected,
                onSelect: widget.onSelect,
                expanded: _channelsExpanded || query.isNotEmpty,
                onExpandedChanged: (expanded) =>
                    setState(() => _channelsExpanded = expanded),
                gateway: widget.gateway,
                messagePreviews: widget.messagePreviews,
                onHide: widget.onHide,
                favoriteConversationIds: widget.favoriteConversationIds,
                mutedConversationIds: widget.mutedConversationIds,
                maxExpandedHeight: channelsMaxHeight,
                showTitle: showSectionTitles,
                onHideSection: () => widget.onHideSection(
                  _TeamsWorkspaceState._channelsSectionId,
                  'Channels',
                ),
              ),
            Expanded(
              child: widget.loading
                  ? const Center(child: CircularProgressIndicator())
                  : chats.isEmpty
                  ? Center(
                      child: Text(
                        query.isEmpty
                            ? 'No chats available.'
                            : 'No chats found.',
                      ),
                    )
                  : widget.splitGroups
                  ? LayoutBuilder(
                      builder: (context, constraints) => Column(
                        children: [
                          if (showSectionTitles &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._groupsSectionId,
                              ))
                            _ChatSectionHeader(
                              title: 'Groups',
                              expanded: groupsExpanded,
                              onChanged: searching
                                  ? (expanded) => setState(
                                      () => _searchGroupsExpanded = expanded,
                                    )
                                  : widget.onGroupsExpandedChanged,
                              onLongPress: () => widget.onHideSection(
                                _TeamsWorkspaceState._groupsSectionId,
                                'Groups',
                              ),
                            ),
                          if (groupsExpanded &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._groupsSectionId,
                              ))
                            Expanded(
                              flex: (widget.groupsHeightFraction * 1000)
                                  .round(),
                              child: _ChatList(
                                storageKey: 'group-chat-list',
                                chats: groups,
                                selected: widget.selected,
                                onSelect: widget.onSelect,
                                scheme: scheme,
                                gateway: widget.gateway,
                                messagePreviews: widget.messagePreviews,
                                unreadConversationIds:
                                    widget.unreadConversationIds,
                                favoriteConversationIds:
                                    widget.favoriteConversationIds,
                                mutedConversationIds:
                                    widget.mutedConversationIds,
                                presenceByUserId: widget.presenceByUserId,
                                onHide: widget.onHide,
                                onLoadMore: widget.onLoadMoreChats,
                                canLoadMore: widget.canLoadMoreChats,
                                loadingMore: widget.loadingMoreChats,
                              ),
                            ),
                          if (groupsExpanded &&
                              directMessagesExpanded &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._groupsSectionId,
                              ) &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._directMessagesSectionId,
                              ))
                            _ResizeHandle(
                              axis: Axis.vertical,
                              onDrag: (delta) => widget.onGroupsHeightChanged(
                                (widget.groupsHeightFraction +
                                        delta / constraints.maxHeight)
                                    .clamp(.2, .8),
                              ),
                            ),
                          if (showSectionTitles &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._directMessagesSectionId,
                              ))
                            _ChatSectionHeader(
                              title: 'Direct messages',
                              expanded: directMessagesExpanded,
                              onChanged: searching
                                  ? (expanded) => setState(
                                      () => _searchDirectMessagesExpanded =
                                          expanded,
                                    )
                                  : widget.onDirectMessagesExpandedChanged,
                              onLongPress: () => widget.onHideSection(
                                _TeamsWorkspaceState._directMessagesSectionId,
                                'Direct messages',
                              ),
                            ),
                          if (directMessagesExpanded &&
                              !widget.hiddenSectionIds.contains(
                                _TeamsWorkspaceState._directMessagesSectionId,
                              ))
                            Expanded(
                              flex: ((1 - widget.groupsHeightFraction) * 1000)
                                  .round(),
                              child: _ChatList(
                                storageKey: 'direct-chat-list',
                                chats: directMessages,
                                selected: widget.selected,
                                onSelect: widget.onSelect,
                                scheme: scheme,
                                gateway: widget.gateway,
                                messagePreviews: widget.messagePreviews,
                                unreadConversationIds:
                                    widget.unreadConversationIds,
                                favoriteConversationIds:
                                    widget.favoriteConversationIds,
                                mutedConversationIds:
                                    widget.mutedConversationIds,
                                presenceByUserId: widget.presenceByUserId,
                                onHide: widget.onHide,
                                onLoadMore: widget.onLoadMoreChats,
                                canLoadMore: widget.canLoadMoreChats,
                                loadingMore: widget.loadingMoreChats,
                              ),
                            ),
                          if (!groupsExpanded && !directMessagesExpanded)
                            const Expanded(
                              child: Center(
                                child: Text(
                                  'Expand a chat section to view chats.',
                                ),
                              ),
                            ),
                        ],
                      ),
                    )
                  : _ChatList(
                      storageKey: 'chat-list',
                      chats: chats,
                      selected: widget.selected,
                      onSelect: widget.onSelect,
                      scheme: scheme,
                      gateway: widget.gateway,
                      messagePreviews: widget.messagePreviews,
                      unreadConversationIds: widget.unreadConversationIds,
                      favoriteConversationIds: widget.favoriteConversationIds,
                      mutedConversationIds: widget.mutedConversationIds,
                      presenceByUserId: widget.presenceByUserId,
                      onHide: widget.onHide,
                      onLoadMore: widget.onLoadMoreChats,
                      canLoadMore: widget.canLoadMoreChats,
                      loadingMore: widget.loadingMoreChats,
                    ),
            ),
            const Divider(height: 1),
            ListTile(
              leading: const Icon(Icons.settings_outlined),
              title: const Text('Settings'),
              onTap: widget.onSettings,
            ),
          ],
        );
      },
    );
  }
}

class _ChannelsSection extends StatelessWidget {
  const _ChannelsSection({
    required this.teams,
    required this.selected,
    required this.onSelect,
    required this.expanded,
    required this.onExpandedChanged,
    required this.gateway,
    required this.messagePreviews,
    required this.onHide,
    required this.favoriteConversationIds,
    required this.mutedConversationIds,
    required this.maxExpandedHeight,
    required this.showTitle,
    required this.onHideSection,
  });

  final List<TeamSummary> teams;
  final Conversation? selected;
  final ValueChanged<Conversation> onSelect;
  final bool expanded;
  final ValueChanged<bool> onExpandedChanged;
  final TeamsGateway gateway;
  final Map<String, String> messagePreviews;
  final ValueChanged<Conversation> onHide;
  final Set<String> favoriteConversationIds;
  final Set<String> mutedConversationIds;
  final double maxExpandedHeight;
  final bool showTitle;
  final VoidCallback onHideSection;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (showTitle)
          _ChatSectionHeader(
            title: 'Channels',
            expanded: expanded,
            onChanged: onExpandedChanged,
            onLongPress: onHideSection,
          ),
        if (expanded || !showTitle)
          ConstrainedBox(
            constraints: BoxConstraints(maxHeight: maxExpandedHeight),
            child: ListView(
              key: const PageStorageKey('channel-list'),
              padding: const EdgeInsets.fromLTRB(10, 0, 10, 2),
              shrinkWrap: true,
              children: [
                for (final team in teams) ...[
                  Padding(
                    padding: const EdgeInsets.fromLTRB(10, 4, 10, 1),
                    child: Text(
                      team.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.labelMedium?.copyWith(
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                  ),
                  for (final channel in team.channels)
                    _ContextMenu(
                      actions: {
                        'Open channel': () => onSelect(channel),
                        'Channel options': () => onHide(channel),
                        'Copy name': () => Clipboard.setData(
                          ClipboardData(text: channel.name),
                        ),
                      },
                      child: ListTile(
                        key: ValueKey(
                          'conversation-tile-channel-${channel.id}',
                        ),
                        dense: true,
                        visualDensity: VisualDensity.compact,
                        minVerticalPadding: 0,
                        selected: selected == channel,
                        selectedTileColor: scheme.secondaryContainer,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(10),
                        ),
                        leading: _TeamAvatar(
                          name: team.name,
                          teamId: team.id,
                          gateway: gateway,
                          radius: 14,
                        ),
                        title: Text(
                          channel.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle:
                            messagePreviews[channel.id]?.isNotEmpty == true
                            ? Text(
                                messagePreviews[channel.id]!,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              )
                            : null,
                        trailing: _ConversationStatusIcons(
                          favorite: favoriteConversationIds.contains(
                            channel.id,
                          ),
                          muted: mutedConversationIds.contains(channel.id),
                        ),
                        onTap: () => onSelect(channel),
                        onLongPress: _desktopPlatform
                            ? null
                            : () => onHide(channel),
                      ),
                    ),
                ],
              ],
            ),
          ),
      ],
    );
  }
}

class _ChatList extends StatelessWidget {
  const _ChatList({
    required this.storageKey,
    required this.chats,
    required this.selected,
    required this.onSelect,
    required this.scheme,
    required this.gateway,
    required this.messagePreviews,
    required this.unreadConversationIds,
    required this.favoriteConversationIds,
    required this.mutedConversationIds,
    required this.presenceByUserId,
    this.onLoadMore,
    this.canLoadMore = false,
    this.loadingMore = false,
    required this.onHide,
  });
  final String storageKey;
  final List<Conversation> chats;
  final Conversation? selected;
  final ValueChanged<Conversation> onSelect;
  final ColorScheme scheme;
  final TeamsGateway gateway;
  final Map<String, String> messagePreviews;
  final Set<String> unreadConversationIds;
  final Set<String> favoriteConversationIds;
  final Set<String> mutedConversationIds;
  final Map<String, PresenceSummary> presenceByUserId;
  final VoidCallback? onLoadMore;
  final bool canLoadMore;
  final bool loadingMore;
  final ValueChanged<Conversation> onHide;

  @override
  Widget build(BuildContext context) => Column(
    children: [
      Expanded(
        child: NotificationListener<ScrollNotification>(
          onNotification: (notification) {
            if (canLoadMore &&
                !loadingMore &&
                notification.metrics.extentAfter < 240) {
              onLoadMore?.call();
            }
            return false;
          },
          child: ListView.separated(
            key: PageStorageKey(storageKey),
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 8),
            itemCount: chats.length + (loadingMore ? 1 : 0),
            separatorBuilder: (_, _) => const SizedBox(height: 2),
            itemBuilder: (context, index) {
              if (index >= chats.length) {
                return const Padding(
                  padding: EdgeInsets.all(12),
                  child: Center(
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                );
              }
              if (index == chats.length - 1 && canLoadMore && !loadingMore) {
                WidgetsBinding.instance.addPostFrameCallback(
                  (_) => onLoadMore?.call(),
                );
              }
              final chat = chats[index];
              final isSelected = selected == chat;
              final isUnread = unreadConversationIds.contains(chat.id);
              return Material(
                color: isSelected
                    ? scheme.secondaryContainer
                    : Colors.transparent,
                borderRadius: BorderRadius.circular(10),
                child: _ContextMenu(
                  actions: {
                    'Open chat': () => onSelect(chat),
                    'Chat options': () => onHide(chat),
                    'Copy name': () =>
                        Clipboard.setData(ClipboardData(text: chat.name)),
                  },
                  child: ListTile(
                    key: ValueKey('conversation-tile-chat-${chat.id}'),
                    dense: true,
                    visualDensity: VisualDensity.compact,
                    minVerticalPadding: 0,
                    contentPadding: const EdgeInsets.symmetric(horizontal: 10),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                    ),
                    leading: _ConversationAvatar(
                      conversation: chat,
                      gateway: gateway,
                      selected: isSelected,
                      radius: 16,
                      presence: chat.profilePhotoUserId == null
                          ? null
                          : presenceByUserId[_userIdKey(
                              chat.profilePhotoUserId!,
                            )],
                    ),
                    title: Text(
                      chat.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: isUnread
                          ? const TextStyle(fontWeight: FontWeight.w600)
                          : null,
                    ),
                    subtitle: Text(
                      messagePreviews[chat.id] ??
                          (chat.preview?.trim().isNotEmpty == true
                              ? chat.preview!
                              : 'No recent messages'),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        _ConversationStatusIcons(
                          favorite: favoriteConversationIds.contains(chat.id),
                          muted: mutedConversationIds.contains(chat.id),
                        ),
                        if (isUnread) ...[
                          const SizedBox(width: 6),
                          Icon(Icons.circle, size: 8, color: scheme.primary),
                        ],
                      ],
                    ),
                    selected: isSelected,
                    onTap: () => onSelect(chat),
                    onLongPress: _desktopPlatform ? null : () => onHide(chat),
                  ),
                ),
              );
            },
          ),
        ),
      ),
    ],
  );
}

class _ConversationStatusIcons extends StatelessWidget {
  const _ConversationStatusIcons({required this.favorite, required this.muted});

  final bool favorite;
  final bool muted;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      if (favorite) const Icon(Icons.star_rounded, size: 16),
      if (favorite && muted) const SizedBox(width: 4),
      if (muted) const Icon(Icons.notifications_off_outlined, size: 16),
    ],
  );
}

class _ChatSectionHeader extends StatelessWidget {
  const _ChatSectionHeader({
    required this.title,
    required this.expanded,
    required this.onChanged,
    this.onLongPress,
  });

  final String title;
  final bool expanded;
  final ValueChanged<bool> onChanged;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(12, 5, 12, 2),
    child: _ContextMenu(
      actions: {
        expanded ? 'Collapse' : 'Expand': () => onChanged(!expanded),
        if (onLongPress != null) 'Hide section': onLongPress!,
      },
      child: ListTile(
        dense: true,
        visualDensity: VisualDensity.compact,
        minVerticalPadding: 0,
        contentPadding: const EdgeInsets.symmetric(horizontal: 10),
        title: Text(title, style: Theme.of(context).textTheme.titleSmall),
        trailing: Icon(expanded ? Icons.expand_less : Icons.expand_more),
        onTap: () => onChanged(!expanded),
        onLongPress: _desktopPlatform ? null : onLongPress,
      ),
    ),
  );
}
