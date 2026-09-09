import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'app_log.dart';
import 'src/rust/api/teams.dart' as rust;
import 'src/rust/api/trouter.dart' as trouter;

final _reactionCodepointPattern = RegExp(r'^([0-9a-f]{4,6})_');

String validateReactionType(String value) {
  final type = value.trim();
  if (type.isEmpty || type.runes.length > 64) {
    throw ArgumentError.value(value, 'reactionType');
  }
  return type;
}

String reactionEmoji(String type) {
  final separator = type.indexOf(';');
  final name = (separator < 0 ? type : type.substring(0, separator))
      .trim()
      .toLowerCase();
  final codepoint = _reactionCodepointPattern.firstMatch(name)?.group(1);
  if (codepoint != null) {
    return String.fromCharCode(int.parse(codepoint, radix: 16));
  }
  const exact = {
    'like': '👍',
    'heart': '❤️',
    'laugh': '😂',
    'surprised': '😮',
    'sad': '😢',
    'angry': '😠',
    'ambulance': '🚑',
    'aubergine': '🍆',
    'basketball': '🏀',
    'bomb': '💣',
    'burger': '🍔',
    'cake': '🎂',
    'champagne': '🍾',
    'coin': '🪙',
    'computer': '💻',
    'detective': '🕵️',
    'ghost': '👻',
    'guitar': '🎸',
    'headphones': '🎧',
    'lamb': '🐑',
    'launch': '🚀',
    'mousetrap': '🪤',
    'rainbow': '🌈',
    'rugbyball': '🏉',
    'snail': '🐌',
    'sun': '☀️',
    'sunflower': '🌻',
    'trophy': '🏆',
    'truck': '🚚',
    'wizard': '🧙',
    'wood': '🪵',
    'yoga': '🧘',
  };
  final direct = exact[name];
  if (direct != null) return direct;
  const matches = <(String, String)>[
    ('approved', '✅'),
    ('resolved', '✅'),
    ('check', '✅'),
    ('yes', '✅'),
    ('upvote', '⬆️'),
    ('no', '❌'),
    ('cross', '❌'),
    ('warning', '⚠️'),
    ('alert', '⚠️'),
    ('party', '🎉'),
    ('cheer', '🎉'),
    ('dance', '💃'),
    ('confetti', '🎊'),
    ('fire', '🔥'),
    ('hot', '🔥'),
    ('heart', '❤️'),
    ('love', '🥰'),
    ('kiss', '💋'),
    ('laugh', '😂'),
    ('lol', '😂'),
    ('giggle', '😄'),
    ('smile', '😊'),
    ('grin', '😁'),
    ('happy', '😄'),
    ('cry', '😭'),
    ('sad', '😢'),
    ('lonely', '😔'),
    ('angry', '😠'),
    ('swear', '🤬'),
    ('pout', '😡'),
    ('shock', '😲'),
    ('surpris', '😮'),
    ('fear', '😱'),
    ('confus', '😕'),
    ('think', '🤔'),
    ('huh', '🤨'),
    ('suspicious', '🤨'),
    ('side-eye', '👀'),
    ('eye', '👀'),
    ('salut', '🫡'),
    ('wave', '👋'),
    ('clap', '👏'),
    ('thumb', '👍'),
    ('fist', '🤜'),
    ('muscle', '💪'),
    ('pray', '🙏'),
    ('hand', '🙌'),
    ('finger', '🫰'),
    ('hug', '🤗'),
    ('bow', '🙇'),
    ('facepalm', '🤦'),
    ('shrug', '🤷'),
    ('cool', '😎'),
    ('nerd', '🤓'),
    ('tongue', '😛'),
    ('wink', '😉'),
    ('unamused', '😒'),
    ('sick', '🤢'),
    ('puke', '🤮'),
    ('ill', '🤒'),
    ('dead', '💀'),
    ('skull', '💀'),
    ('poop', '💩'),
    ('brain', '🧠'),
    ('idea', '💡'),
    ('light', '💡'),
    ('money', '💰'),
    ('stonk', '📈'),
    ('chart', '📈'),
    ('star', '⭐'),
    ('spark', '✨'),
    ('rainbow', '🌈'),
    ('cat', '🐈'),
    ('meow', '🐈'),
    ('dog', '🐕'),
    ('wolf', '🐺'),
    ('monkey', '🐒'),
    ('pigeon', '🐦'),
    ('parrot', '🦜'),
    ('duck', '🦆'),
    ('goat', '🐐'),
    ('alien', '👽'),
    ('robot', '🤖'),
    ('devil', '😈'),
    ('superhero', '🦸'),
    ('villain', '🦹'),
    ('ninja', '🥷'),
    ('santa', '🎅'),
    ('xmas', '🎄'),
    ('holiday', '🎄'),
    ('beer', '🍻'),
    ('cake', '🎂'),
    ('flower', '🌺'),
    ('banana', '🍌'),
    ('cucumber', '🥒'),
    ('soap', '🧼'),
    ('toilet', '🚽'),
    ('bone', '🦴'),
    ('car', '🚗'),
    ('runner', '🏃'),
    ('golf', '🏌️'),
    ('student', '🧑‍🎓'),
    ('graduate', '🎓'),
    ('bug', '🐛'),
    ('loading', '⏳'),
    ('typing', '⌨️'),
    ('thank', '🙏'),
    ('support', '🤝'),
    ('fine', '🔥'),
  ];
  for (final (term, emoji) in matches) {
    if (name.contains(term)) return emoji;
  }
  return '✨';
}

enum ConversationKind { chat, channel }

class Conversation {
  const Conversation.chat({
    required this.id,
    required this.name,
    required this.isGroup,
    this.teamId,
    this.profilePhotoUserId,
    this.avatarUserIds = const [],
    this.lastMessageId,
    this.preview,
  }) : kind = ConversationKind.chat;

  const Conversation.channel({
    required this.id,
    required this.name,
    required this.teamId,
  }) : kind = ConversationKind.channel,
       isGroup = false,
       profilePhotoUserId = null,
       avatarUserIds = const [],
       lastMessageId = null,
       preview = null;

  final String id;
  final String name;
  final String? teamId;
  final String? profilePhotoUserId;
  final List<String> avatarUserIds;
  final String? lastMessageId;
  final String? preview;
  final bool isGroup;
  final ConversationKind kind;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is Conversation &&
          other.kind == kind &&
          other.id == id &&
          other.teamId == teamId;

  @override
  int get hashCode => Object.hash(kind, id, teamId);
}

class TeamSummary {
  const TeamSummary({
    required this.id,
    required this.name,
    required this.channels,
  });

  final String id;
  final String name;
  final List<Conversation> channels;
}

class MessageSummary {
  const MessageSummary({
    this.id = '',
    required this.sender,
    this.senderId,
    this.isFromCurrentUser = false,
    required this.timestamp,
    required this.content,
    this.quotes = const [],
    this.images = const [],
    this.imageUrls = const [],
    this.cachedImageCount = 0,
    this.reactions = const [],
  });

  final String id;
  final String sender;
  final String? senderId;
  final bool isFromCurrentUser;
  final String timestamp;
  final String content;
  final List<MessageQuote> quotes;
  final List<MessageImage> images;
  final List<String> imageUrls;
  final int cachedImageCount;
  final List<MessageReaction> reactions;

  MessageSummary withReactions(List<MessageReaction> reactions) =>
      MessageSummary(
        id: id,
        sender: sender,
        senderId: senderId,
        isFromCurrentUser: isFromCurrentUser,
        timestamp: timestamp,
        content: content,
        quotes: quotes,
        images: images,
        imageUrls: imageUrls,
        cachedImageCount: cachedImageCount,
        reactions: reactions,
      );
}

class MessageQuote {
  const MessageQuote({
    this.messageId,
    required this.sender,
    required this.content,
  });

  final String? messageId;
  final String sender;
  final String content;
}

class MessageImage {
  const MessageImage({
    required this.contentType,
    required this.bytes,
    this.sourceUrl,
  });

  final String? sourceUrl;

  final String contentType;
  final Uint8List bytes;
}

class ReactionUser {
  const ReactionUser({required this.id, required this.name});
  final String id;
  final String name;
}

class MessageReaction {
  const MessageReaction({
    required this.type,
    required this.count,
    this.selected = false,
    this.users = const [],
  });

  final List<ReactionUser> users;
  final String type;
  final int count;
  final bool selected;
}

class TeamCustomReaction {
  const TeamCustomReaction({
    required this.reactionType,
    required this.shortcut,
    required this.documentId,
    required this.contentType,
    required this.icon,
  });

  final String reactionType;
  final String shortcut;
  final String documentId;
  final String contentType;
  final Uint8List icon;
}

class MessageEvent {
  const MessageEvent({
    this.conversationId,
    this.accountId,
    this.accountActive = true,
    this.resourceType = '',
  });

  final String? conversationId;
  final String? accountId;
  final bool accountActive;
  final String resourceType;
}

class SavedAccountNotificationSummary {
  const SavedAccountNotificationSummary({
    required this.accountId,
    required this.accountName,
    required this.conversationId,
    required this.conversationName,
    required this.isGroup,
    required this.isChannel,
    required this.message,
    this.messages = const [],
  });

  final String accountId;
  final String accountName;
  final String conversationId;
  final String conversationName;
  final bool isGroup;
  final bool isChannel;
  final MessageSummary message;
  final List<MessageSummary> messages;
}

class UserSummary {
  const UserSummary({this.id, required this.displayName, this.email});

  final String? id;
  final String displayName;
  final String? email;
}

class UserDetailsSummary {
  const UserDetailsSummary({
    required this.userId,
    required this.displayName,
    this.email,
    this.jobTitle,
    this.availability,
    this.activity,
    this.statusMessage,
  });

  final String userId;
  final String displayName;
  final String? email;
  final String? jobTitle;
  final String? availability;
  final String? activity;
  final String? statusMessage;
}

class PresenceSummary {
  const PresenceSummary({
    required this.userId,
    required this.availability,
    required this.activity,
  });

  final String userId;
  final String availability;
  final String activity;
}

abstract interface class TeamsGateway {
  Future<List<TeamSummary>> listTeams();
  Future<List<Conversation>> listChats({int limit = 50});
  Future<List<MessageSummary>> readMessages(Conversation conversation);
  Future<void> sendMessage(Conversation conversation, String content);
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  });
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  );
  Future<UserSummary> getUser();
  Future<Uint8List?> getProfilePhoto(String userId);
  Future<Uint8List?> getChatPhoto(String chatId);
  Future<Uint8List?> getTeamPhoto(String teamId);
  Stream<MessageEvent> messageEvents();
  Future<void> stopMessageEvents();
}

abstract interface class FileTeamsGateway {
  Future<void> sendFileMessage(
    Conversation conversation,
    Uint8List bytes,
    String fileName,
    String contentType,
  );
}

abstract interface class ResponsiveTeamsGateway {
  Future<List<MessageSummary>> refreshMessages(Conversation conversation);
  Future<List<MessageSummary>> hydrateRecentImages(Conversation conversation);
}

abstract interface class LazyImageTeamsGateway {
  Future<MessageImage?> loadMessageImage(String url);
}

abstract interface class ReactionUsersTeamsGateway {
  Future<String> reactionUserName(String userId);
}

abstract interface class ImageHistoryTeamsGateway {
  Future<List<MessageSummary>> readImageHistory(Conversation conversation);
}

abstract interface class MessageSyncTeamsGateway {
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  );
}

abstract interface class CustomReactionTeamsGateway {
  Future<TeamCustomReaction?> getCustomReaction(String reactionType);
}

abstract interface class MultiAccountTeamsGateway {
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  });
}

abstract interface class PresenceTeamsGateway {
  Future<List<PresenceSummary>> getPresences(List<String> userIds);
}

abstract interface class UserDetailsTeamsGateway {
  Future<UserDetailsSummary?> getUserDetails(String userId);
}

typedef ChatLoader = Future<List<rust.Chat>> Function({required BigInt limit});

class RustTeamsGateway
    implements
        ImageHistoryTeamsGateway,
        LazyImageTeamsGateway,
        ReactionUsersTeamsGateway,
        TeamsGateway,
        FileTeamsGateway,
        ResponsiveTeamsGateway,
        MessageSyncTeamsGateway,
        CustomReactionTeamsGateway,
        MultiAccountTeamsGateway,
        PresenceTeamsGateway,
        UserDetailsTeamsGateway {
  RustTeamsGateway({ChatLoader? chatLoader, Duration? chatTimeout})
    : _chatLoader = chatLoader ?? rust.listChats,
      _chatTimeout = chatTimeout ?? const Duration(seconds: 30);

  static const _photoTimeout = Duration(seconds: 5);
  static const _maxPhotoCacheEntries = 256;

  final ChatLoader _chatLoader;
  final Duration _chatTimeout;
  final _profilePhotos = <String, Uint8List?>{};
  final _profilePhotoLoads = <String, Future<Uint8List?>>{};
  final _chatPhotos = <String, Uint8List?>{};
  final _chatPhotoLoads = <String, Future<Uint8List?>>{};
  final _teamPhotos = <String, Uint8List?>{};
  final _teamPhotoLoads = <String, Future<Uint8List?>>{};
  final _customReactions = <String, TeamCustomReaction?>{};
  final _customReactionLoads = <String, Future<TeamCustomReaction?>>{};
  final _userDetails = <String, UserDetailsSummary?>{};
  final _userDetailLoads = <String, Future<UserDetailsSummary?>>{};

  @override
  Future<TeamCustomReaction?> getCustomReaction(String reactionType) {
    final key = reactionType.trim();
    if (_customReactions.containsKey(key)) {
      return Future.value(_customReactions[key]);
    }
    return _customReactionLoads[key] ??= _loadCustomReaction(key);
  }

  Future<TeamCustomReaction?> _loadCustomReaction(String reactionType) async {
    try {
      final reaction = await rust.getCustomReaction(reactionType: reactionType);
      if (reaction == null) {
        _customReactions[reactionType] = null;
        return null;
      }
      final icon = base64Decode(reaction.contentBase64);
      if (icon.isEmpty) {
        _customReactions[reactionType] = null;
        return null;
      }
      final result = TeamCustomReaction(
        reactionType: reaction.reactionType,
        shortcut: reaction.shortcut,
        documentId: reaction.documentId,
        contentType: reaction.contentType,
        icon: icon,
      );
      _customReactions[reactionType] = result;
      return result;
    } catch (error) {
      AppLog.debug('Custom reaction', 'type=$reactionType, error=$error');
      _customReactions[reactionType] = null;
      return null;
    } finally {
      _customReactionLoads.remove(reactionType);
    }
  }

  @override
  Future<List<TeamSummary>> listTeams() async {
    final rawTeams = await rust.listTeams();
    final teams = <TeamSummary>[];
    final seenTeams = <String>{};
    for (final team in rawTeams) {
      final teamId = team.id.trim();
      if (teamId.isEmpty || !seenTeams.add(teamId)) continue;
      final channels = <Conversation>[];
      final seenChannels = <String>{};
      for (final channel in team.channels) {
        final channelId = channel.id.trim();
        if (channelId.isEmpty || !seenChannels.add(channelId)) continue;
        final channelName = channel.name.trim();
        channels.add(
          Conversation.channel(
            id: channelId,
            name: channelName.isEmpty ? 'Unnamed channel' : channelName,
            teamId: teamId,
          ),
        );
      }
      final teamName = team.name.trim();
      teams.add(
        TeamSummary(
          id: teamId,
          name: teamName.isEmpty ? 'Unnamed team' : teamName,
          channels: channels,
        ),
      );
    }
    return teams;
  }

  @override
  Future<UserDetailsSummary?> getUserDetails(String userId) {
    final id = userId.trim();
    if (id.isEmpty) return Future.value();
    if (_userDetails.containsKey(id)) return Future.value(_userDetails[id]);
    return _userDetailLoads.putIfAbsent(id, () async {
      try {
        final details = await rust.getUserDetails(userId: id);
        final result = UserDetailsSummary(
          userId: details.userId,
          displayName: details.displayName,
          email: details.email,
          jobTitle: details.jobTitle,
          availability: details.availability,
          activity: details.activity,
          statusMessage: details.statusMessage,
        );
        _userDetails[id] = result;
        return result;
      } catch (error) {
        AppLog.debug('User details', 'userId=$id, error=$error');
        return null;
      } finally {
        _userDetailLoads.remove(id);
      }
    });
  }

  @override
  Future<List<PresenceSummary>> getPresences(List<String> userIds) async {
    final ids = <String>[];
    final seen = <String>{};
    for (final rawId in userIds) {
      final id = rawId.trim();
      if (id.isEmpty || !seen.add(id)) continue;
      ids.add(id);
      if (ids.length == 650) break;
    }
    if (ids.isEmpty) return const [];
    final presences = await rust.getPresences(userIds: ids);
    return [
      for (final presence in presences)
        PresenceSummary(
          userId: presence.userId,
          availability: presence.availability,
          activity: presence.activity,
        ),
    ];
  }

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    final safeLimit = limit.clamp(1, 500);
    final rawChats = await _chatLoader(
      limit: BigInt.from(safeLimit),
    ).timeout(_chatTimeout);
    final chats = <Conversation>[];
    final seen = <String>{};
    for (final chat in rawChats) {
      final id = chat.id.trim();
      final teamId = chat.teamId?.trim();
      final key = '${teamId ?? ''}:$id';
      if (id.isEmpty || !seen.add(key)) continue;
      final name = chat.name.trim();
      final avatarUserIds = <String>[];
      final seenAvatarIds = <String>{};
      for (final rawId in chat.memberUserIds) {
        final userId = rawId.trim();
        if (userId.isNotEmpty && seenAvatarIds.add(userId)) {
          avatarUserIds.add(userId);
        }
      }
      final profilePhotoUserId = chat.profilePhotoUserId?.trim();
      chats.add(
        Conversation.chat(
          id: id,
          name: name.isEmpty ? 'Unnamed chat' : name,
          isGroup: chat.isGroup,
          teamId: teamId?.isEmpty == true ? null : teamId,
          profilePhotoUserId: profilePhotoUserId?.isEmpty == true
              ? null
              : profilePhotoUserId,
          avatarUserIds: avatarUserIds,
          lastMessageId: chat.lastMessageId,
          preview: chat.lastMessagePreview,
        ),
      );
    }
    if (kDebugMode) {
      AppLog.debug(
        'Chat data',
        'loaded=${chats.length}, requested=$safeLimit, groups=${chats.where((chat) => chat.isGroup).length}',
      );
    }
    return chats;
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) =>
      _readMessages(conversation, limit: 100, includeImages: true);

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) =>
      _readMessages(conversation, limit: 100, includeImages: false);

  @override
  Future<List<MessageSummary>> hydrateRecentImages(Conversation conversation) =>
      _readMessages(conversation, limit: 100, includeImages: true);

  @override
  Future<List<MessageSummary>> readImageHistory(Conversation conversation) =>
      _readMessages(conversation, limit: 0x7fffffff, includeImages: false);

  @override
  Future<MessageImage?> loadMessageImage(String url) async {
    final image = await rust.loadMessageImage(url: url);
    return image == null
        ? null
        : MessageImage(
            contentType: image.contentType,
            sourceUrl: image.sourceUrl,
            bytes: base64Decode(image.dataBase64),
          );
  }

  final _reactionUserNames = <String, Future<String>>{};

  @override
  Future<String> reactionUserName(String userId) =>
      _reactionUserNames.putIfAbsent(
        userId,
        () => rust.reactionUserName(userId: userId).catchError((Object error) {
          _reactionUserNames.remove(userId);
          return userId;
        }),
      );

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    final chats = await listChats(limit: 500);
    final teams = await listTeams();
    final conversations = <Conversation>[
      ...chats,
      for (final team in teams) ...team.channels,
    ];
    onProgress(0, conversations.length);
    for (var index = 0; index < conversations.length; index++) {
      final conversation = conversations[index];
      final messages = await _readMessages(
        conversation,
        limit: perConversation,
        includeImages: true,
      );
      await onMessages(conversation, messages);
      onProgress(index + 1, conversations.length);
    }
  }

  String? _trimmedOrNull(String? value) {
    final trimmed = value?.trim();
    return trimmed?.isEmpty == true ? null : trimmed;
  }

  Future<List<MessageSummary>> _readMessages(
    Conversation conversation, {
    required int limit,
    required bool includeImages,
  }) async {
    final messages = switch (conversation.kind) {
      ConversationKind.chat => await rust.readMessages(
        conversationId: conversation.id,
        limit: BigInt.from(limit),
        includeImages: includeImages,
      ),
      ConversationKind.channel => await rust.readChannelMessages(
        teamId: conversation.teamId!,
        channelId: conversation.id,
        limit: BigInt.from(limit),
        includeImages: includeImages,
      ),
    };
    final summaries = <MessageSummary>[];
    final seenMessageIds = <String>{};
    final observedReactions = <String>{};
    for (final message in messages) {
      final id = message.id.trim();
      if (id.isNotEmpty && !seenMessageIds.add(id)) continue;
      final sender = message.sender.trim();
      final reactions = <String, (int, bool, List<ReactionUser>)>{};
      for (final reaction in message.reactions) {
        final type = reaction.reactionType.trim();
        final count = reaction.count.toInt();
        if (type.isEmpty || count <= 0) continue;
        final existing = reactions[type];
        reactions[type] = (
          (existing?.$1 ?? 0) + count,
          (existing?.$2 ?? false) || reaction.selected,
          [
            ...?existing?.$3,
            for (final user in reaction.users)
              ReactionUser(id: user.id, name: user.name),
          ],
        );
        observedReactions.add(type);
      }
      summaries.add(
        MessageSummary(
          id: id,
          sender: sender.isEmpty ? '?' : sender,
          senderId: _trimmedOrNull(message.senderId),
          isFromCurrentUser: message.isFromCurrentUser,
          timestamp: message.timestamp,
          content: message.content,
          quotes: [
            for (final quote in message.quotes)
              MessageQuote(
                messageId: _trimmedOrNull(quote.messageId),
                sender: quote.sender.trim(),
                content: quote.content.trim(),
              ),
          ],
          images: _decodeImages(message.images),
          imageUrls: message.imageUrls,
          reactions: [
            for (final reaction in reactions.entries)
              MessageReaction(
                type: reaction.key,
                count: reaction.value.$1,
                selected: reaction.value.$2,
                users: reaction.value.$3,
              ),
          ],
        ),
      );
    }
    if (kDebugMode) {
      AppLog.debug(
        'Message data',
        'conversation=${conversation.id}, loaded=${summaries.length}, images=$includeImages, reactions=${observedReactions.toList()..sort()}',
      );
    }
    return summaries;
  }

  List<MessageImage> _decodeImages(List<rust.MessageImage> images) {
    final decoded = <MessageImage>[];
    for (final image in images) {
      try {
        final contentType = image.contentType.trim().toLowerCase();
        if (!contentType.startsWith('image/')) continue;
        final bytes = base64Decode(image.dataBase64);
        if (bytes.isEmpty) continue;
        decoded.add(
          MessageImage(
            contentType: contentType,
            bytes: bytes,
            sourceUrl: image.sourceUrl,
          ),
        );
      } on FormatException catch (error) {
        AppLog.record('Decode message image', error);
      }
    }
    return decoded;
  }

  String _conversationId(Conversation conversation) {
    final id = conversation.id.trim();
    if (id.isEmpty) {
      throw ArgumentError.value(conversation.id, 'conversation.id');
    }
    return id;
  }

  String _teamId(Conversation conversation) {
    final id = conversation.teamId?.trim();
    if (id == null || id.isEmpty) {
      throw StateError('Channel ${conversation.id} does not have a team id.');
    }
    return id;
  }

  @override
  Future<void> sendMessage(Conversation conversation, String content) {
    final conversationId = _conversationId(conversation);
    if (content.trim().isEmpty) return Future.value();
    return switch (conversation.kind) {
      ConversationKind.chat => rust.sendMessage(
        conversationId: conversationId,
        content: content,
      ),
      ConversationKind.channel => rust.sendChannelMessage(
        teamId: _teamId(conversation),
        channelId: conversationId,
        content: content,
      ),
    };
  }

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) {
    final conversationId = _conversationId(conversation);
    if (bytes.isEmpty) return Future.value();
    final normalizedType = contentType.trim().toLowerCase();
    if (!normalizedType.startsWith('image/')) {
      return Future.error(ArgumentError.value(contentType, 'contentType'));
    }
    return rust.sendImageMessage(
      conversationId: conversationId,
      teamId: conversation.kind == ConversationKind.channel
          ? _teamId(conversation)
          : null,
      caption: caption,
      contentType: normalizedType,
      dataBase64: base64Encode(bytes),
    );
  }

  @override
  Future<void> sendFileMessage(
    Conversation conversation,
    Uint8List bytes,
    String fileName,
    String contentType,
  ) {
    final conversationId = _conversationId(conversation);
    if (bytes.isEmpty) return Future.value();
    return rust.sendFileMessage(
      conversationId: conversationId,
      teamId: conversation.kind == ConversationKind.channel
          ? _teamId(conversation)
          : null,
      fileName: fileName,
      contentType: contentType.trim().isEmpty
          ? 'application/octet-stream'
          : contentType,
      dataBase64: base64Encode(bytes),
    );
  }

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) {
    final conversationId = _conversationId(conversation);
    final messageId = message.id.trim();
    final type = validateReactionType(reactionType);
    if (messageId.isEmpty) {
      return Future.error(ArgumentError.value(message.id, 'message.id'));
    }
    return rust.setReaction(
      conversationId: conversationId,
      teamId: conversation.kind == ConversationKind.channel
          ? _teamId(conversation)
          : null,
      messageId: messageId,
      reactionType: type,
      remove: message.reactions.any(
        (reaction) => reaction.type == type && reaction.selected,
      ),
    );
  }

  @override
  Stream<MessageEvent> messageEvents() => trouter.listenMessageEvents().map(
    (event) => MessageEvent(
      conversationId: event.conversationId,
      accountId: event.accountId,
      accountActive: event.accountActive,
      resourceType: event.resourceType,
    ),
  );

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    final notification = await rust.readSavedAccountNotification(
      accountId: accountId,
      conversationId: conversationId,
      messageLimit: BigInt.from(recentMessageLimit.clamp(1, 25)),
    );
    if (notification == null) return null;
    MessageSummary toSummary(rust.Message message) {
      final sender = message.sender.trim();
      final reactions = <String, (int, bool, List<ReactionUser>)>{};
      for (final reaction in message.reactions) {
        final type = reaction.reactionType.trim();
        final count = reaction.count.toInt();
        if (type.isEmpty || count <= 0) continue;
        final existing = reactions[type];
        reactions[type] = (
          (existing?.$1 ?? 0) + count,
          (existing?.$2 ?? false) || reaction.selected,
          [
            ...?existing?.$3,
            for (final user in reaction.users)
              ReactionUser(id: user.id, name: user.name),
          ],
        );
      }
      return MessageSummary(
        id: message.id.trim(),
        sender: sender.isEmpty ? '?' : sender,
        senderId: _trimmedOrNull(message.senderId),
        isFromCurrentUser: message.isFromCurrentUser,
        timestamp: message.timestamp,
        content: message.content,
        reactions: [
          for (final reaction in reactions.entries)
            MessageReaction(
              type: reaction.key,
              count: reaction.value.$1,
              selected: reaction.value.$2,
              users: reaction.value.$3,
            ),
        ],
      );
    }

    final messages = notification.messages
        .map(toSummary)
        .toList(growable: false);
    return SavedAccountNotificationSummary(
      accountId: notification.accountId,
      accountName: notification.accountName,
      conversationId: notification.conversationId,
      conversationName: notification.conversationName,
      isGroup: notification.isGroup,
      isChannel: notification.isChannel,
      message: toSummary(notification.message),
      messages: messages,
    );
  }

  @override
  Future<void> stopMessageEvents() => trouter.stopMessageEvents();

  @override
  Future<UserSummary> getUser() async {
    final profile = await rust.getUserProfile();
    final id = profile.id?.trim();
    final email = profile.email?.trim();
    final displayName = profile.displayName.trim();
    return UserSummary(
      id: id?.isEmpty == true ? null : id,
      displayName: displayName.isNotEmpty
          ? displayName
          : (email?.isNotEmpty == true ? email! : 'You'),
      email: email?.isEmpty == true ? null : email,
    );
  }

  void _rememberPhoto(
    Map<String, Uint8List?> cache,
    String id,
    Uint8List? bytes,
  ) {
    cache[id] = bytes;
    while (cache.length > _maxPhotoCacheEntries) {
      cache.remove(cache.keys.first);
    }
  }

  @override
  Future<Uint8List?> getProfilePhoto(String userId) {
    final id = userId.trim();
    if (id.isEmpty) return Future.value();
    if (_profilePhotos.containsKey(id)) return Future.value(_profilePhotos[id]);
    return _profilePhotoLoads.putIfAbsent(id, () async {
      try {
        final photo = await rust
            .getProfilePhoto(userId: id)
            .timeout(_photoTimeout);
        final decoded = photo == null ? null : base64Decode(photo);
        _rememberPhoto(_profilePhotos, id, decoded);
        AppLog.debug(
          'Profile photo',
          'userId=$id, loaded=${decoded != null}, bytes=${decoded?.length ?? 0}',
        );
        return decoded;
      } catch (error) {
        AppLog.debug('Profile photo', 'userId=$id, error=$error');
        return null;
      } finally {
        _profilePhotoLoads.remove(id);
      }
    });
  }

  @override
  Future<Uint8List?> getChatPhoto(String chatId) {
    final id = chatId.trim();
    if (id.isEmpty) return Future.value();
    if (_chatPhotos.containsKey(id)) return Future.value(_chatPhotos[id]);
    return _chatPhotoLoads.putIfAbsent(id, () async {
      try {
        final photo = await rust
            .getChatPhoto(chatId: id)
            .timeout(_photoTimeout);
        final decoded = photo == null ? null : base64Decode(photo);
        _rememberPhoto(_chatPhotos, id, decoded);
        AppLog.debug(
          'Chat photo',
          'chatId=$id, loaded=${decoded != null}, bytes=${decoded?.length ?? 0}',
        );
        return decoded;
      } catch (error) {
        AppLog.debug('Chat photo', 'chatId=$id, error=$error');
        return null;
      } finally {
        _chatPhotoLoads.remove(id);
      }
    });
  }

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) {
    final id = teamId.trim();
    if (id.isEmpty) return Future.value();
    if (_teamPhotos.containsKey(id)) return Future.value(_teamPhotos[id]);
    return _teamPhotoLoads.putIfAbsent(id, () async {
      try {
        final photo = await rust
            .getTeamPhoto(teamId: id)
            .timeout(_photoTimeout);
        final decoded = photo == null ? null : base64Decode(photo);
        _rememberPhoto(_teamPhotos, id, decoded);
        AppLog.debug(
          'Team photo',
          'teamId=$id, loaded=${decoded != null}, bytes=${decoded?.length ?? 0}',
        );
        return decoded;
      } catch (error) {
        AppLog.debug('Team photo', 'teamId=$id, error=$error');
        return null;
      } finally {
        _teamPhotoLoads.remove(id);
      }
    });
  }
}
