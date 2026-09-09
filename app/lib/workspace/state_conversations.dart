part of '../workspace.dart';

final _genericConversationNamePattern = RegExp(r'^[0-9a-fA-F-]{32,36}$');

extension _WorkspaceConversationActions on _TeamsWorkspaceState {
  bool _reactingConversation(Conversation conversation) =>
      _reactingConversations.contains(conversation);

  Future<void> _react(MessageSummary message, String reactionType) async {
    final conversation = _selected;
    if (conversation == null ||
        message.id.isEmpty ||
        message.id.startsWith('local:') ||
        _reactingConversation(conversation)) {
      return;
    }

    final original = message;
    final reactionKey = (conversation, message.id, reactionType);
    final suppressionKey = (conversation, message.id);
    final removing = message.reactions.any(
      (reaction) => reaction.type == reactionType && reaction.selected,
    );
    final suppressionUntil = DateTime.now().add(const Duration(seconds: 10));
    if (removing) {
      _myReactionKeys.remove(reactionKey);
      _myRemovedReactionKeys[reactionKey] = suppressionUntil;
    } else {
      _myRemovedReactionKeys.remove(reactionKey);
      _myReactionKeys.add(reactionKey);
    }
    _ownReactionSuppressions[suppressionKey] = suppressionUntil;
    final optimistic = removing
        ? _withoutReaction(message, reactionType)
        : _withAddedReaction(message, reactionType);
    _replaceMessage(conversation, optimistic);
    _mutate(() => _reactingConversations.add(conversation));

    try {
      await widget.gateway.setReaction(conversation, message, reactionType);
      if (mounted) _mutate(() => _reactingConversations.remove(conversation));
      await _refreshSelectedMessages(conversation);
    } catch (error) {
      AppLog.record('React to message', error);
      if (removing) {
        _myRemovedReactionKeys.remove(reactionKey);
      } else {
        _myReactionKeys.remove(reactionKey);
      }
      _ownReactionSuppressions.remove(suppressionKey);
      _replaceMessage(conversation, original);
      _showConversationError(conversation, error);
    } finally {
      if (mounted && _reactingConversation(conversation)) {
        _mutate(() => _reactingConversations.remove(conversation));
      }
    }
  }

  MessageSummary _withAddedReaction(
    MessageSummary message,
    String reactionType,
  ) {
    final reactions = <MessageReaction>[];
    var found = false;
    for (final reaction in message.reactions) {
      if (reaction.type == reactionType) {
        reactions.add(
          MessageReaction(
            type: reaction.type,
            count: reaction.count + 1,
            users: [
              ...reaction.users,
              if (_user case final user?)
                ReactionUser(id: user.id ?? '', name: user.displayName),
            ],
            selected: true,
          ),
        );
        found = true;
      } else {
        reactions.add(reaction);
      }
    }
    if (!found) {
      reactions.add(
        MessageReaction(
          type: reactionType,
          count: 1,
          selected: true,
          users: [
            if (_user case final user?)
              ReactionUser(id: user.id ?? '', name: user.displayName),
          ],
        ),
      );
    }
    return message.withReactions(reactions);
  }

  MessageSummary _withoutReaction(MessageSummary message, String reactionType) {
    final reactions = <MessageReaction>[];
    for (final reaction in message.reactions) {
      if (reaction.type != reactionType || !reaction.selected) {
        reactions.add(reaction);
        continue;
      }
      if (reaction.count > 1) {
        reactions.add(
          MessageReaction(
            type: reaction.type,
            count: reaction.count - 1,
            users: reaction.users
                .where((user) => !_sameUserId(user.id, _user?.id))
                .toList(),
          ),
        );
      }
    }
    return message.withReactions(reactions);
  }

  List<MessageSummary> _withKnownMyReactions(
    Conversation conversation,
    List<MessageSummary> messages,
  ) {
    if (_myReactionKeys.isEmpty && _myRemovedReactionKeys.isEmpty) {
      return messages;
    }
    final now = DateTime.now();
    _myRemovedReactionKeys.removeWhere((_, expiry) => !expiry.isAfter(now));
    if (_myReactionKeys.isEmpty && _myRemovedReactionKeys.isEmpty) {
      return messages;
    }
    return [
      for (final message in messages)
        _withKnownMyReaction(conversation, message, now),
    ];
  }

  MessageSummary _withKnownMyReaction(
    Conversation conversation,
    MessageSummary message,
    DateTime now,
  ) {
    final reactions = <MessageReaction>[];
    for (final reaction in message.reactions) {
      final key = (conversation, message.id, reaction.type);
      final removed = _myRemovedReactionKeys[key]?.isAfter(now) == true;
      if (removed && reaction.selected && reaction.count == 1) continue;
      reactions.add(
        MessageReaction(
          type: reaction.type,
          users: reaction.users
              .where((user) => !removed || !_sameUserId(user.id, _user?.id))
              .toList(),
          count: removed && reaction.selected
              ? reaction.count - 1
              : reaction.count,
          selected:
              !removed && (reaction.selected || _myReactionKeys.contains(key)),
        ),
      );
    }
    for (final key in _myReactionKeys.where(
      (key) => key.$1 == conversation && key.$2 == message.id,
    )) {
      if (_myRemovedReactionKeys[key]?.isAfter(now) == true) continue;
      final type = key.$3;
      if (reactions.any((reaction) => reaction.type == type)) continue;
      reactions.add(MessageReaction(type: type, count: 1, selected: true));
    }
    return message.withReactions(reactions);
  }

  void _replaceMessage(Conversation conversation, MessageSummary replacement) {
    final current =
        _messageCache[conversation] ??
        (_selected == conversation ? _messages : const <MessageSummary>[]);
    final messages = [
      for (final message in current)
        if (message.id == replacement.id) replacement else message,
    ];
    _messageCache[conversation] = messages;
    if (mounted && _selected == conversation) {
      _mutate(() => _messages = messages);
    }
  }

  Conversation _withCachedMetadata(Conversation conversation) {
    final messages = _messageCache[conversation];
    if (messages == null || messages.isEmpty) return conversation;

    var name = conversation.name;
    final genericName =
        name == 'Chat' ||
        name == 'Group chat' ||
        _genericConversationNamePattern.hasMatch(name);
    if (genericName) {
      final otherSenders = <String>[];
      for (final message in messages.reversed) {
        final sender = message.sender.trim();
        final fromCurrentUser =
            message.isFromCurrentUser ||
            _sameUserId(message.senderId, _user?.id) ||
            sender == _user?.displayName;
        if (fromCurrentUser ||
            sender.isEmpty ||
            sender == '?' ||
            sender == 'Unknown' ||
            otherSenders.contains(sender)) {
          continue;
        }
        otherSenders.add(sender);
        if (!conversation.isGroup || otherSenders.length >= 3) break;
      }
      if (otherSenders.isNotEmpty) {
        name = conversation.isGroup
            ? otherSenders.join(', ')
            : otherSenders.first;
      }
    }

    var preview = conversation.preview?.trim();
    if (preview?.isNotEmpty != true) {
      for (final message in messages.reversed) {
        final content = message.content.trim();
        if (content.isNotEmpty) {
          preview = content;
          break;
        }
      }
    }
    if (name == conversation.name && preview == conversation.preview) {
      return conversation;
    }
    return Conversation.chat(
      id: conversation.id,
      name: name,
      isGroup: conversation.isGroup,
      teamId: conversation.teamId,
      profilePhotoUserId: conversation.profilePhotoUserId,
      avatarUserIds: conversation.avatarUserIds,
      lastMessageId: conversation.lastMessageId,
      preview: preview,
    );
  }
}
