import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/desktop_notifications.dart';
import 'package:microslop/main.dart';
import 'package:microslop/teams_gateway.dart';

import '../test/widget_test.dart' as fixtures;

const _bridge = 'http://127.0.0.1:18444';
const _origin = 'http://127.0.0.1:18443';
const _selfChatName = 'brandonp2412';

Map<String, dynamic> _map(dynamic value) =>
    Map<String, dynamic>.from(value as Map);
List<dynamic> _list(dynamic value) => value as List<dynamic>;
String _requiredString(Map<String, dynamic> value, String key) =>
    (value[key] as String).trim();
String? _optionalString(Map<String, dynamic> value, String key) {
  final raw = value[key];
  if (raw is! String || raw.trim().isEmpty) return null;
  return raw.trim();
}

class _BridgeTeamsGateway
    implements TeamsGateway, ResponsiveTeamsGateway, LazyImageTeamsGateway {
  final _client = HttpClient();

  Future<dynamic> _post(
    String path, [
    Map<String, Object?> body = const {},
  ]) async {
    final request = await _client.postUrl(Uri.parse('$_bridge$path'));
    final payload = utf8.encode(jsonEncode(body));
    request.headers.contentType = ContentType.json;
    request.headers.set('Origin', _origin);
    request.contentLength = payload.length;
    request.add(payload);
    final response = await request.close().timeout(const Duration(seconds: 90));
    final text = await utf8.decoder.bind(response).join();
    if (response.statusCode != 200) {
      throw StateError('Bridge $path failed (${response.statusCode}): $text');
    }
    return text.isEmpty ? <String, Object?>{} : jsonDecode(text);
  }

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    final data = _list(await _post('/chats', {'limit': limit.clamp(1, 500)}));
    return [
      for (final raw in data)
        if (_map(raw) case final chat)
          Conversation.chat(
            id: _requiredString(chat, 'id'),
            name: _requiredString(chat, 'name'),
            isGroup: chat['isGroup'] == true,
            teamId: _optionalString(chat, 'teamId'),
            profilePhotoUserId: _optionalString(chat, 'profilePhotoUserId'),
            avatarUserIds: _list(chat['memberUserIds'])
                .whereType<String>()
                .where((id) => id.trim().isNotEmpty)
                .toList(growable: false),
            preview: chat['preview'] is String
                ? chat['preview'] as String
                : null,
          ),
    ];
  }

  @override
  Future<List<TeamSummary>> listTeams() async {
    final data = _list(await _post('/teams'));
    return [
      for (final raw in data)
        if (_map(raw) case final team)
          TeamSummary(
            id: _requiredString(team, 'id'),
            name: _requiredString(team, 'name'),
            channels: [
              for (final rawChannel in _list(team['channels']))
                if (_map(rawChannel) case final channel)
                  Conversation.channel(
                    id: _requiredString(channel, 'id'),
                    name: _requiredString(channel, 'name'),
                    teamId: _requiredString(team, 'id'),
                  ),
            ],
          ),
    ];
  }

  Future<List<MessageSummary>> _read(
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
    return [
      for (final raw in data)
        if (_map(raw) case final message)
          MessageSummary(
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
            quotes: [
              for (final rawQuote in _list(message['quotes']))
                if (_map(rawQuote) case final quote)
                  MessageQuote(
                    messageId: _optionalString(quote, 'messageId'),
                    sender: _optionalString(quote, 'sender') ?? '',
                    content: _optionalString(quote, 'content') ?? '',
                  ),
            ],
            images: [
              for (final rawImage in _list(message['images']))
                if (_map(rawImage) case final image)
                  if (_optionalString(image, 'dataBase64') case final encoded?)
                    MessageImage(
                      contentType: _requiredString(image, 'contentType'),
                      bytes: base64Decode(encoded),
                      sourceUrl: _optionalString(image, 'sourceUrl'),
                    ),
            ],
            imageUrls: _list(message['imageUrls']).whereType<String>().toList(),
            reactions: [
              for (final rawReaction in _list(message['reactions']))
                if (_map(rawReaction) case final reaction)
                  if ((reaction['count'] as num?)?.toInt() case final count?
                      when count > 0)
                    MessageReaction(
                      type: _requiredString(reaction, 'type'),
                      count: count,
                      selected: reaction['selected'] == true,
                      users: [
                        for (final rawUser in _list(reaction['users']))
                          if (_map(rawUser) case final user)
                            ReactionUser(
                              id: _optionalString(user, 'id') ?? '',
                              name: _optionalString(user, 'name') ?? '',
                            ),
                      ],
                    ),
            ],
          ),
    ];
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) =>
      _read(conversation, limit: 100, includeImages: true);

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) =>
      _read(conversation, limit: 100, includeImages: false);

  @override
  Future<List<MessageSummary>> hydrateRecentImages(Conversation conversation) =>
      _read(conversation, limit: 25, includeImages: true);

  @override
  Future<MessageImage?> loadMessageImage(String url) async {
    final raw = await _post('/messages/image/read', {'url': url});
    if (raw == null) return null;
    final image = _map(raw);
    return MessageImage(
      contentType: _requiredString(image, 'contentType'),
      bytes: base64Decode(_requiredString(image, 'dataBase64')),
      sourceUrl: _optionalString(image, 'sourceUrl'),
    );
  }

  @override
  Future<UserSummary> getUser() async {
    final user = _map(await _post('/user'));
    return UserSummary(
      id: _optionalString(user, 'id'),
      displayName: _requiredString(user, 'displayName'),
      email: _optionalString(user, 'email'),
    );
  }

  Future<Uint8List?> _photo(String path, String key, String value) async {
    final response = _map(await _post(path, {key: value}));
    final encoded = _optionalString(response, 'dataBase64');
    return encoded == null ? null : base64Decode(encoded);
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
  Stream<MessageEvent> messageEvents() => const Stream.empty();

  @override
  Future<void> stopMessageEvents() async {}

  @override
  Future<void> sendMessage(Conversation conversation, String content) =>
      Future.error(UnsupportedError('Read-only live CLS gateway'));

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) => Future.error(UnsupportedError('Read-only live CLS gateway'));

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) => Future.error(UnsupportedError('Read-only live CLS gateway'));
}

Finder _messageBubbles() => find.byWidgetPredicate(
  (widget) =>
      widget.key is ValueKey<String> &&
      (widget.key! as ValueKey<String>).value.startsWith('message-bubble-'),
);

Map<String, double> _visibleBubbleTops(WidgetTester tester) {
  final screen = tester.getRect(find.byType(Scaffold).first);
  return {
    for (final element in _messageBubbles().evaluate())
      if (tester.getRect(find.byWidget(element.widget)).bottom > screen.top &&
          tester.getRect(find.byWidget(element.widget)).top < screen.bottom)
        (element.widget.key! as ValueKey<String>).value: tester
            .getTopLeft(find.byWidget(element.widget))
            .dy,
  };
}

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (!condition() && DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 100));
  }
  expect(condition(), isTrue);
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('real Waydroid history scroll records visual CLS', (
    tester,
  ) async {
    await binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => binding.setSurfaceSize(null));

    final gateway = _BridgeTeamsGateway();
    final chats = await gateway.listChats(limit: 100);
    final self = chats.singleWhere(
      (chat) => !chat.isGroup && chat.name.trim() == _selfChatName,
    );

    await tester.pumpWidget(
      OstApp(
        gateway: fixtures.RestoringAuthGateway(),
        teamsGateway: gateway,
        callGateway: fixtures.FakeCallGateway(),
        notifications: const NoopDesktopNotifications(),
      ),
    );
    await _pumpUntil(
      tester,
      () => find.byTooltip('Open navigation').evaluate().isNotEmpty,
    );
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    final search = find.byWidgetPredicate(
      (widget) =>
          widget is TextField && widget.decoration?.hintText == 'Search...',
    );
    await tester.enterText(search, _selfChatName);
    await tester.pump(const Duration(milliseconds: 300));
    final tile = find.byKey(ValueKey('conversation-tile-chat-${self.id}'));
    await _pumpUntil(tester, () => tile.evaluate().isNotEmpty);
    await tester.tap(tile);
    await _pumpUntil(tester, () => _messageBubbles().evaluate().isNotEmpty);
    await tester.pump(const Duration(seconds: 2));

    final messageList = find.ancestor(
      of: _messageBubbles().first,
      matching: find.byType(ListView),
    );
    expect(messageList, findsOneWidget);

    var maxDragDeviation = 0.0;
    var maxSettleShift = 0.0;
    for (var page = 0; page < 10; page++) {
      final gesture = await tester.startGesture(tester.getCenter(messageList));
      double? previousDelta;
      for (var frame = 0; frame < 30; frame++) {
        final before = _visibleBubbleTops(tester);
        await gesture.moveBy(const Offset(0, 10));
        await tester.pump(const Duration(milliseconds: 16));
        final after = _visibleBubbleTops(tester);
        final deltas = <double>[
          for (final entry in before.entries)
            if (after[entry.key] case final next?) next - entry.value,
        ];
        if (deltas.isNotEmpty) {
          deltas.sort();
          final median = deltas[deltas.length ~/ 2];
          if (previousDelta != null) {
            maxDragDeviation = maxDragDeviation > (median - previousDelta).abs()
                ? maxDragDeviation
                : (median - previousDelta).abs();
          }
          previousDelta = median;
        }
      }
      await gesture.up();
      await tester.pump();
      final beforeSettle = _visibleBubbleTops(tester);
      await tester.pump(const Duration(milliseconds: 700));
      final afterSettle = _visibleBubbleTops(tester);
      for (final entry in beforeSettle.entries) {
        final next = afterSettle[entry.key];
        if (next == null) continue;
        final shift = (next - entry.value).abs();
        if (shift > maxSettleShift) maxSettleShift = shift;
      }
    }

    debugPrint('WAYDROID_LIVE_CLS_DRAG_DEVIATION=$maxDragDeviation');
    debugPrint('WAYDROID_LIVE_CLS_SETTLE_SHIFT=$maxSettleShift');
    await tester.pump(const Duration(seconds: 2));
  });
}
