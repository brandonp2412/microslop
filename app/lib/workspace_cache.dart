import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/foundation.dart' show listEquals;

const cachedWorkspaceSchemaVersion = 2;

String _string(Object? value, {String fallback = ''}) =>
    value is String ? value : fallback;

String? _optionalString(Object? value) => value is String ? value : null;

String? _nonEmptyString(Object? value) {
  if (value is! String) return null;
  final trimmed = value.trim();
  return trimmed.isEmpty ? null : trimmed;
}

bool _bool(Object? value, {bool fallback = false}) =>
    value is bool ? value : fallback;

Iterable<Object?> _list(Object? value) =>
    value is List ? value.cast<Object?>() : const <Object?>[];

Map<Object?, Object?> _map(Object? value) =>
    value is Map ? value.cast<Object?, Object?>() : const <Object?, Object?>{};

List<String> _stringIds(Object? value) {
  final result = <String>[];
  final seen = <String>{};
  for (final item in _list(value)) {
    final id = _nonEmptyString(item);
    if (id != null && seen.add(id)) result.add(id);
  }
  return result;
}

class CachedConversation {
  const CachedConversation({
    required this.id,
    required this.name,
    required this.isGroup,
    this.teamId,
    this.profilePhotoUserId,
    this.memberUserIds = const [],
    this.lastMessageId,
    this.preview,
  });

  final String id;
  final String name;
  final bool isGroup;
  final String? teamId;
  final String? profilePhotoUserId;
  final List<String> memberUserIds;
  final String? lastMessageId;
  final String? preview;

  Map<String, Object?> toJson() => {
    'id': id,
    'name': name,
    'isGroup': isGroup,
    'teamId': teamId,
    'profilePhotoUserId': profilePhotoUserId,
    'memberUserIds': memberUserIds,
    'lastMessageId': lastMessageId,
    'preview': preview,
  };

  factory CachedConversation.fromJson(Map<String, dynamic> json) =>
      CachedConversation(
        id: _string(json['id']).trim(),
        name: _string(json['name'], fallback: 'Unknown chat'),
        isGroup: _bool(json['isGroup']),
        teamId: _nonEmptyString(json['teamId']),
        profilePhotoUserId: _nonEmptyString(json['profilePhotoUserId']),
        memberUserIds: _stringIds(json['memberUserIds']),
        lastMessageId: _nonEmptyString(json['lastMessageId']),
        preview: _optionalString(json['preview']),
      );

  @override
  bool operator ==(Object other) =>
      other is CachedConversation &&
      id == other.id &&
      name == other.name &&
      isGroup == other.isGroup &&
      teamId == other.teamId &&
      profilePhotoUserId == other.profilePhotoUserId &&
      lastMessageId == other.lastMessageId &&
      preview == other.preview &&
      listEquals(memberUserIds, other.memberUserIds);

  @override
  int get hashCode => Object.hash(
    id,
    name,
    isGroup,
    teamId,
    profilePhotoUserId,
    lastMessageId,
    preview,
    Object.hashAll(memberUserIds),
  );
}

class CachedTeam {
  const CachedTeam({
    required this.id,
    required this.name,
    required this.channels,
  });

  final String id;
  final String name;
  final List<CachedConversation> channels;

  Map<String, Object?> toJson() => {
    'id': id,
    'name': name,
    'channels': channels.map((channel) => channel.toJson()).toList(),
  };

  factory CachedTeam.fromJson(Map<String, dynamic> json) {
    final id = _string(json['id']).trim();
    final channels = <CachedConversation>[];
    final seen = <String>{};
    for (final item in _list(json['channels'])) {
      if (item is! Map) continue;
      final parsed = CachedConversation.fromJson(item.cast<String, dynamic>());
      if (parsed.id.isEmpty || !seen.add(parsed.id)) continue;
      channels.add(
        CachedConversation(
          id: parsed.id,
          name: parsed.name,
          isGroup: false,
          teamId: id.isEmpty ? parsed.teamId : id,
          profilePhotoUserId: parsed.profilePhotoUserId,
          memberUserIds: parsed.memberUserIds,
          lastMessageId: parsed.lastMessageId,
          preview: parsed.preview,
        ),
      );
    }
    return CachedTeam(
      id: id,
      name: _string(json['name'], fallback: 'Unknown team'),
      channels: channels,
    );
  }
}

class CachedQuote {
  const CachedQuote({
    this.messageId,
    required this.sender,
    required this.content,
  });

  final String? messageId;
  final String sender;
  final String content;

  Map<String, Object?> toJson() => {
    'messageId': messageId,
    'sender': sender,
    'content': content,
  };

  static CachedQuote? tryFromJson(Object? value) {
    if (value is! Map) return null;
    final json = value.cast<Object?, Object?>();
    final sender = _string(json['sender']).trim();
    final content = _string(json['content']).trim();
    if (sender.isEmpty && content.isEmpty) return null;
    return CachedQuote(
      messageId: _nonEmptyString(json['messageId']),
      sender: sender,
      content: content,
    );
  }
}

class CachedImage {
  const CachedImage({required this.contentType, required this.bytes});

  final String contentType;
  final Uint8List bytes;

  Map<String, Object> toJson() => {
    'contentType': contentType,
    'dataBase64': base64Encode(bytes),
  };

  static CachedImage? tryFromJson(Object? value) {
    if (value is! Map) return null;
    final json = value.cast<Object?, Object?>();
    final contentType = _nonEmptyString(json['contentType']);
    final encoded = _nonEmptyString(json['dataBase64']);
    if (contentType == null || encoded == null) return null;
    try {
      final bytes = base64Decode(encoded);
      if (bytes.isEmpty) return null;
      return CachedImage(contentType: contentType, bytes: bytes);
    } on FormatException {
      return null;
    }
  }
}

class CachedMessage {
  const CachedMessage({
    required this.id,
    required this.sender,
    this.senderId,
    required this.isFromCurrentUser,
    required this.timestamp,
    required this.content,
    this.quotes = const [],
    this.images = const [],
    this.reactions = const [],
  });

  final String id;
  final String sender;
  final String? senderId;
  final bool isFromCurrentUser;
  final String timestamp;
  final String content;
  final List<CachedQuote> quotes;
  final List<CachedImage> images;
  final List<CachedReaction> reactions;

  Map<String, Object?> toJson() => {
    'id': id,
    'sender': sender,
    'senderId': senderId,
    'isFromCurrentUser': isFromCurrentUser,
    'timestamp': timestamp,
    'content': content,
    'quotes': quotes.map((quote) => quote.toJson()).toList(),
    'images': images.map((image) => image.toJson()).toList(),
    'reactions': reactions.map((reaction) => reaction.toJson()).toList(),
  };

  factory CachedMessage.fromJson(Map<String, dynamic> json) {
    final reactionsByType = <String, (int, bool)>{};
    for (final value in _list(json['reactions'])) {
      if (value is! Map) continue;
      final reaction = CachedReaction.fromJson(value.cast<String, dynamic>());
      if (reaction.type.isEmpty || reaction.count <= 0) continue;
      final existing = reactionsByType[reaction.type];
      reactionsByType[reaction.type] = (
        (existing?.$1 ?? 0) + reaction.count,
        (existing?.$2 ?? false) || reaction.selected,
      );
    }
    return CachedMessage(
      id: _string(json['id']).trim(),
      sender: _string(json['sender'], fallback: '?'),
      senderId: _nonEmptyString(json['senderId']),
      isFromCurrentUser: _bool(json['isFromCurrentUser']),
      timestamp: _string(json['timestamp']),
      content: _string(json['content']),
      quotes: [
        for (final value in _list(json['quotes']))
          if (CachedQuote.tryFromJson(value) case final quote?) quote,
      ],
      images: [
        for (final value in _list(json['images']))
          if (CachedImage.tryFromJson(value) case final image?) image,
      ],
      reactions: [
        for (final entry in reactionsByType.entries)
          CachedReaction(
            type: entry.key,
            count: entry.value.$1,
            selected: entry.value.$2,
          ),
      ],
    );
  }
}

class CachedReaction {
  const CachedReaction({
    required this.type,
    required this.count,
    this.selected = false,
  });

  final String type;
  final int count;
  final bool selected;

  Map<String, Object> toJson() => {
    'type': type,
    'count': count,
    'selected': selected,
  };

  factory CachedReaction.fromJson(Map<String, dynamic> json) {
    final rawCount = json['count'];
    return CachedReaction(
      type: _string(json['type']).trim(),
      count: rawCount is num ? rawCount.toInt().clamp(0, 1 << 30) : 0,
      selected: _bool(json['selected']),
    );
  }
}

class CachedWorkspace {
  const CachedWorkspace({
    required this.chats,
    required this.teams,
    required this.selectedChatId,
    required this.messages,
    this.hiddenConversationIds = const [],
    this.hiddenSectionIds = const {},
    this.schemaVersion = cachedWorkspaceSchemaVersion,
  });

  final int schemaVersion;
  final List<CachedConversation> chats;
  final List<CachedTeam> teams;
  final String? selectedChatId;
  final Map<String, List<CachedMessage>> messages;
  final List<String> hiddenConversationIds;
  final Set<String> hiddenSectionIds;

  Map<String, Object?> toJson() => {
    'schemaVersion': schemaVersion,
    'chats': chats.map((chat) => chat.toJson()).toList(),
    'teams': teams.map((team) => team.toJson()).toList(),
    'selectedChatId': selectedChatId,
    'messages': {
      for (final entry in messages.entries)
        entry.key: entry.value.map((message) => message.toJson()).toList(),
    },
    'hiddenConversationIds': hiddenConversationIds,
    'hiddenSectionIds': hiddenSectionIds.toList(),
  };

  factory CachedWorkspace.fromJson(
    Map<String, dynamic> json, {
    bool includeMessages = true,
  }) {
    final chats = <CachedConversation>[];
    final seenChatIds = <String>{};
    for (final value in _list(json['chats'])) {
      if (value is! Map) continue;
      final chat = CachedConversation.fromJson(value.cast<String, dynamic>());
      if (chat.id.isEmpty || !seenChatIds.add(chat.id)) continue;
      chats.add(chat);
    }

    final teams = <CachedTeam>[];
    final seenTeamIds = <String>{};
    for (final value in _list(json['teams'])) {
      if (value is! Map) continue;
      final team = CachedTeam.fromJson(value.cast<String, dynamic>());
      if (team.id.isEmpty || !seenTeamIds.add(team.id)) continue;
      teams.add(team);
    }

    final messages = <String, List<CachedMessage>>{};
    if (includeMessages) {
      for (final entry in _map(json['messages']).entries) {
        final id = _nonEmptyString(entry.key);
        if (id == null || entry.value is! List) continue;
        messages[id] = [
          for (final value in _list(entry.value))
            if (value is Map)
              CachedMessage.fromJson(value.cast<String, dynamic>()),
        ];
      }
    }

    final rawVersion = json['schemaVersion'];
    return CachedWorkspace(
      schemaVersion: rawVersion is num
          ? rawVersion.toInt().clamp(0, 1 << 30)
          : 0,
      chats: chats,
      teams: teams,
      selectedChatId: _nonEmptyString(json['selectedChatId']),
      messages: messages,
      hiddenConversationIds: _stringIds(json['hiddenConversationIds']),
      hiddenSectionIds: _stringIds(json['hiddenSectionIds']).toSet(),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is CachedWorkspace &&
      jsonEncode(toJson()) == jsonEncode(other.toJson());

  @override
  int get hashCode => jsonEncode(toJson()).hashCode;
}

List<String> prefetchConversationIds({
  required List<CachedConversation> directMessages,
  required List<CachedConversation> groups,
  required List<CachedConversation> channels,
  int limit = 8,
}) {
  if (limit <= 0) return const [];
  final ids = <String>[];
  final seen = <String>{};
  for (final conversations in [directMessages, groups, channels]) {
    for (final conversation in conversations) {
      final id = conversation.id.trim();
      if (id.isEmpty || !seen.add(id)) continue;
      ids.add(id);
      if (ids.length >= limit) return ids;
    }
  }
  return ids;
}

bool isSelfChat(CachedConversation chat, String? userId) {
  final id = userId?.trim();
  return id != null &&
      id.isNotEmpty &&
      !chat.isGroup &&
      chat.teamId == null &&
      chat.memberUserIds.contains(id);
}
