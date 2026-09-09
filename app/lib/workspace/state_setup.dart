part of '../workspace.dart';

extension _WorkspaceSetup on _TeamsWorkspaceState {
  Future<void> _initializeWorkspace() async {
    await _loadNotificationPreferences();
    if (!mounted) return;
    if (widget.gateway is ResponsiveTeamsGateway) {
      _databaseReady ??= _database.initialize().catchError((Object error) {
        AppLog.record('Initialize message cache', error);
      });
    }
    await _restoreCachedChats();
    if (!mounted) return;
    await _hydrateCachedNavigation();
    await _loadWorkspace();
    _initialWorkspaceLoadComplete = true;
  }

  Future<void> _loadNotificationPreferences() async {
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    final enabled = prefs.getBool('notifications.enabled') ?? true;
    _mutate(() {
      _notificationsEnabled = enabled;
      _mentionNotificationsEnabled =
          prefs.getBool('notifications.mentions') ?? true;
      _directMessageNotificationsEnabled =
          prefs.getBool('notifications.directMessages') ?? true;
      _groupNotificationsEnabled =
          prefs.getBool('notifications.groups') ?? false;
      _channelNotificationsEnabled =
          prefs.getBool('notifications.channels') ?? false;
      _reactionNotificationsEnabled =
          prefs.getBool('notifications.reactions') ?? false;
      _linkPreviewsEnabled = prefs.getBool('messages.linkPreviews') ?? true;
      _mutedConversationIds
        ..clear()
        ..addAll(
          prefs.getStringList('notifications.mutedConversations') ?? const [],
        );
      _favoriteConversationIds
        ..clear()
        ..addAll(
          prefs.getStringList('navigation.favoriteConversations') ?? const [],
        );
      _syncedConversationIds
        ..clear()
        ..addAll(
          prefs.getStringList('messages.syncedConversations') ?? const [],
        );
    });
    if (BackgroundMessageWatcher.supported) {
      final watcherEnabled =
          enabled && (prefs.getBool('notifications.backgroundWatcher') ?? true);
      unawaited(
        BackgroundMessageWatcher.setEnabled(watcherEnabled).catchError(
          (Object error) => AppLog.record(
            watcherEnabled
                ? 'Start background message watcher'
                : 'Stop background message watcher',
            error,
          ),
        ),
      );
    }
  }

  Future<void> _signOut() async {
    try {
      await BackgroundMessageWatcher.setEnabled(false);
    } catch (error) {
      AppLog.record('Stop background message watcher', error);
    }
    await _clearAccountData();
    await widget.onSignOut();
  }

  Future<void> _clearAccountData() async {
    if (widget.gateway is ResponsiveTeamsGateway) {
      try {
        await _database.clear();
      } catch (error) {
        AppLog.record('Clear message cache', error);
      }
    }
    try {
      final prefs = await SharedPreferences.getInstance();
      for (final key in _TeamsWorkspaceState._accountPreferenceKeys) {
        await prefs.remove(key);
      }
    } catch (error) {
      AppLog.record('Clear workspace preferences', error);
    }
    AppLog.entries.value = const [];
  }

  Future<void> _openSettings() async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => _SettingsScreen(
          callGateway: widget.callGateway,
          teamsGateway: widget.gateway,
          notifications: widget.notifications,
          enabled: _notificationsEnabled,
          mentionsEnabled: _mentionNotificationsEnabled,
          directMessagesEnabled: _directMessageNotificationsEnabled,
          groupsEnabled: _groupNotificationsEnabled,
          channelsEnabled: _channelNotificationsEnabled,
          reactionsEnabled: _reactionNotificationsEnabled,
          linkPreviewsEnabled: _linkPreviewsEnabled,
          hiddenSectionIds: _hiddenSectionIds,
          onRestoreSection: _restoreSection,
          hiddenConversations: _conversations(hidden: true),
          onRestoreConversation: _restoreConversation,
          onTestCall: _runTestCall,
          messageSync: _messageSync,
          onSyncMessages: _syncMessages,
          onSyncMessageLimitChanged: _setSyncMessageLimit,
          onChanged:
              (
                enabled,
                mentionsEnabled,
                directMessagesEnabled,
                groupsEnabled,
                channelsEnabled,
                reactionsEnabled,
                linkPreviewsEnabled,
              ) async {
                final prefs = await SharedPreferences.getInstance();
                await prefs.setBool('notifications.enabled', enabled);
                await prefs.setBool('notifications.mentions', mentionsEnabled);
                await prefs.setBool(
                  'notifications.directMessages',
                  directMessagesEnabled,
                );
                await prefs.setBool('notifications.groups', groupsEnabled);
                await prefs.setBool('notifications.channels', channelsEnabled);
                await prefs.setBool(
                  'notifications.reactions',
                  reactionsEnabled,
                );
                await prefs.setBool(
                  'messages.linkPreviews',
                  linkPreviewsEnabled,
                );
                if (mounted) {
                  if (!enabled || !reactionsEnabled) {
                    _pendingReactionNotifications.clear();
                    _pendingSavedAccountReactionNotifications.clear();
                  }
                  if (!enabled) _pendingMessageNotifications.clear();
                  _mutate(() {
                    _notificationsEnabled = enabled;
                    _mentionNotificationsEnabled = mentionsEnabled;
                    _directMessageNotificationsEnabled = directMessagesEnabled;
                    _groupNotificationsEnabled = groupsEnabled;
                    _channelNotificationsEnabled = channelsEnabled;
                    _reactionNotificationsEnabled = reactionsEnabled;
                    _linkPreviewsEnabled = linkPreviewsEnabled;
                  });
                }
              },
        ),
      ),
    );
  }

  Future<void> _openCommandPalette() async {
    final conversations = _conversations(hidden: false);
    final destination = await showDialog<Object>(
      context: context,
      builder: (context) => Dialog(
        key: const ValueKey('command-palette'),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 560, maxHeight: 520),
          child: ListView(
            shrinkWrap: true,
            padding: const EdgeInsets.symmetric(vertical: 12),
            children: [
              const ListTile(title: Text('Go to…')),
              for (final conversation in conversations)
                ListTile(
                  leading: Icon(
                    conversation.kind == ConversationKind.channel
                        ? Icons.tag
                        : Icons.chat_bubble_outline,
                  ),
                  title: Text(conversation.name),
                  onTap: () => Navigator.pop(context, conversation),
                ),
              ListTile(
                leading: const Icon(Icons.settings_outlined),
                title: const Text('Settings'),
                onTap: () => Navigator.pop(context, 'settings'),
              ),
            ],
          ),
        ),
      ),
    );
    if (!mounted) return;
    if (destination case final Conversation conversation) {
      await _select(conversation);
    } else if (destination == 'settings') {
      await _openSettings();
    }
  }

  Future<void> _restoreCachedChats() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString('workspace.chats');
    if (!mounted || raw == null) return;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map) {
        throw const FormatException('Cached workspace root is not an object');
      }
      final cached = CachedWorkspace.fromJson(
        decoded.cast<String, dynamic>(),
        includeMessages: false,
      );
      final chats = cached.chats
          .map(
            (chat) => Conversation.chat(
              id: chat.id,
              name: chat.name,
              isGroup: chat.isGroup,
              teamId: chat.teamId,
              profilePhotoUserId: chat.profilePhotoUserId,
              avatarUserIds: chat.memberUserIds,
              lastMessageId: chat.lastMessageId,
              preview: chat.preview,
            ),
          )
          .toList();
      final teams = cached.teams
          .map(
            (team) => TeamSummary(
              id: team.id,
              name: team.name,
              channels: [
                for (final channel in team.channels)
                  Conversation.channel(
                    id: channel.id,
                    name: channel.name,
                    teamId: team.id,
                  ),
              ],
            ),
          )
          .toList();
      final allChatsCached =
          prefs.getBool('workspace.allChatsCached') ??
          (prefs.getStringList('messages.syncedConversations')?.isNotEmpty ??
              false);
      _mutate(() {
        _hiddenConversationIds.addAll(cached.hiddenConversationIds);
        _hiddenSectionIds.addAll(cached.hiddenSectionIds);
        _chats = chats;
        _teams = teams;
        if (allChatsCached) {
          _chatLimit = 500;
          _hasMoreChats = false;
        }
        if (_pendingNotificationChatId == null) {
          _pendingSelectedChatId =
              prefs.getString('workspace.selectedChatId') ??
              cached.selectedChatId;
        }
        _loading = false;
      });
      _restorePendingSelection([
        ...chats,
        for (final team in teams) ...team.channels,
      ]);
      unawaited(_persistWorkspaceCache());
    } catch (error) {
      AppLog.record('Restore cached workspace', error);
    }
  }
}
