part of '../workspace.dart';

extension _WorkspaceDataActions on _TeamsWorkspaceState {
  int _chatRequestLimit() {
    final pending = _pendingChatLimit;
    return (pending != null && pending > _chatLimit ? pending : _chatLimit)
        .clamp(1, 500);
  }

  Future<void> _loadWorkspace() async {
    if (!mounted) return;
    if (_loadingWorkspace) {
      _loadWorkspaceAgain = true;
      return;
    }
    _loadingWorkspace = true;
    final blockForInitialLoad = _chats.isEmpty && _teams.isEmpty;
    final requestGeneration = ++_chatRequestGeneration;
    _mutate(() {
      _loading = blockForInitialLoad;
      _error = null;
    });

    unawaited(_loadUser());
    unawaited(_loadTeams());

    try {
      final requestedLimit = _chatRequestLimit();
      final chats = await widget.gateway.listChats(limit: requestedLimit);
      if (!mounted || requestGeneration < _chatAppliedGeneration) return;
      _chatAppliedGeneration = requestGeneration;
      final activityChats = _takePendingActivityConversations(chats);
      _mutate(() {
        _chats = chats;
        _chatLimit = requestedLimit;
        if ((_pendingChatLimit ?? 0) <= requestedLimit) {
          _pendingChatLimit = null;
        }
        _hasMoreChats = requestedLimit < 500 && chats.length >= requestedLimit;
        _loading = false;
      });
      _restorePendingSelection(chats);
      _dispatchConversationActivity(activityChats);
      unawaited(_persistWorkspaceCache());
      unawaited(_presentPendingShare());
      unawaited(_refreshPresence());
    } catch (error) {
      AppLog.record('Load chats', error);
      if (!mounted || requestGeneration != _chatRequestGeneration) return;
      _mutate(() {
        _error = error;
        _loading = false;
      });
    } finally {
      _loadingWorkspace = false;
      if (mounted && _loadWorkspaceAgain) {
        _loadWorkspaceAgain = false;
        unawaited(_loadWorkspace());
      }
    }
  }

  Map<String, String> _directPresenceUsers(Iterable<Conversation> chats) => {
    for (final chat in chats)
      if (!chat.isGroup && chat.teamId == null)
        if (chat.profilePhotoUserId?.trim() case final id? when id.isNotEmpty)
          _userIdKey(id): id,
  };

  Set<String> _directPresenceUserKeys(Iterable<Conversation> chats) => {
    for (final chat in chats)
      if (!chat.isGroup && chat.teamId == null)
        if (chat.profilePhotoUserId?.trim() case final id? when id.isNotEmpty)
          _userIdKey(id),
  };

  Future<void> _refreshPresence() async {
    final gateway = widget.gateway;
    final currentUserId = _user?.id?.trim();
    if (gateway is! PresenceTeamsGateway ||
        currentUserId == null ||
        currentUserId.isEmpty) {
      return;
    }
    final users = _directPresenceUsers(_chats)
      ..remove(_userIdKey(currentUserId));
    if (users.isEmpty) return;
    if (_refreshingPresence) {
      _presenceRefreshPending = true;
      return;
    }
    final presenceGateway = gateway as PresenceTeamsGateway;
    _refreshingPresence = true;
    try {
      final presences = await presenceGateway.getPresences(
        users.values.toList(growable: false),
      );
      if (!mounted) return;
      _mutate(() {
        _presenceByUserId = {
          for (final presence in presences)
            _userIdKey(presence.userId): presence,
        };
      });
    } catch (error) {
      AppLog.debug('Load teammate presence', error);
    } finally {
      _refreshingPresence = false;
      if (mounted && _presenceRefreshPending) {
        _presenceRefreshPending = false;
        unawaited(_refreshPresence());
      }
    }
  }

  Future<void> _loadUser() async {
    final requestGeneration = ++_userRequestGeneration;
    try {
      final user = await widget.gateway.getUser();
      if (mounted && requestGeneration >= _userAppliedGeneration) {
        _userAppliedGeneration = requestGeneration;
        _mutate(() => _user = user);
        unawaited(_refreshPresence());
      }
    } catch (error) {
      if (requestGeneration == _userRequestGeneration) {
        AppLog.record('Load current user', error);
      }
    }
  }

  Future<void> _hideConversation(Conversation conversation) async {
    final favorite = _favoriteConversationIds.contains(conversation.id);
    final muted = _mutedConversationIds.contains(conversation.id);
    final action = await _showActions(context, {
      'favorite': favorite ? 'Remove from favourites' : 'Add to favourites',
      'mute': muted ? 'Unmute notifications' : 'Mute notifications',
      'hide': 'Hide ${conversation.name}',
    });
    if (action == null || !mounted) return;
    _mutate(() {
      switch (action) {
        case 'favorite':
          favorite
              ? _favoriteConversationIds.remove(conversation.id)
              : _favoriteConversationIds.add(conversation.id);
        case 'mute':
          muted
              ? _mutedConversationIds.remove(conversation.id)
              : _mutedConversationIds.add(conversation.id);
          if (!muted) {
            _pendingMessageNotifications.remove(conversation.id);
            _pendingReactionNotifications.remove(conversation.id);
          }
        case 'hide':
          _hiddenConversationIds.add(conversation.id);
          if (_selected == conversation) _selected = null;
      }
    });
    if (action == 'hide') {
      await _persistWorkspaceCache();
      return;
    }
    final prefs = await SharedPreferences.getInstance();
    await prefs.setStringList(
      action == 'favorite'
          ? 'navigation.favoriteConversations'
          : 'notifications.mutedConversations',
      (action == 'favorite' ? _favoriteConversationIds : _mutedConversationIds)
          .toList(),
    );
  }

  Future<void> _hideSection(String id, String title) async {
    final hide = await _showActions(context, {true: 'Hide $title'});
    if (hide != true || !mounted) return;
    _mutate(() => _hiddenSectionIds.add(id));
    await _persistWorkspaceCache();
  }

  void _restoreSection(String id) {
    _mutate(() => _hiddenSectionIds.remove(id));
    unawaited(_persistWorkspaceCache());
  }

  void _restoreConversation(Conversation conversation) {
    _mutate(() => _hiddenConversationIds.remove(conversation.id));
    unawaited(_persistWorkspaceCache());
  }

  Future<void> _loadTeams() async {
    if (_loadingTeams) {
      _loadTeamsAgain = true;
      return;
    }
    _loadingTeams = true;
    try {
      do {
        _loadTeamsAgain = false;
        final requestGeneration = ++_teamRequestGeneration;
        try {
          final teams = await widget.gateway.listTeams();
          if (!mounted || requestGeneration < _teamAppliedGeneration) return;
          _teamAppliedGeneration = requestGeneration;
          final channels = [for (final team in teams) ...team.channels];
          final activityChannels = _takePendingActivityConversations(channels);
          _mutate(() => _teams = teams);
          _restorePendingSelection(channels);
          _dispatchConversationActivity(activityChannels);
          unawaited(_persistWorkspaceCache());
          unawaited(_presentPendingShare());
        } catch (error) {
          if (requestGeneration == _teamRequestGeneration) {
            AppLog.record('Load teams and channels', error);
          }
        }
      } while (_loadTeamsAgain);
    } finally {
      _loadingTeams = false;
    }
  }

  Future<void> _takePendingSharedContent() async {
    final content = await PlatformShareTarget.takePending();
    if (content != null) _receiveSharedContent(content);
  }

  void _receiveSharedContent(SharedContent content) {
    _pendingSharedContent = content;
    unawaited(_presentPendingShare());
  }

  Future<void> _presentPendingShare() async {
    if (!mounted || _presentingShareTarget) return;
    final content = _pendingSharedContent;
    if (content == null) return;
    final conversations = _conversations(hidden: false);
    if (conversations.isEmpty) return;
    _presentingShareTarget = true;
    try {
      final target = await showModalBottomSheet<Conversation>(
        context: context,
        isScrollControlled: true,
        builder: (context) => SafeArea(
          child: FractionallySizedBox(
            heightFactor: .72,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(24, 20, 24, 12),
                  child: Text(
                    'Share to',
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                ),
                Expanded(
                  child: ListView.builder(
                    itemCount: conversations.length,
                    itemBuilder: (context, index) {
                      final conversation = conversations[index];
                      return ListTile(
                        leading: conversation.kind == ConversationKind.channel
                            ? const CircleAvatar(child: Icon(Icons.tag_rounded))
                            : _ConversationAvatar(
                                conversation: conversation,
                                gateway: widget.gateway,
                                selected: false,
                                radius: 20,
                              ),
                        title: Text(conversation.name),
                        subtitle: Text(
                          conversation.kind == ConversationKind.channel
                              ? 'Channel'
                              : conversation.isGroup
                              ? 'Group chat'
                              : 'Direct message',
                        ),
                        onTap: () => Navigator.pop(context, conversation),
                      );
                    },
                  ),
                ),
              ],
            ),
          ),
        ),
      );
      if (!mounted || target == null) {
        if (target == null) _pendingSharedContent = null;
        return;
      }
      if (identical(_pendingSharedContent, content)) {
        _pendingSharedContent = null;
      }
      await _select(target);
      if (!mounted || _selected != target) return;
      if (content.hasImage) {
        await _sendImage(
          content.imageBytes!,
          content.contentType ?? 'image/jpeg',
        );
      }
      if (content.hasText) {
        final text = content.text!.trim();
        final selected = _selected == target;
        final existing = selected ? _composer.text : _drafts[target] ?? '';
        final updated = existing.isEmpty || existing == text
            ? text
            : '$existing\n$text';
        if (selected) {
          _composer.value = TextEditingValue(
            text: updated,
            selection: TextSelection.collapsed(offset: updated.length),
          );
        } else {
          _drafts[target] = updated;
        }
      }
    } finally {
      _presentingShareTarget = false;
      if (_pendingSharedContent != null) unawaited(_presentPendingShare());
    }
  }

  void _dismissConversationNotifications(String conversationId) {
    final notifications = widget.notifications;
    if (notifications is! ConversationDesktopNotifications) return;
    final conversationNotifications =
        notifications as ConversationDesktopNotifications;
    unawaited(
      conversationNotifications
          .dismissConversation(conversationId)
          .catchError(
            (Object error) =>
                AppLog.record('Dismiss conversation notifications', error),
          ),
    );
  }

  void _unfocusTextInput() {
    FocusManager.instance.primaryFocus?.unfocus();
    _workspaceFocus.requestFocus();
  }

  Future<void> _select(Conversation conversation) async {
    if (!mounted) return;
    _unfocusTextInput();
    final previous = _selected;
    if (previous != null) {
      final draft = _composer.text;
      if (draft.isEmpty) {
        _drafts.remove(previous);
      } else {
        _drafts[previous] = draft;
      }
    }
    _pendingMessageNotifications.remove(conversation.id);
    _pendingReactionNotifications.remove(conversation.id);
    final targetDraft = _drafts[conversation] ?? '';
    _composer.value = TextEditingValue(
      text: targetDraft,
      selection: TextSelection.collapsed(offset: targetDraft.length),
    );
    final responsive = widget.gateway is ResponsiveTeamsGateway;
    final cachedMessages = _messageCache[conversation];
    _mutate(() {
      _selected = conversation;
      _unreadConversationIds.remove(conversation.id);
      _messages = cachedMessages ?? const [];
      _loadingMessages = cachedMessages == null;
      _error = null;
      _messageError = null;
    });
    AppLog.info(
      'Conversation selection',
      'Selected kind=${conversation.kind.name}; appBarTitle=visible',
    );
    _dismissConversationNotifications(conversation.id);
    unawaited(() async {
      try {
        final prefs = await SharedPreferences.getInstance();
        await prefs.setString('workspace.selectedChatId', conversation.id);
      } catch (error) {
        AppLog.record('Persist selected conversation', error);
      }
    }());
    if (cachedMessages != null) {
      unawaited(_refreshSelectedMessages(conversation));
      return;
    }
    if (responsive) {
      unawaited(() async {
        await _databaseReady;
        final storedMessages = await _database
            .loadMessages(conversation)
            .catchError((Object error) {
              AppLog.record('Load ${conversation.name} message cache', error);
              return const <MessageSummary>[];
            });
        if (!mounted ||
            _selected != conversation ||
            storedMessages.isEmpty ||
            _messageCache[conversation] != null) {
          return;
        }
        _messageCache[conversation] = storedMessages;
        _mutate(() {
          _messages = storedMessages;
          _loadingMessages = false;
        });
      }());
      unawaited(_refreshSelectedMessages(conversation));
      return;
    }
    final requestGeneration = _beginMessageRequest();
    try {
      final messages = await widget.gateway.readMessages(conversation);
      if (!_acceptMessageResponse(conversation, requestGeneration)) return;
      _messageCache[conversation] = messages;
      if (mounted && _selected == conversation) {
        _mutate(() {
          _messages = messages;
          _messageError = null;
        });
      }
    } catch (error) {
      AppLog.record('Load ${conversation.name} messages', error);
      if (mounted && _selected == conversation) {
        _mutate(() => _messageError = error);
      }
    } finally {
      if (mounted && _selected == conversation) {
        _mutate(() => _loadingMessages = false);
      }
    }
  }

  CachedWorkspace _cachedWorkspaceSnapshot() => CachedWorkspace(
    chats: [
      for (final chat in _chats)
        CachedConversation(
          id: chat.id,
          name: chat.name,
          isGroup: chat.isGroup,
          teamId: chat.teamId,
          profilePhotoUserId: chat.profilePhotoUserId,
          memberUserIds: chat.avatarUserIds,
          lastMessageId: chat.lastMessageId,
          preview: chat.preview,
        ),
    ],
    teams: [
      for (final team in _teams)
        CachedTeam(
          id: team.id,
          name: team.name,
          channels: [
            for (final channel in team.channels)
              CachedConversation(
                id: channel.id,
                name: channel.name,
                isGroup: false,
                teamId: team.id,
              ),
          ],
        ),
    ],
    selectedChatId: _selected?.id,
    hiddenConversationIds: _hiddenConversationIds.toList(),
    hiddenSectionIds: _hiddenSectionIds,
    messages: const {},
  );

  Future<void> _persistWorkspaceCache() async {
    if (_persistingWorkspaceCache) {
      _persistWorkspaceCacheAgain = true;
      return;
    }
    _persistingWorkspaceCache = true;
    try {
      do {
        _persistWorkspaceCacheAgain = false;
        final snapshot = _cachedWorkspaceSnapshot();
        final prefs = await SharedPreferences.getInstance();
        await prefs.setString('workspace.chats', jsonEncode(snapshot.toJson()));
      } while (_persistWorkspaceCacheAgain);
    } catch (error) {
      AppLog.record('Persist workspace cache', error);
    } finally {
      _persistingWorkspaceCache = false;
      _persistWorkspaceCacheAgain = false;
    }
  }

  void _restorePendingSelection(Iterable<Conversation> conversations) {
    final selectedId = _pendingNotificationChatId ?? _pendingSelectedChatId;
    if (selectedId == null ||
        (_selected != null && _pendingNotificationChatId == null) ||
        !mounted) {
      return;
    }
    if (_hiddenConversationIds.contains(selectedId)) {
      _pendingSelectedChatId = null;
      return;
    }
    final selected = conversations
        .where((conversation) => conversation.id == selectedId)
        .firstOrNull;
    if (selected == null) return;
    _mutate(() {
      _pendingNotificationChatId = null;
      _pendingSelectedChatId = null;
    });
    _scaffoldKey.currentState?.closeDrawer();
    unawaited(_select(selected));
  }

  void _onNotificationSelection(String conversationId) {
    final id = conversationId.trim();
    if (id.isEmpty) return;
    _scaffoldKey.currentState?.closeDrawer();
    Conversation? conversation;
    for (final chat in _chats) {
      if (chat.id == id) {
        conversation = chat;
        break;
      }
    }
    if (conversation == null) {
      for (final team in _teams) {
        for (final channel in team.channels) {
          if (channel.id != id) continue;
          conversation = channel;
          break;
        }
        if (conversation != null) break;
      }
    }
    if (conversation != null) {
      _pendingNotificationChatId = null;
      unawaited(_select(conversation));
      return;
    }
    _pendingNotificationChatId = id;
    unawaited(_loadWorkspace());
  }

  Future<void> _hydrateCachedNavigation() async {
    if (widget.gateway is! ResponsiveTeamsGateway) return;
    final conversations = [
      ..._chats,
      for (final team in _teams) ...team.channels,
    ];
    var changed = false;
    for (final conversation in conversations) {
      if (_messageCache.containsKey(conversation)) continue;
      try {
        final messages = await _database.loadMessages(conversation);
        if (messages.isNotEmpty) {
          _messageCache[conversation] = messages;
          changed = true;
        }
      } catch (error) {
        AppLog.record('Load ${conversation.name} message cache', error);
      }
    }
    if (mounted && changed) _mutate(() {});
  }
}
