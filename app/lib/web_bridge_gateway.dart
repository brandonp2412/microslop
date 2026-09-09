// ignore_for_file: deprecated_member_use, avoid_web_libraries_in_flutter

import 'dart:async';
import 'dart:convert';
import 'dart:html' as html;
import 'dart:typed_data';

import 'auth_gateway.dart';
import 'call_gateway.dart';
import 'desktop_notifications.dart';
import 'teams_gateway.dart';

const _bridgeBaseUrl = String.fromEnvironment(
  'MICROSLOP_WEB_BRIDGE_URL',
  defaultValue: 'http://127.0.0.1:18444',
);
const _bridgeRequestTimeout = Duration(seconds: 45);

Future<dynamic> _post(
  String path, [
  Map<String, Object?> body = const {},
]) async {
  if (!path.startsWith('/') || path.contains('..')) {
    throw ArgumentError.value(path, 'path', 'Invalid web bridge path');
  }
  late final html.HttpRequest response;
  try {
    response = await html.HttpRequest.request(
      '$_bridgeBaseUrl$path',
      method: 'POST',
      requestHeaders: const {'Content-Type': 'application/json'},
      sendData: jsonEncode(body),
    ).timeout(_bridgeRequestTimeout);
  } on TimeoutException {
    throw TimeoutException(
      'Microslop web bridge request timed out: $path',
      _bridgeRequestTimeout,
    );
  }
  final text = response.responseText ?? '';
  dynamic decoded;
  try {
    decoded = text.trim().isEmpty ? <String, Object?>{} : jsonDecode(text);
  } on FormatException catch (error) {
    throw StateError(
      'Microslop web bridge returned invalid JSON for $path: $error',
    );
  }
  if (response.status != 200) {
    final data = decoded is Map ? decoded : const <Object?, Object?>{};
    final message = data['error']?.toString();
    throw StateError(
      message?.trim().isNotEmpty == true
          ? message!
          : 'Microslop web bridge request failed (${response.status}).',
    );
  }
  return decoded;
}

Map<String, dynamic> _map(dynamic value) {
  if (value is! Map) {
    throw StateError(
      'Microslop web bridge returned an object with the wrong shape.',
    );
  }
  final result = <String, dynamic>{};
  for (final entry in value.entries) {
    if (entry.key is! String) {
      throw StateError(
        'Microslop web bridge returned a non-string object key.',
      );
    }
    result[entry.key as String] = entry.value;
  }
  return result;
}

List<dynamic> _list(dynamic value) {
  if (value is! List) {
    throw StateError(
      'Microslop web bridge returned a list with the wrong shape.',
    );
  }
  return value;
}

String _requiredString(Map<String, dynamic> data, String key) {
  final value = data[key];
  if (value is! String || value.trim().isEmpty) {
    throw StateError('Microslop web bridge response is missing "$key".');
  }
  return value.trim();
}

String? _optionalString(Map<String, dynamic> data, String key) {
  final value = data[key];
  if (value == null) return null;
  if (value is! String) {
    throw StateError(
      'Microslop web bridge response field "$key" is not a string.',
    );
  }
  final trimmed = value.trim();
  return trimmed.isEmpty ? null : trimmed;
}

int _requiredInt(Map<String, dynamic> data, String key) {
  final value = data[key];
  if (value is! num || !value.isFinite) {
    throw StateError(
      'Microslop web bridge response field "$key" is not numeric.',
    );
  }
  return value.toInt();
}

class WebBridgeAuthGateway implements AuthGateway, MultiAccountAuthGateway {
  AuthSnapshot _snapshot(dynamic value) {
    final data = _map(value);
    return AuthSnapshot(
      signedIn: data['signedIn'] == true,
      loginInProgress: data['loginInProgress'] == true,
    );
  }

  @override
  Future<AuthSnapshot> status() async => _snapshot(await _post('/auth/status'));

  @override
  Future<AuthSnapshot> restoreWorkSession() async =>
      _snapshot(await _post('/auth/restore'));

  @override
  Future<DeviceCodeDetails> beginWorkLogin() async {
    final data = _map(await _post('/auth/begin'));
    return DeviceCodeDetails(
      verificationUri: _requiredString(data, 'verificationUri'),
      userCode: _requiredString(data, 'userCode'),
      expiresInSeconds: BigInt.from(_requiredInt(data, 'expiresInSeconds')),
    );
  }

  @override
  Future<AuthSnapshot> completeWorkLogin() async =>
      _snapshot(await _post('/auth/complete'));

  @override
  Future<void> cancelWorkLogin() async {
    await _post('/auth/cancel');
  }

  @override
  Future<AuthSnapshot> logout() async => _snapshot(await _post('/auth/logout'));

  @override
  Future<List<SavedAccountSummary>> savedAccounts() async => [
    for (final value in _list(await _post('/auth/accounts')))
      if (_map(value) case final account)
        SavedAccountSummary(
          id: _requiredString(account, 'id'),
          displayName: _requiredString(account, 'displayName'),
          username: _requiredString(account, 'username'),
        ),
  ];

  @override
  Future<String?> activeAccountId() async =>
      _optionalString(_map(await _post('/auth/active-account')), 'accountId');

  @override
  Future<AuthSnapshot> switchAccount(String accountId) async =>
      _snapshot(await _post('/auth/switch', {'accountId': accountId}));
}

class WebBridgeTeamsGateway
    implements
        ImageHistoryTeamsGateway,
        LazyImageTeamsGateway,
        ReactionUsersTeamsGateway,
        TeamsGateway,
        ResponsiveTeamsGateway,
        PresenceTeamsGateway,
        MultiAccountTeamsGateway {
  Timer? _eventTimer;
  StreamController<MessageEvent>? _eventController;
  bool _pollingEvents = false;
  bool _reportedReady = false;
  int? _eventCursor;

  @override
  Future<List<TeamSummary>> listTeams() async {
    final data = _list(await _post('/teams'));
    final teams = <TeamSummary>[];
    final seenTeams = <String>{};
    for (final value in data) {
      final team = _map(value);
      final id = _requiredString(team, 'id');
      if (!seenTeams.add(id)) continue;
      final channels = <Conversation>[];
      final seenChannels = <String>{};
      for (final value in _list(team['channels'])) {
        final channel = _map(value);
        final channelId = _requiredString(channel, 'id');
        if (!seenChannels.add(channelId)) continue;
        channels.add(
          Conversation.channel(
            id: channelId,
            name: _requiredString(channel, 'name'),
            teamId: id,
          ),
        );
      }
      teams.add(
        TeamSummary(
          id: id,
          name: _requiredString(team, 'name'),
          channels: channels,
        ),
      );
    }
    return teams;
  }

  @override
  Future<List<PresenceSummary>> getPresences(List<String> userIds) async {
    final ids = userIds
        .map((id) => id.trim())
        .where((id) => id.isNotEmpty)
        .toSet()
        .take(650)
        .toList(growable: false);
    if (ids.isEmpty) return const [];
    final data = _list(await _post('/presences', {'userIds': ids}));
    return [
      for (final value in data)
        if (value is Map)
          PresenceSummary(
            userId: _requiredString(_map(value), 'userId'),
            availability: _requiredString(_map(value), 'availability'),
            activity: _requiredString(_map(value), 'activity'),
          ),
    ];
  }

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    final data = _list(await _post('/chats', {'limit': limit.clamp(1, 500)}));
    final chats = <Conversation>[];
    final seen = <String>{};
    for (final value in data) {
      final chat = _map(value);
      final id = _requiredString(chat, 'id');
      final teamId = _optionalString(chat, 'teamId');
      if (!seen.add('${teamId ?? ''}:$id')) continue;
      final avatarUserIds = <String>[];
      final seenAvatarIds = <String>{};
      for (final value in _list(chat['memberUserIds'])) {
        if (value is! String) continue;
        final userId = value.trim();
        if (userId.isNotEmpty && seenAvatarIds.add(userId)) {
          avatarUserIds.add(userId);
        }
      }
      chats.add(
        Conversation.chat(
          id: id,
          name: _requiredString(chat, 'name'),
          isGroup: chat['isGroup'] == true,
          teamId: teamId,
          profilePhotoUserId: _optionalString(chat, 'profilePhotoUserId'),
          avatarUserIds: avatarUserIds,
          preview: chat['preview'] is String ? chat['preview'] as String : null,
        ),
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
      _readMessages(conversation, limit: 25, includeImages: true);

  @override
  Future<List<MessageSummary>> readImageHistory(Conversation conversation) =>
      _readMessages(conversation, limit: 0x7fffffff, includeImages: false);

  @override
  Future<MessageImage?> loadMessageImage(String url) async {
    final value = await _post('/messages/image/read', {'url': url});
    if (value == null) return null;
    final image = _map(value);
    return MessageImage(
      contentType: _requiredString(image, 'contentType'),
      sourceUrl: _optionalString(image, 'sourceUrl'),
      bytes: base64Decode(_requiredString(image, 'dataBase64')),
    );
  }

  final _reactionUserNames = <String, Future<String>>{};

  @override
  Future<String> reactionUserName(String userId) =>
      _reactionUserNames.putIfAbsent(userId, () async {
        try {
          return _requiredString(
            _map(await _post('/reaction/user', {'id': userId})),
            'name',
          );
        } catch (_) {
          _reactionUserNames.remove(userId);
          return userId;
        }
      });

  Future<List<MessageSummary>> _readMessages(
    Conversation conversation, {
    required int limit,
    required bool includeImages,
  }) async {
    final data = _list(
      await _post('/messages/read', {
        'kind': conversation.kind == ConversationKind.channel
            ? 'channel'
            : 'chat',
        'id': conversation.id,
        'teamId': conversation.teamId,
        'limit': limit,
        'includeImages': includeImages,
      }),
    );
    final messages = <MessageSummary>[];
    final seenMessageIds = <String>{};
    for (final value in data) {
      final message = _map(value);
      final id = _requiredString(message, 'id');
      if (!seenMessageIds.add(id)) continue;
      final images = <MessageImage>[];
      for (final value in _list(message['images'])) {
        final image = _map(value);
        final contentType = _requiredString(image, 'contentType').toLowerCase();
        if (!contentType.startsWith('image/')) continue;
        try {
          final bytes = base64Decode(_requiredString(image, 'dataBase64'));
          if (bytes.isNotEmpty) {
            images.add(
              MessageImage(
                contentType: contentType,
                bytes: bytes,
                sourceUrl: _optionalString(image, 'sourceUrl'),
              ),
            );
          }
        } on FormatException {
          continue;
        }
      }
      final reactionCounts = <String, (int, bool)>{};
      for (final value in _list(message['reactions'])) {
        final reaction = _map(value);
        final type = _requiredString(reaction, 'type');
        final count = _requiredInt(reaction, 'count');
        if (count <= 0) continue;
        final existing = reactionCounts[type];
        reactionCounts[type] = (
          (existing?.$1 ?? 0) + count,
          (existing?.$2 ?? false) || reaction['selected'] == true,
        );
      }
      messages.add(
        MessageSummary(
          id: id,
          sender: _optionalString(message, 'sender') ?? '?',
          senderId: _optionalString(message, 'senderId'),
          isFromCurrentUser: message['isFromCurrentUser'] == true,
          timestamp: message['timestamp'] is String
              ? message['timestamp'] as String
              : '',
          content: message['content'] is String
              ? message['content'] as String
              : '',
          quotes: [
            for (final value in _list(message['quotes']))
              if (_map(value) case final quote)
                MessageQuote(
                  messageId: _optionalString(quote, 'messageId'),
                  sender: _optionalString(quote, 'sender') ?? '',
                  content: _optionalString(quote, 'content') ?? '',
                ),
          ],
          images: images,
          imageUrls: _list(message['imageUrls']).whereType<String>().toList(),
          reactions: [
            for (final reaction in reactionCounts.entries)
              MessageReaction(
                type: reaction.key,
                count: reaction.value.$1,
                selected: reaction.value.$2,
                users: [
                  for (final value in _list(message['reactions']))
                    if (_map(value)['type'] == reaction.key)
                      for (final user in _list(_map(value)['users']))
                        ReactionUser(
                          id: _optionalString(_map(user), 'id') ?? '',
                          name: _optionalString(_map(user), 'name') ?? '',
                        ),
                ],
              ),
          ],
        ),
      );
    }
    return messages;
  }

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    final response = _map(
      await _post('/saved-account/notification', {
        'accountId': accountId,
        'conversationId': conversationId,
        'messageLimit': recentMessageLimit.clamp(1, 25),
      }),
    );
    final rawNotification = response['notification'];
    if (rawNotification == null) return null;
    final notification = _map(rawNotification);

    MessageSummary parseMessage(dynamic value) {
      final message = _map(value);
      final reactionCounts = <String, (int, bool)>{};
      for (final value in _list(message['reactions'])) {
        final reaction = _map(value);
        final type = _requiredString(reaction, 'type');
        final count = _requiredInt(reaction, 'count');
        if (count <= 0) continue;
        final existing = reactionCounts[type];
        reactionCounts[type] = (
          (existing?.$1 ?? 0) + count,
          (existing?.$2 ?? false) || reaction['selected'] == true,
        );
      }
      return MessageSummary(
        id: _requiredString(message, 'id'),
        sender: _optionalString(message, 'sender') ?? '?',
        senderId: _optionalString(message, 'senderId'),
        isFromCurrentUser: message['isFromCurrentUser'] == true,
        timestamp: message['timestamp'] is String
            ? message['timestamp'] as String
            : '',
        content: message['content'] is String
            ? message['content'] as String
            : '',
        reactions: [
          for (final reaction in reactionCounts.entries)
            MessageReaction(
              type: reaction.key,
              count: reaction.value.$1,
              selected: reaction.value.$2,
              users: [
                for (final value in _list(message['reactions']))
                  if (_map(value)['type'] == reaction.key)
                    for (final user in _list(_map(value)['users']))
                      ReactionUser(
                        id: _optionalString(_map(user), 'id') ?? '',
                        name: _optionalString(_map(user), 'name') ?? '',
                      ),
              ],
            ),
        ],
      );
    }

    return SavedAccountNotificationSummary(
      accountId: _requiredString(notification, 'accountId'),
      accountName: _requiredString(notification, 'accountName'),
      conversationId: _requiredString(notification, 'conversationId'),
      conversationName: _requiredString(notification, 'conversationName'),
      isGroup: notification['isGroup'] == true,
      isChannel: notification['isChannel'] == true,
      message: parseMessage(notification['message']),
      messages: [
        for (final value in _list(notification['messages']))
          parseMessage(value),
      ],
    );
  }

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    final id = conversation.id.trim();
    if (id.isEmpty || content.trim().isEmpty) return;
    await _post('/messages/send', {
      'id': id,
      'teamId': conversation.kind == ConversationKind.channel
          ? _requiredTeamId(conversation)
          : null,
      'content': content,
    });
  }

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {
    final id = conversation.id.trim();
    final normalizedType = contentType.trim().toLowerCase();
    if (id.isEmpty || bytes.isEmpty) return;
    if (!normalizedType.startsWith('image/')) {
      throw ArgumentError.value(contentType, 'contentType');
    }
    await _post('/messages/image', {
      'id': id,
      'teamId': conversation.kind == ConversationKind.channel
          ? _requiredTeamId(conversation)
          : null,
      'caption': caption,
      'contentType': normalizedType,
      'dataBase64': base64Encode(bytes),
    });
  }

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    final id = conversation.id.trim();
    final messageId = message.id.trim();
    final type = validateReactionType(reactionType);
    if (id.isEmpty || messageId.isEmpty) {
      throw ArgumentError(
        'Reaction requires conversation, message, and reaction ids.',
      );
    }
    await _post('/reaction', {
      'id': id,
      'teamId': conversation.kind == ConversationKind.channel
          ? _requiredTeamId(conversation)
          : null,
      'messageId': messageId,
      'reactionType': type,
      'remove': message.reactions.any(
        (reaction) => reaction.type == type && reaction.selected,
      ),
    });
  }

  String _requiredTeamId(Conversation conversation) {
    final teamId = conversation.teamId?.trim();
    if (teamId == null || teamId.isEmpty) {
      throw StateError('Channel ${conversation.id} does not have a team id.');
    }
    return teamId;
  }

  @override
  Future<UserSummary> getUser() async {
    final data = _map(await _post('/user'));
    return UserSummary(
      id: _optionalString(data, 'id'),
      displayName: _requiredString(data, 'displayName'),
      email: _optionalString(data, 'email'),
    );
  }

  Future<Uint8List?> _photo(String path, String key, String value) async {
    final id = value.trim();
    if (id.isEmpty) return null;
    final data = _map(await _post(path, {key: id}));
    final encoded = _optionalString(data, 'dataBase64');
    if (encoded == null) return null;
    try {
      final bytes = base64Decode(encoded);
      return bytes.isEmpty ? null : bytes;
    } on FormatException {
      return null;
    }
  }

  @override
  Future<Uint8List?> getProfilePhoto(String userId) =>
      _photo('/photo/profile', 'userId', userId);

  @override
  Future<Uint8List?> getChatPhoto(String chatId) =>
      _photo('/photo/chat', 'chatId', chatId);

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) =>
      _photo('/photo/team', 'teamId', teamId);

  @override
  Stream<MessageEvent> messageEvents() {
    final existing = _eventController;
    if (existing != null) return existing.stream;

    final controller = StreamController<MessageEvent>.broadcast();
    _eventController = controller;
    _eventTimer = Timer.periodic(
      const Duration(seconds: 1),
      (_) => unawaited(_pollMessageEvents()),
    );
    unawaited(_pollMessageEvents());
    return controller.stream;
  }

  Future<void> _pollMessageEvents() async {
    if (_pollingEvents) return;
    _pollingEvents = true;
    try {
      final data = _map(
        await _post('/events/messages/poll', {
          if (_eventCursor != null) 'cursor': _eventCursor,
        }),
      );
      final controller = _eventController;
      if (controller == null || controller.isClosed) return;
      _eventCursor = _requiredInt(data, 'cursor');
      if ((data['ready'] == true && !_reportedReady) || data['lost'] == true) {
        _reportedReady = true;
        controller.add(const MessageEvent(conversationId: ''));
      }
      for (final value in _list(data['events'])) {
        final event = _map(value);
        controller.add(
          MessageEvent(
            conversationId: event['conversationId'] as String?,
            accountId: event['accountId'] as String?,
            accountActive: event['accountActive'] != false,
            resourceType: event['resourceType'] as String? ?? '',
          ),
        );
      }
    } catch (error, stackTrace) {
      _eventController?.addError(error, stackTrace);
    } finally {
      _pollingEvents = false;
    }
  }

  @override
  Future<void> stopMessageEvents() async {
    _eventTimer?.cancel();
    _eventTimer = null;
    _reportedReady = false;
    _eventCursor = null;
    final controller = _eventController;
    _eventController = null;
    if (controller != null && !controller.isClosed) await controller.close();
  }
}

class WebBridgeCallGateway implements CallGateway {
  Timer? _eventTimer;
  StreamController<CallUpdate>? _eventController;
  bool _pollingEvents = false;
  int? _eventCursor;

  @override
  Stream<CallUpdate> events() {
    final existing = _eventController;
    if (existing != null) return existing.stream;
    final controller = StreamController<CallUpdate>.broadcast();
    _eventController = controller;
    _eventTimer = Timer.periodic(
      const Duration(milliseconds: 250),
      (_) => unawaited(_pollCallEvents()),
    );
    unawaited(_pollCallEvents());
    return controller.stream;
  }

  Future<void> _pollCallEvents() async {
    if (_pollingEvents) return;
    _pollingEvents = true;
    try {
      final data = _map(
        await _post('/events/calls/poll', {
          if (_eventCursor != null) 'cursor': _eventCursor,
        }),
      );
      final controller = _eventController;
      if (controller == null || controller.isClosed) return;
      _eventCursor = _requiredInt(data, 'cursor');
      if (data['lost'] == true) {
        controller.addError(StateError('Call event history was truncated.'));
      }
      for (final value in _list(data['events'])) {
        final event = _map(value);
        final kind = switch (event['kind']) {
          'incoming' => CallUpdateKind.incoming,
          'dialing' => CallUpdateKind.dialing,
          'ringing' => CallUpdateKind.ringing,
          'connected' => CallUpdateKind.connected,
          'ended' => CallUpdateKind.ended,
          _ => CallUpdateKind.error,
        };
        controller.add(
          CallUpdate(
            kind: kind,
            callId: event['callId'] as String,
            conversationId: event['conversationId'] as String?,
            displayName: event['displayName'] as String?,
            detail: event['detail'] as String?,
          ),
        );
      }
    } catch (error, stackTrace) {
      _eventController?.addError(error, stackTrace);
    } finally {
      _pollingEvents = false;
    }
  }

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) async {
    await _post('/call/start', {
      'conversationId': conversationId,
      'video': video,
      if (calleeUserId != null) 'calleeUserId': calleeUserId,
    });
  }

  @override
  Future<void> hangUp() async {
    await _post('/call/hangup');
  }

  @override
  Future<void> acceptCall(String callId) async {
    await _post('/call/accept', {'callId': callId});
  }

  @override
  Future<void> declineCall(String callId) async {
    await _post('/call/decline', {'callId': callId});
  }

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {
    await _post('/call/microphone', {'enabled': enabled});
  }

  @override
  Future<void> setSpeakerEnabled(bool enabled) async {
    await _post('/call/speaker', {'enabled': enabled});
  }

  @override
  Future<void> stopEvents() async {
    _eventTimer?.cancel();
    _eventTimer = null;
    _eventCursor = null;
    final controller = _eventController;
    _eventController = null;
    if (controller != null && !controller.isClosed) await controller.close();
  }
}

class WebBridgeNotifications implements DesktopNotifications {
  @override
  Stream<String> get conversationSelections => const Stream.empty();

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {
    NotificationHistory.add(title, body);
    if (!html.Notification.supported) return;
    if (html.Notification.permission != 'granted') {
      await html.Notification.requestPermission();
    }
    if (html.Notification.permission == 'granted') {
      html.Notification(title, body: body);
    }
  }
}
