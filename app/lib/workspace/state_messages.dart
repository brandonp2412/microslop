part of '../workspace.dart';

extension _WorkspaceMessageActions on _TeamsWorkspaceState {
  void _setSyncMessageLimit(int limit) {
    final current = _messageSync.value;
    if (current.running) return;
    _messageSync.value = current.copyWith(limit: limit);
  }

  Future<void> _syncMessages() async {
    final gateway = widget.gateway is MessageSyncTeamsGateway
        ? widget.gateway as MessageSyncTeamsGateway
        : null;
    final current = _messageSync.value;
    if (current.running || gateway == null) return;
    _messageSync.value = current.copyWith(
      running: true,
      completed: 0,
      total: 0,
      clearError: true,
    );
    final syncedChats = <Conversation>[];
    final requestGeneration = _beginMessageRequest();
    try {
      await gateway.syncRecentMessages(
        current.limit,
        (completed, total) {
          if (!mounted) return;
          final state = _messageSync.value;
          _messageSync.value = state.copyWith(
            completed: completed,
            total: total,
          );
        },
        (conversation, messages) async {
          if (!mounted) return;
          _syncedConversationIds.add(conversation.id);
          if (conversation.kind == ConversationKind.chat) {
            syncedChats.add(conversation);
          }
          if (!_acceptMessageResponse(conversation, requestGeneration)) return;
          final mergedMessages = _withKnownMyReactions(
            conversation,
            _mergePendingOutgoingMessages(conversation, messages),
          );
          _messageCache[conversation] = mergedMessages;
          if (widget.gateway is ResponsiveTeamsGateway) {
            unawaited(
              _database
                  .saveMessages(conversation, mergedMessages)
                  .catchError(
                    (Object error) => AppLog.record(
                      'Persist ${conversation.name} synced messages',
                      error,
                    ),
                  ),
            );
          }
          if (mounted && _selected == conversation) {
            _mutate(() {
              _messages = mergedMessages;
              _loadingMessages = false;
            });
          }
        },
      );
      if (!mounted) return;
      final liveChats = _chats;
      final liveChatIds = liveChats.map((chat) => chat.id).toSet();
      final mergedChats = [
        ...liveChats,
        for (final chat in syncedChats)
          if (!liveChatIds.contains(chat.id)) chat,
      ];
      _mutate(() {
        _chats = List.unmodifiable(mergedChats);
        _chatLimit = 500;
        _hasMoreChats = false;
      });
      final prefs = await SharedPreferences.getInstance();
      await prefs.setStringList(
        'messages.syncedConversations',
        _syncedConversationIds.toList(),
      );
      await prefs.setBool('workspace.allChatsCached', true);
      await _persistWorkspaceCache();
    } catch (error) {
      if (mounted) {
        _messageSync.value = _messageSync.value.copyWith(error: error);
      }
    } finally {
      if (mounted) {
        _messageSync.value = _messageSync.value.copyWith(running: false);
      }
    }
  }

  bool _sameRefreshMessages(
    List<MessageSummary>? previous,
    List<MessageSummary> current,
  ) {
    if (identical(previous, current)) return true;
    if (previous == null || previous.length != current.length) return false;
    for (var index = 0; index < current.length; index++) {
      final left = previous[index];
      final right = current[index];
      if (left.id != right.id ||
          left.sender != right.sender ||
          left.senderId != right.senderId ||
          left.isFromCurrentUser != right.isFromCurrentUser ||
          left.timestamp != right.timestamp ||
          left.content != right.content ||
          left.images.length != right.images.length ||
          !listEquals(left.imageUrls, right.imageUrls) ||
          left.quotes.length != right.quotes.length ||
          left.reactions.length != right.reactions.length) {
        return false;
      }
      for (var imageIndex = 0; imageIndex < left.images.length; imageIndex++) {
        final leftImage = left.images[imageIndex];
        final rightImage = right.images[imageIndex];
        if (leftImage.contentType != rightImage.contentType ||
            leftImage.sourceUrl != rightImage.sourceUrl ||
            !listEquals(leftImage.bytes, rightImage.bytes)) {
          return false;
        }
      }
      for (var quoteIndex = 0; quoteIndex < left.quotes.length; quoteIndex++) {
        final leftQuote = left.quotes[quoteIndex];
        final rightQuote = right.quotes[quoteIndex];
        if (leftQuote.messageId != rightQuote.messageId ||
            leftQuote.sender != rightQuote.sender ||
            leftQuote.content != rightQuote.content) {
          return false;
        }
      }
      for (
        var reactionIndex = 0;
        reactionIndex < left.reactions.length;
        reactionIndex++
      ) {
        final leftReaction = left.reactions[reactionIndex];
        final rightReaction = right.reactions[reactionIndex];
        if (leftReaction.type != rightReaction.type ||
            leftReaction.count != rightReaction.count ||
            leftReaction.selected != rightReaction.selected ||
            leftReaction.users.length != rightReaction.users.length) {
          return false;
        }
        for (
          var userIndex = 0;
          userIndex < leftReaction.users.length;
          userIndex++
        ) {
          final leftUser = leftReaction.users[userIndex];
          final rightUser = rightReaction.users[userIndex];
          if (leftUser.id != rightUser.id || leftUser.name != rightUser.name) {
            return false;
          }
        }
      }
    }
    return true;
  }

  bool _isCurrentUserMessage(MessageSummary message) =>
      message.isFromCurrentUser ||
      _sameUserId(message.senderId, _user?.id) ||
      message.sender == _user?.displayName;

  String? _addedReactionType(MessageSummary previous, MessageSummary current) {
    for (final reaction in current.reactions) {
      var previousCount = 0;
      for (final old in previous.reactions.reversed) {
        if (old.type != reaction.type) continue;
        previousCount = old.count;
        break;
      }
      if (reaction.count > previousCount) return reaction.type;
    }
    return null;
  }

  String _reactionNotificationToken(MessageSummary message) =>
      '${message.id}:${message.reactions.map((reaction) => '${reaction.type}:${reaction.count}').join(',')}';

  (MessageSummary, String)? _addedReactionChange(
    List<MessageSummary>? previous,
    List<MessageSummary> current,
  ) {
    if (previous == null) return null;
    final previousById = {
      for (final message in previous)
        if (message.id.isNotEmpty) message.id: message,
    };
    for (final message in current.reversed) {
      if (message.id.isEmpty) continue;
      final previousMessage = previousById[message.id];
      if (previousMessage == null) continue;
      final reactionType = _addedReactionType(previousMessage, message);
      if (reactionType != null) return (message, reactionType);
    }
    return null;
  }

  bool _mentionsName(String content, String? displayName) {
    final name = displayName?.trim();
    if (name == null || name.isEmpty) return false;
    return _mentionPatterns
        .putIfAbsent(
          name,
          () => RegExp('@\\s*${RegExp.escape(name)}', caseSensitive: false),
        )
        .hasMatch(content);
  }

  bool _mentionsCurrentUser(String content) =>
      _mentionsName(content, _user?.displayName);

  bool _messageCategoryEnabled(Conversation conversation, bool mentioned) {
    if (mentioned && _mentionNotificationsEnabled) return true;
    return switch (conversation.kind) {
      ConversationKind.channel => _channelNotificationsEnabled,
      ConversationKind.chat =>
        conversation.isGroup
            ? _groupNotificationsEnabled
            : _directMessageNotificationsEnabled,
    };
  }

  String _notificationBody(Conversation conversation, MessageSummary message) {
    final content = message.content.trim();
    final summary = content.isNotEmpty
        ? content
        : message.images.isNotEmpty
        ? 'Sent an image'
        : 'New message';
    if (conversation.kind == ConversationKind.channel || conversation.isGroup) {
      final sender = message.sender.trim();
      if (sender.isNotEmpty && sender != '?') return '$sender: $summary';
    }
    return summary;
  }

  Future<void> _showMessageNotification(
    Conversation conversation,
    MessageSummary message, {
    String? reactionType,
  }) async {
    final body = reactionType == null
        ? _notificationBody(conversation, message)
        : '${reactionEmoji(reactionType)} Reacted to your message';
    final notifications = widget.notifications;
    final title = reactionType == null
        ? conversation.name
        : '${reactionEmoji(reactionType)} ${conversation.name}';
    if (notifications is! ConversationDesktopNotifications) {
      await notifications.show(
        title: title,
        body: body,
        conversationId: conversation.id,
      );
      return;
    }
    Uint8List? avatar;
    final senderId = reactionType == null
        ? message.senderId?.trim()
        : conversation.profilePhotoUserId;
    if (senderId?.isNotEmpty == true) {
      try {
        avatar = await widget.gateway
            .getProfilePhoto(senderId!)
            .timeout(const Duration(milliseconds: 800), onTimeout: () => null);
      } catch (error) {
        AppLog.debug('Notification avatar', 'userId=$senderId, error=$error');
      }
    }
    if (!mounted ||
        !_notificationsEnabled ||
        _mutedConversationIds.contains(conversation.id) ||
        (reactionType == null
            ? !_messageCategoryEnabled(
                conversation,
                _mentionsCurrentUser(message.content),
              )
            : !_reactionNotificationsEnabled) ||
        (_appLifecycleState == AppLifecycleState.resumed &&
            _selected?.id == conversation.id)) {
      return;
    }
    await (notifications as ConversationDesktopNotifications).showConversation(
      title: title,
      body: body,
      conversationId: conversation.id,
      senderName: reactionType == null ? message.sender : conversation.name,
      senderAvatar: avatar,
      groupConversation:
          conversation.kind == ConversationKind.channel || conversation.isGroup,
    );
  }

  Future<void> _handleSavedAccountActivity(MessageEvent event) async {
    final accountId = event.accountId?.trim();
    final conversationId = event.conversationId?.trim();
    final gateway = widget.gateway;
    if (!_notificationsEnabled ||
        accountId == null ||
        accountId.isEmpty ||
        conversationId == null ||
        conversationId.isEmpty ||
        gateway is! MultiAccountTeamsGateway) {
      return;
    }
    final notificationKey = '$accountId:$conversationId';
    final requestGeneration = ++_savedAccountActivityRequestGeneration;
    try {
      final notification = await (gateway as MultiAccountTeamsGateway)
          .savedAccountNotification(
            accountId,
            conversationId,
            recentMessageLimit:
                event.resourceType == 'MessageUpdate' &&
                    _reactionNotificationsEnabled
                ? 25
                : 1,
          );
      if (notification == null || !mounted || !_notificationsEnabled) return;
      final appliedGeneration =
          _savedAccountActivityAppliedGenerations[notificationKey] ?? 0;
      if (requestGeneration < appliedGeneration) return;
      _savedAccountActivityAppliedGenerations[notificationKey] =
          requestGeneration;
      final message = notification.message;
      final messages = notification.messages.isEmpty
          ? <MessageSummary>[message]
          : notification.messages;
      final previousMessages =
          _savedAccountNotificationMessages[notificationKey];
      _savedAccountNotificationMessages[notificationKey] = messages;
      if (event.resourceType == 'MessageUpdate') {
        final pendingReaction =
            _pendingSavedAccountReactionNotifications[notificationKey];
        if (pendingReaction != null) {
          final pendingMessage = messages
              .where(
                (message) =>
                    _reactionNotificationToken(message) == pendingReaction.$1,
              )
              .firstOrNull;
          if (_reactionNotificationsEnabled &&
              pendingMessage?.isFromCurrentUser == true) {
            if (_notifiedMessageIds[notificationKey] == pendingReaction.$1) {
              _pendingSavedAccountReactionNotifications.remove(notificationKey);
              return;
            }
            await widget.notifications.show(
              title:
                  '${reactionEmoji(pendingReaction.$2)} ${notification.accountName} · ${notification.conversationName}',
              body: '',
            );
            _pendingSavedAccountReactionNotifications.remove(notificationKey);
            _notifiedMessageIds[notificationKey] = pendingReaction.$1;
            return;
          }
          _pendingSavedAccountReactionNotifications.remove(notificationKey);
        }
        final reactionChange = _addedReactionChange(previousMessages, messages);
        if (reactionChange == null) return;
        final (reactionMessage, reactionType) = reactionChange;
        if (!_reactionNotificationsEnabled ||
            !reactionMessage.isFromCurrentUser) {
          return;
        }
        final token = _reactionNotificationToken(reactionMessage);
        if (_notifiedMessageIds[notificationKey] == token) return;
        try {
          await widget.notifications.show(
            title:
                '${reactionEmoji(reactionType)} ${notification.accountName} · ${notification.conversationName}',
            body: '',
          );
          _notifiedMessageIds[notificationKey] = token;
        } catch (error) {
          _pendingSavedAccountReactionNotifications[notificationKey] = (
            token,
            reactionType,
          );
          rethrow;
        }
        return;
      }
      if (event.resourceType.isNotEmpty && event.resourceType != 'NewMessage') {
        return;
      }
      if (message.isFromCurrentUser) return;
      final mentioned = _mentionsName(
        message.content,
        notification.accountName,
      );
      final enabled =
          mentioned && _mentionNotificationsEnabled ||
          notification.isChannel && _channelNotificationsEnabled ||
          !notification.isChannel &&
              notification.isGroup &&
              _groupNotificationsEnabled ||
          !notification.isChannel &&
              !notification.isGroup &&
              _directMessageNotificationsEnabled;
      if (!enabled) return;
      final token = message.id.isEmpty ? message.timestamp : message.id;
      if (_notifiedMessageIds[notificationKey] == token) return;
      final summary = message.content.trim().isEmpty
          ? 'New message'
          : message.content.trim();
      final body = notification.isChannel || notification.isGroup
          ? '${message.sender}: $summary'
          : summary;
      await widget.notifications.show(
        title: '${notification.accountName} · ${notification.conversationName}',
        body: body,
      );
      _notifiedMessageIds[notificationKey] = token;
    } catch (error) {
      AppLog.record('Resolve saved-account notification', error);
    }
  }

  List<Conversation> _takePendingActivityConversations(
    Iterable<Conversation> conversations,
  ) => [
    for (final conversation in conversations)
      if (_pendingActivityConversationIds.remove(conversation.id)) conversation,
  ];

  void _dispatchConversationActivity(Iterable<Conversation> conversations) {
    for (final conversation in conversations) {
      if (_appLifecycleState == AppLifecycleState.resumed &&
          _selected?.id == conversation.id) {
        unawaited(_refreshSelectedMessages(_selected!));
      } else {
        unawaited(_handleConversationActivity(conversation));
      }
    }
  }

  Future<void> _handleConversationActivity(Conversation conversation) async {
    final requestGeneration = _beginMessageRequest();
    var previous = _messageCache[conversation];
    if (previous == null && widget.gateway is ResponsiveTeamsGateway) {
      try {
        final stored = await _database.loadMessages(conversation);
        if (stored.isNotEmpty) previous = stored;
      } catch (error) {
        AppLog.debug('Load activity cache for ${conversation.name}', error);
      }
    }
    try {
      final gateway = widget.gateway;
      final fresh = gateway is ResponsiveTeamsGateway
          ? await (gateway as ResponsiveTeamsGateway).refreshMessages(
              conversation,
            )
          : await gateway.readMessages(conversation);
      if (!_acceptMessageResponse(conversation, requestGeneration)) return;
      final messages = _withKnownMyReactions(
        conversation,
        _mergePendingOutgoingMessages(conversation, fresh),
      );
      final changed = !_sameRefreshMessages(previous, messages);
      if (changed) _messageCache[conversation] = messages;
      if (gateway is ResponsiveTeamsGateway && changed) {
        unawaited(
          _database
              .saveMessages(conversation, messages, replaceImages: false)
              .catchError(
                (Object error) => AppLog.record(
                  'Persist ${conversation.name} activity',
                  error,
                ),
              ),
        );
      }
      final openConversation = _selected;
      if (mounted &&
          _appLifecycleState == AppLifecycleState.resumed &&
          openConversation?.id == conversation.id) {
        if (changed ||
            _loadingMessages ||
            _messageError != null ||
            _unreadConversationIds.contains(conversation.id)) {
          _mutate(() {
            if (changed) _messages = messages;
            _loadingMessages = false;
            _messageError = null;
            _unreadConversationIds.remove(conversation.id);
          });
        }
        return;
      }
      if (messages.isEmpty) {
        if (mounted) {
          _mutate(() => _unreadConversationIds.remove(conversation.id));
        }
        return;
      }
      final latest = messages.last;
      final previousLatest = previous?.lastOrNull;
      final sameMessage =
          previousLatest != null &&
          previousLatest.id.isNotEmpty &&
          previousLatest.id == latest.id;
      final newMessage =
          previousLatest == null ||
          !sameMessage ||
          previousLatest.timestamp != latest.timestamp ||
          previousLatest.content != latest.content;
      if (newMessage && !_isCurrentUserMessage(latest) && mounted) {
        _mutate(() => _unreadConversationIds.add(conversation.id));
      }
      if (!_notificationsEnabled ||
          _mutedConversationIds.contains(conversation.id)) {
        _pendingMessageNotifications.remove(conversation.id);
        _pendingReactionNotifications.remove(conversation.id);
        return;
      }
      final latestToken = latest.id.isEmpty ? latest.timestamp : latest.id;
      final pendingNotification = _pendingMessageNotifications[conversation.id];
      if (pendingNotification != null) {
        if (pendingNotification.$1 == latestToken &&
            !_isCurrentUserMessage(latest) &&
            _messageCategoryEnabled(
              conversation,
              _mentionsCurrentUser(latest.content),
            )) {
          try {
            await _showMessageNotification(
              conversation,
              pendingNotification.$2,
            );
            _pendingMessageNotifications.remove(conversation.id);
            _notifiedMessageIds[conversation.id] = latestToken;
          } catch (error) {
            AppLog.record('Retry ${conversation.name} notification', error);
          }
          return;
        }
        _pendingMessageNotifications.remove(conversation.id);
      }
      final pendingReaction = _pendingReactionNotifications[conversation.id];
      if (pendingReaction != null) {
        final pendingMessage = messages
            .where(
              (message) =>
                  _reactionNotificationToken(message) == pendingReaction.$1,
            )
            .firstOrNull;
        if (_reactionNotificationsEnabled &&
            pendingMessage != null &&
            _isCurrentUserMessage(pendingMessage)) {
          if (_notifiedMessageIds[conversation.id] == pendingReaction.$1) {
            _pendingReactionNotifications.remove(conversation.id);
            return;
          }
          await _showMessageNotification(
            conversation,
            pendingMessage,
            reactionType: pendingReaction.$2,
          );
          _pendingReactionNotifications.remove(conversation.id);
          _notifiedMessageIds[conversation.id] = pendingReaction.$1;
          return;
        }
        _pendingReactionNotifications.remove(conversation.id);
      }
      final reactionChange = !newMessage
          ? _addedReactionChange(previous, messages)
          : null;
      if (reactionChange != null) {
        final (reactionMessage, reactionType) = reactionChange;
        final suppressedUntil =
            _ownReactionSuppressions[(conversation, reactionMessage.id)];
        if (suppressedUntil != null &&
            suppressedUntil.isAfter(DateTime.now())) {
          return;
        }
        if (!_reactionNotificationsEnabled ||
            !_isCurrentUserMessage(reactionMessage)) {
          return;
        }
        final token = _reactionNotificationToken(reactionMessage);
        if (_notifiedMessageIds[conversation.id] == token) return;
        try {
          await _showMessageNotification(
            conversation,
            reactionMessage,
            reactionType: reactionType,
          );
          _notifiedMessageIds[conversation.id] = token;
        } catch (error) {
          _pendingReactionNotifications[conversation.id] = (
            token,
            reactionType,
          );
          rethrow;
        }
        return;
      }
      if (!newMessage || _isCurrentUserMessage(latest)) return;
      final mentioned = _mentionsCurrentUser(latest.content);
      if (!_messageCategoryEnabled(conversation, mentioned)) return;
      final token = latestToken;
      if (_notifiedMessageIds[conversation.id] == token) return;
      try {
        await _showMessageNotification(conversation, latest);
        _notifiedMessageIds[conversation.id] = token;
      } catch (error) {
        _pendingMessageNotifications[conversation.id] = (token, latest);
        rethrow;
      }
    } catch (error) {
      AppLog.record('Resolve ${conversation.name} notification', error);
    }
  }

  void _onMessageEvent(MessageEvent event) {
    if (!event.accountActive) {
      unawaited(_handleSavedAccountActivity(event));
      return;
    }
    final conversation = _selected;
    final eventConversationId = event.conversationId?.trim();
    final reconcileAll =
        eventConversationId == null || eventConversationId.isEmpty;
    Conversation? changedConversation;
    if (!reconcileAll) {
      for (final chat in _chats) {
        if (chat.id != eventConversationId) continue;
        changedConversation = chat;
        break;
      }
      if (changedConversation == null) {
        for (final team in _teams) {
          for (final channel in team.channels) {
            if (channel.id != eventConversationId) continue;
            changedConversation = channel;
            break;
          }
          if (changedConversation != null) break;
        }
      }
    }
    final isOpen =
        _appLifecycleState == AppLifecycleState.resumed &&
        conversation != null &&
        (reconcileAll || eventConversationId == conversation.id);

    AppLog.debug(
      'Teams message event',
      'accountId=${event.accountId ?? '<active>'}, '
          'conversationId=${reconcileAll ? '<reconcile>' : eventConversationId}, '
          'conversation=${changedConversation?.name ?? '<unknown>'}, '
          'resourceType=${event.resourceType}, open=$isOpen',
    );

    final unknownConversation = !reconcileAll && changedConversation == null;
    if (changedConversation != null && !isOpen) {
      unawaited(_handleConversationActivity(changedConversation));
    } else if (unknownConversation) {
      _pendingActivityConversationIds.add(eventConversationId);
    }
    if (_initialWorkspaceLoadComplete) {
      unawaited(_refreshChats(notifyPreviewChanges: reconcileAll));
      if (unknownConversation) unawaited(_loadTeams());
    }
    if (isOpen) {
      unawaited(_refreshSelectedMessages(conversation));
    }
  }

  Future<void> _refreshChats({bool notifyPreviewChanges = false}) async {
    if (_refreshingChats) {
      _refreshChatsAgain = true;
      _refreshChatsAgainNotifyPreviewChanges |= notifyPreviewChanges;
      return;
    }
    _refreshingChats = true;
    var currentNotifyPreviewChanges = notifyPreviewChanges;
    try {
      do {
        _refreshChatsAgain = false;
        _refreshChatsAgainNotifyPreviewChanges = false;
        try {
          final requestGeneration = ++_chatRequestGeneration;
          final requestedLimit = _chatRequestLimit();
          final chats = await widget.gateway.listChats(limit: requestedLimit);
          if (!mounted) return;
          if (requestGeneration >= _chatAppliedGeneration) {
            _chatAppliedGeneration = requestGeneration;
            final previousPresenceUsers = _directPresenceUserKeys(_chats);
            var changedChats = const <Conversation>[];
            if (currentNotifyPreviewChanges) {
              final previousChats = {for (final chat in _chats) chat.id: chat};
              changedChats = chats.where((chat) {
                final previous = previousChats[chat.id];
                if (previous == null) {
                  return _appLifecycleState != AppLifecycleState.resumed ||
                      _selected?.id != chat.id;
                }
                final previousMessageId = previous.lastMessageId?.trim();
                final nextMessageId = chat.lastMessageId?.trim();
                final messageChanged =
                    previousMessageId != nextMessageId &&
                    (previousMessageId?.isNotEmpty == true ||
                        nextMessageId?.isNotEmpty == true);
                final changed =
                    messageChanged || previous.preview != chat.preview;
                return changed &&
                    (_appLifecycleState != AppLifecycleState.resumed ||
                        _selected?.id != chat.id);
              }).toList();
            }
            final activityChats = {
              for (final chat in changedChats) chat.id: chat,
              for (final chat in _takePendingActivityConversations(chats))
                chat.id: chat,
            };
            _mutate(() {
              _chats = chats;
              _chatLimit = requestedLimit;
              if ((_pendingChatLimit ?? 0) <= requestedLimit) {
                _pendingChatLimit = null;
              }
              _hasMoreChats =
                  requestedLimit < 500 && chats.length >= requestedLimit;
              _loading = false;
            });
            if (!setEquals(
              previousPresenceUsers,
              _directPresenceUserKeys(chats),
            )) {
              unawaited(_refreshPresence());
            }
            unawaited(_persistWorkspaceCache());
            _dispatchConversationActivity(activityChats.values);
          }
        } catch (error) {
          AppLog.record('Refresh chats after activity', error);
        }
        currentNotifyPreviewChanges = _refreshChatsAgainNotifyPreviewChanges;
      } while (_refreshChatsAgain);
    } finally {
      _refreshingChats = false;
    }
  }

  Future<void> _loadMoreChats() async {
    if (!mounted || _loadingMoreChats || !_hasMoreChats) return;
    if (_chatLimit >= 500) {
      _mutate(() => _hasMoreChats = false);
      return;
    }
    final nextLimit = (_chatLimit + 50).clamp(1, 500);
    final requestGeneration = ++_chatRequestGeneration;
    final previousPresenceUsers = _directPresenceUserKeys(_chats);
    _mutate(() {
      _loadingMoreChats = true;
      _pendingChatLimit = nextLimit;
    });
    try {
      final chats = await widget.gateway.listChats(limit: nextLimit);
      if (!mounted || requestGeneration < _chatAppliedGeneration) return;
      _chatAppliedGeneration = requestGeneration;
      _mutate(() {
        _chats = chats;
        _loading = false;
        _chatLimit = nextLimit;
        if ((_pendingChatLimit ?? 0) <= nextLimit) {
          _pendingChatLimit = null;
        }
        _hasMoreChats = nextLimit < 500 && chats.length >= nextLimit;
      });
      if (!setEquals(previousPresenceUsers, _directPresenceUserKeys(chats))) {
        unawaited(_refreshPresence());
      }
      unawaited(_persistWorkspaceCache());
    } catch (error) {
      if (requestGeneration == _chatRequestGeneration) {
        AppLog.record('Load more chats', error);
      }
    } finally {
      if (mounted) _mutate(() => _loadingMoreChats = false);
    }
  }

  Future<void> _refreshSelectedMessages(Conversation conversation) async {
    if (!mounted || _selected != conversation) return;
    if (!_refreshingMessageConversations.add(conversation)) {
      _pendingMessageRefreshes.add(conversation);
      return;
    }
    final requestGeneration = _beginMessageRequest();
    try {
      final gateway = widget.gateway;
      final responsiveGateway = gateway is ResponsiveTeamsGateway
          ? gateway as ResponsiveTeamsGateway
          : null;
      final freshMessages = responsiveGateway != null
          ? await responsiveGateway.refreshMessages(conversation)
          : await gateway.readMessages(conversation);
      if (!_acceptMessageResponse(conversation, requestGeneration)) return;
      final messages = _withKnownMyReactions(
        conversation,
        _mergePendingOutgoingMessages(conversation, freshMessages),
      );
      final previous = _messageCache[conversation];
      final changed = !_sameRefreshMessages(previous, messages);
      if (changed) _messageCache[conversation] = messages;
      if (!mounted || _selected != conversation) return;
      if (changed || _loadingMessages || _messageError != null) {
        _mutate(() {
          if (changed) _messages = messages;
          _loadingMessages = false;
          _messageError = null;
        });
      }
      if (responsiveGateway != null && changed) {
        unawaited(
          _database
              .saveMessages(conversation, messages, replaceImages: false)
              .catchError(
                (Object error) => AppLog.record(
                  'Persist ${conversation.name} messages',
                  error,
                ),
              ),
        );
        unawaited(_hydrateRecentImages(conversation));
      }
    } catch (error) {
      AppLog.record('Watch ${conversation.name} messages', error);
      if (mounted && _selected == conversation) {
        _mutate(() {
          _loadingMessages = false;
          _messageError = error;
        });
      }
    } finally {
      _refreshingMessageConversations.remove(conversation);
      final pending = _pendingMessageRefreshes.remove(conversation);
      if (mounted && pending && _selected == conversation) {
        unawaited(_refreshSelectedMessages(conversation));
      }
    }
  }

  Future<List<MessageImage>> _loadCachedImages(MessageSummary message) async {
    final conversation = _selected;
    if (conversation == null || widget.gateway is! ResponsiveTeamsGateway) {
      return const [];
    }
    try {
      return await _database.loadImages(conversation, message.id);
    } catch (error) {
      AppLog.debug('Load ${conversation.name} message images', error);
      return const [];
    }
  }

  Future<void> _hydrateRecentImages(Conversation conversation) async {
    final gateway = widget.gateway;
    if (gateway is! ResponsiveTeamsGateway) return;
    if (!_hydratingImages.add(conversation)) {
      _pendingImageHydrations.add(conversation);
      return;
    }
    try {
      await Future<void>.delayed(const Duration(milliseconds: 300));
      if (!mounted || _selected != conversation) return;
      final hydrated = await (gateway as ResponsiveTeamsGateway)
          .hydrateRecentImages(conversation);
      unawaited(
        _database
            .saveImages(conversation, hydrated)
            .catchError(
              (Object error) => AppLog.record(
                'Persist ${conversation.name} message images',
                error,
              ),
            ),
      );
      if (!mounted || _selected != conversation) return;
    } catch (error) {
      AppLog.debug('Hydrate ${conversation.name} message images', error);
    } finally {
      _hydratingImages.remove(conversation);
      final pending = _pendingImageHydrations.remove(conversation);
      if (mounted && _selected == conversation && pending) {
        unawaited(_hydrateRecentImages(conversation));
      }
    }
  }

  bool _sendingConversation(Conversation conversation) =>
      _sendingConversations.contains(conversation);

  Future<void> _send() async {
    final conversation = _selected;
    final content = _composer.text.trim();
    if (!mounted ||
        conversation == null ||
        content.isEmpty ||
        _sendingConversation(conversation)) {
      return;
    }

    final optimisticMessage = MessageSummary(
      id: 'local:${DateTime.now().microsecondsSinceEpoch}',
      sender: _user?.displayName ?? 'You',
      senderId: _user?.id,
      isFromCurrentUser: true,
      timestamp: DateTime.now().toUtc().toIso8601String(),
      content: content,
    );
    final messages = [
      ...(_messageCache[conversation] ?? _messages),
      optimisticMessage,
    ];
    _messageCache[conversation] = messages;
    _composer.clear();
    _drafts.remove(conversation);
    _mutate(() {
      _sendingConversations.add(conversation);
      _messages = messages;
      if (conversation.kind == ConversationKind.chat) {
        final index = _chats.indexWhere((chat) => chat.id == conversation.id);
        if (index > 0) {
          final chats = [..._chats];
          chats.insert(0, chats.removeAt(index));
          _chats = List.unmodifiable(chats);
        }
      }
    });

    try {
      await widget.gateway.sendMessage(conversation, content);
      if (mounted) _mutate(() => _sendingConversations.remove(conversation));
      await _refreshSelectedMessages(conversation);
    } catch (error) {
      AppLog.record('Send message to ${conversation.name}', error);
      _removeOptimisticMessage(conversation, optimisticMessage.id);
      if (mounted) {
        final selected = _selected == conversation;
        final existing = selected
            ? _composer.text
            : _drafts[conversation] ?? '';
        final restored = existing.isEmpty || existing == content
            ? content
            : '$content\n$existing';
        if (selected) {
          _composer.text = restored;
          _composer.selection = TextSelection.collapsed(
            offset: restored.length,
          );
        } else {
          _drafts[conversation] = restored;
        }
        _showConversationError(conversation, error);
      }
    } finally {
      if (mounted && _sendingConversation(conversation)) {
        _mutate(() => _sendingConversations.remove(conversation));
      }
    }
  }

  List<MessageSummary> _mergePendingOutgoingMessages(
    Conversation conversation,
    List<MessageSummary> freshMessages,
  ) {
    final pendingMessages = (_messageCache[conversation] ?? const [])
        .where((message) => message.id.startsWith('local:'))
        .toList();
    if (pendingMessages.isEmpty) return freshMessages;

    final merged = [...freshMessages];
    for (final pending in pendingMessages) {
      final acknowledged = freshMessages.any(
        (message) => _matchesOptimisticOutgoing(conversation, message, pending),
      );
      if (!acknowledged) merged.add(pending);
    }
    return merged;
  }

  bool _matchesOptimisticOutgoing(
    Conversation conversation,
    MessageSummary message,
    MessageSummary pending,
  ) {
    if (message.id.startsWith('local:') ||
        message.content != pending.content ||
        message.images.isEmpty != pending.images.isEmpty) {
      return false;
    }
    final isCurrentUser =
        message.isFromCurrentUser ||
        _sameUserId(message.senderId, _user?.id) ||
        message.sender == _user?.displayName ||
        message.sender == pending.sender;
    final directChat =
        conversation.kind == ConversationKind.chat && !conversation.isGroup;
    if (!isCurrentUser && !directChat) return false;

    final sentAt = DateTime.tryParse(pending.timestamp);
    final receivedAt = DateTime.tryParse(message.timestamp);
    if (sentAt == null || receivedAt == null) return true;
    return sentAt.difference(receivedAt).abs() <= const Duration(minutes: 5);
  }

  void _removeOptimisticMessage(Conversation conversation, String id) {
    final messages = (_messageCache[conversation] ?? const [])
        .where((message) => message.id != id)
        .toList();
    _messageCache[conversation] = messages;
    if (mounted && _selected == conversation) {
      _mutate(() => _messages = messages);
    }
  }

  Future<void> _sendImage(
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {
    final conversation = _selected;
    if (!mounted ||
        conversation == null ||
        bytes.isEmpty ||
        _sendingConversation(conversation)) {
      return;
    }

    final optimisticMessage = MessageSummary(
      id: 'local:${DateTime.now().microsecondsSinceEpoch}',
      sender: _user?.displayName ?? 'You',
      senderId: _user?.id,
      isFromCurrentUser: true,
      timestamp: DateTime.now().toUtc().toIso8601String(),
      content: caption,
      images: [MessageImage(contentType: contentType, bytes: bytes)],
    );
    final messages = [
      ...(_messageCache[conversation] ?? _messages),
      optimisticMessage,
    ];
    _messageCache[conversation] = messages;
    if (caption.isNotEmpty) {
      _composer.clear();
      _drafts.remove(conversation);
    }
    _mutate(() {
      _sendingConversations.add(conversation);
      _messages = messages;
    });

    try {
      await widget.gateway.sendImageMessage(
        conversation,
        bytes,
        contentType,
        caption: caption,
      );
      if (mounted) _mutate(() => _sendingConversations.remove(conversation));
      await _refreshSelectedMessages(conversation);
    } catch (error) {
      AppLog.record('Send image to ${conversation.name}', error);
      _removeOptimisticMessage(conversation, optimisticMessage.id);
      if (mounted &&
          _selected == conversation &&
          _composer.text.isEmpty &&
          caption.isNotEmpty) {
        _composer.text = caption;
        _composer.selection = TextSelection.collapsed(offset: caption.length);
      }
      _showConversationError(conversation, error);
    } finally {
      if (mounted && _sendingConversation(conversation)) {
        _mutate(() => _sendingConversations.remove(conversation));
      }
    }
  }

  bool _stillSelected(Conversation conversation) =>
      mounted && _selected == conversation;

  Future<void> _pickImage() async {
    final conversation = _selected;
    if (!mounted || conversation == null) return;
    try {
      final result = await FilePicker.pickFiles(
        type: FileType.image,
        withData: true,
      );
      if (result == null ||
          result.files.isEmpty ||
          !_stillSelected(conversation)) {
        return;
      }
      final file = result.files.single;
      final bytes = file.bytes ?? await file.xFile.readAsBytes();
      if (!_stillSelected(conversation)) return;
      await _sendImage(bytes, _imageContentType(file.extension));
    } catch (error) {
      AppLog.record('Pick image', error);
      _showConversationError(conversation, error);
    }
  }

  Future<void> _sendFile(
    Uint8List bytes,
    String fileName,
    String contentType,
  ) async {
    final conversation = _selected;
    final gateway = widget.gateway is FileTeamsGateway
        ? widget.gateway as FileTeamsGateway
        : null;
    if (!mounted ||
        conversation == null ||
        bytes.isEmpty ||
        _sendingConversation(conversation) ||
        gateway == null) {
      return;
    }
    _mutate(() => _sendingConversations.add(conversation));
    try {
      await gateway.sendFileMessage(conversation, bytes, fileName, contentType);
      await _refreshSelectedMessages(conversation);
    } catch (error) {
      AppLog.record('Send file to ${conversation.name}', error);
      _showConversationError(conversation, error);
    } finally {
      if (mounted && _sendingConversation(conversation)) {
        _mutate(() => _sendingConversations.remove(conversation));
      }
    }
  }

  Future<void> _pickFile() async {
    final conversation = _selected;
    if (!mounted || conversation == null) return;
    try {
      final result = await FilePicker.pickFiles(withData: true);
      if (result == null ||
          result.files.isEmpty ||
          !_stillSelected(conversation)) {
        return;
      }
      final file = result.files.single;
      final bytes = file.bytes ?? await file.xFile.readAsBytes();
      if (!_stillSelected(conversation)) return;
      if (bytes.length > 25 * 1024 * 1024) {
        throw StateError('Files larger than 25 MB are not supported yet.');
      }
      await _sendFile(bytes, file.name, _fileContentType(file.extension));
    } catch (error) {
      AppLog.record('Pick file', error);
      _showConversationError(conversation, error);
    }
  }

  Future<void> _openMessageCamera() async {
    final conversation = _selected;
    if (!mounted ||
        conversation == null ||
        !PlatformMessageMedia.supported ||
        !await _ensureCameraPermission()) {
      return;
    }
    if (!mounted || _selected != conversation) return;
    final bytes = await Navigator.of(context).push<Uint8List>(
      MaterialPageRoute(builder: (_) => const _MessageCameraScreen()),
    );
    if (bytes != null && bytes.isNotEmpty && _stillSelected(conversation)) {
      await _sendImage(bytes, 'image/jpeg');
    }
  }

  Future<void> _recordAudioMessage() async {
    final conversation = _selected;
    if (!mounted ||
        conversation == null ||
        !PlatformMessageMedia.supported ||
        !await _ensureMicrophonePermission() ||
        !_stillSelected(conversation)) {
      return;
    }
    try {
      await PlatformMessageMedia.startAudioRecording();
      if (!mounted || _selected != conversation) {
        await PlatformMessageMedia.cancelAudioRecording();
        return;
      }
      final send = await showModalBottomSheet<bool>(
        context: context,
        isDismissible: false,
        enableDrag: false,
        builder: (_) => const _AudioRecordingSheet(),
      );
      if (!_stillSelected(conversation)) {
        await PlatformMessageMedia.cancelAudioRecording();
        return;
      }
      if (send == true) {
        final recording = await PlatformMessageMedia.stopAudioRecording();
        if (!_stillSelected(conversation)) return;
        await _sendFile(recording.data, recording.name, recording.contentType);
      } else {
        await PlatformMessageMedia.cancelAudioRecording();
      }
    } catch (error) {
      unawaited(PlatformMessageMedia.cancelAudioRecording());
      AppLog.record('Record audio message', error);
      _showConversationError(conversation, error);
    }
  }

  Future<void> _pasteImageOrText() async {
    final conversation = _selected;
    if (!mounted || conversation == null) return;
    final content = await MessageClipboard.read();
    if (!_stillSelected(conversation) || content == null) return;
    final bytes = content.imageBytes;
    final contentType = content.imageContentType;
    if (bytes != null && bytes.isNotEmpty && contentType != null) {
      await _sendImage(bytes, contentType);
      return;
    }
    final text = content.text;
    if (text == null || text.isEmpty) return;
    final selection = _composer.selection;
    final start = selection.isValid ? selection.start : _composer.text.length;
    final end = selection.isValid ? selection.end : _composer.text.length;
    final updated = _composer.text.replaceRange(start, end, text);
    _composer.value = TextEditingValue(
      text: updated,
      selection: TextSelection.collapsed(offset: start + text.length),
    );
  }
}
