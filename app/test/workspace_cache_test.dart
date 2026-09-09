import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/workspace_cache.dart';

void main() {
  test('round trips cached workspace data including channels and images', () {
    final data = CachedWorkspace(
      chats: const [
        CachedConversation(
          id: 'self',
          name: 'brandonp2412',
          isGroup: false,
          profilePhotoUserId: 'u1',
          memberUserIds: ['u1'],
          preview: 'hello',
        ),
      ],
      teams: const [
        CachedTeam(
          id: 'team-1',
          name: 'Engineering',
          channels: [
            CachedConversation(
              id: 'channel-1',
              name: 'General',
              isGroup: false,
              teamId: 'team-1',
            ),
          ],
        ),
      ],
      selectedChatId: 'channel-1',
      messages: {
        'channel-1': [
          CachedMessage(
            id: 'm1',
            sender: 'Ada',
            isFromCurrentUser: false,
            timestamp: 'now',
            content: 'hello',
            quotes: const [
              CachedQuote(
                messageId: 'original-1',
                sender: 'Grace',
                content: 'quoted message',
              ),
            ],
            images: [
              CachedImage(
                contentType: 'image/png',
                bytes: Uint8List.fromList([1, 2, 3]),
              ),
            ],
          ),
        ],
      },
      hiddenSectionIds: const {'groups'},
    );

    final restored = CachedWorkspace.fromJson(data.toJson());
    expect(restored, data);
    expect(restored.schemaVersion, cachedWorkspaceSchemaVersion);
    expect(restored.teams.single.channels.single.teamId, 'team-1');
    expect(restored.messages['channel-1']!.single.images.single.bytes, [
      1,
      2,
      3,
    ]);
    expect(
      restored.messages['channel-1']!.single.quotes.single.content,
      'quoted message',
    );
  });

  test('treats absent schema version as a legacy cache', () {
    final restored = CachedWorkspace.fromJson(const {
      'chats': <Object>[],
      'teams': <Object>[],
      'messages': <String, Object>{},
    });
    expect(restored.schemaVersion, 0);
  });

  test('tolerates malformed conversation scalar fields', () {
    final conversation = CachedConversation.fromJson({
      'id': 42,
      'name': false,
      'isGroup': 'yes',
      'teamId': 5,
      'profilePhotoUserId': <Object>[],
      'preview': 99,
    });
    expect(conversation.id, '');
    expect(conversation.name, 'Unknown chat');
    expect(conversation.isGroup, isFalse);
    expect(conversation.teamId, isNull);
    expect(conversation.profilePhotoUserId, isNull);
    expect(conversation.preview, isNull);
  });

  test('normalizes cached member ids', () {
    final conversation = CachedConversation.fromJson({
      'id': 'chat',
      'name': 'Chat',
      'isGroup': false,
      'memberUserIds': [' u1 ', null, 7, '', 'u1', 'u2'],
    });
    expect(conversation.memberUserIds, ['u1', 'u2']);
  });

  test('conversation equality distinguishes different teams', () {
    const first = CachedConversation(
      id: 'general',
      name: 'General',
      isGroup: false,
      teamId: 'team-a',
    );
    const second = CachedConversation(
      id: 'general',
      name: 'General',
      isGroup: false,
      teamId: 'team-b',
    );
    expect(first, isNot(second));
    expect(first.hashCode, isNot(second.hashCode));
  });

  test('repairs channel team ids and removes duplicate channels', () {
    final team = CachedTeam.fromJson({
      'id': ' team ',
      'name': 'Engineering',
      'channels': [
        {
          'id': 'general',
          'name': 'General',
          'isGroup': true,
          'teamId': 'wrong',
        },
        {'id': 'general', 'name': 'Duplicate', 'isGroup': false},
        {'id': '', 'name': 'Empty', 'isGroup': false},
        42,
      ],
    });
    expect(team.id, 'team');
    expect(team.channels, hasLength(1));
    expect(team.channels.single.teamId, 'team');
    expect(team.channels.single.isGroup, isFalse);
  });

  test('skips malformed and duplicate chats and teams', () {
    final workspace = CachedWorkspace.fromJson({
      'chats': [
        5,
        {'id': '', 'name': 'Empty', 'isGroup': false},
        {'id': 'chat', 'name': 'One', 'isGroup': false},
        {'id': 'chat', 'name': 'Two', 'isGroup': false},
      ],
      'teams': [
        null,
        {'id': '', 'name': 'Empty', 'channels': <Object>[]},
        {'id': 'team', 'name': 'One', 'channels': <Object>[]},
        {'id': 'team', 'name': 'Two', 'channels': <Object>[]},
      ],
      'messages': <String, Object>{},
    });
    expect(workspace.chats.map((chat) => chat.name), ['One']);
    expect(workspace.teams.map((team) => team.name), ['One']);
  });

  test('normalizes selected and hidden ids without crashing', () {
    final workspace = CachedWorkspace.fromJson({
      'chats': <Object>[],
      'teams': <Object>[],
      'selectedChatId': 123,
      'messages': <String, Object>{},
      'hiddenConversationIds': [' a ', null, 5, '', 'a', 'b'],
      'hiddenSectionIds': [' groups ', false, '', 'groups', 'channels'],
    });
    expect(workspace.selectedChatId, isNull);
    expect(workspace.hiddenConversationIds, ['a', 'b']);
    expect(workspace.hiddenSectionIds, {'groups', 'channels'});
  });

  test('skips malformed message maps and message entries', () {
    final workspace = CachedWorkspace.fromJson({
      'chats': <Object>[],
      'teams': <Object>[],
      'messages': {
        5: <Object>[],
        '': <Object>[],
        'wrong-list': 'not a list',
        'good': [
          null,
          42,
          {'id': 'm1', 'sender': 'Ada'},
        ],
      },
    });
    expect(workspace.messages.keys, ['good']);
    expect(workspace.messages['good'], hasLength(1));
    expect(workspace.messages['good']!.single.id, 'm1');
  });

  test('tolerates malformed cached message scalar fields', () {
    final message = CachedMessage.fromJson({
      'id': 3,
      'sender': false,
      'senderId': 1,
      'isFromCurrentUser': 'yes',
      'timestamp': 2,
      'content': <Object>[],
    });
    expect(message.id, '');
    expect(message.sender, '?');
    expect(message.senderId, isNull);
    expect(message.isFromCurrentUser, isFalse);
    expect(message.timestamp, '');
    expect(message.content, '');
  });

  test('drops corrupt cached images while preserving valid images', () {
    final message = CachedMessage.fromJson({
      'id': 'm1',
      'sender': 'Ada',
      'images': [
        null,
        {'contentType': '', 'dataBase64': 'AQI='},
        {'contentType': 'image/png', 'dataBase64': 'not-base64!'},
        {'contentType': 'image/png', 'dataBase64': ''},
        {'contentType': 'image/png', 'dataBase64': 'AQI='},
      ],
    });
    expect(message.images, hasLength(1));
    expect(message.images.single.bytes, [1, 2]);
  });

  test('normalizes, merges, and bounds cached reactions', () {
    final message = CachedMessage.fromJson({
      'id': 'm1',
      'sender': 'Ada',
      'reactions': [
        null,
        {'type': '', 'count': 2},
        {'type': 'like', 'count': -1},
        {'type': 'like', 'count': 2},
        {'type': ' like ', 'count': 3, 'selected': true},
        {'type': 'heart', 'count': 1 << 40},
        {'type': 'laugh', 'count': 'many'},
      ],
    });
    expect(message.reactions.map((reaction) => reaction.type), [
      'like',
      'heart',
    ]);
    expect(message.reactions.first.count, 5);
    expect(message.reactions.first.selected, isTrue);
    expect(message.reactions.last.count, 1 << 30);
  });

  test('prioritizes direct messages before groups and channels', () {
    const direct = CachedConversation(
      id: 'direct',
      name: 'Direct',
      isGroup: false,
    );
    const group = CachedConversation(id: 'group', name: 'Group', isGroup: true);
    const channel = CachedConversation(
      id: 'channel',
      name: 'Channel',
      isGroup: false,
      teamId: 'team',
    );

    expect(
      prefetchConversationIds(
        directMessages: const [direct],
        groups: const [group],
        channels: const [channel],
        limit: 3,
      ),
      ['direct', 'group', 'channel'],
    );
  });

  test('prefetch ids ignore duplicates, blanks, and nonpositive limits', () {
    const direct = CachedConversation(
      id: 'same',
      name: 'Direct',
      isGroup: false,
    );
    const duplicate = CachedConversation(
      id: 'same',
      name: 'Group',
      isGroup: true,
    );
    const blank = CachedConversation(id: '  ', name: 'Blank', isGroup: true);
    const channel = CachedConversation(
      id: 'channel',
      name: 'Channel',
      isGroup: false,
      teamId: 'team',
    );
    expect(
      prefetchConversationIds(
        directMessages: const [direct],
        groups: const [duplicate, blank],
        channels: const [channel],
      ),
      ['same', 'channel'],
    );
    expect(
      prefetchConversationIds(
        directMessages: const [direct],
        groups: const [],
        channels: const [],
        limit: 0,
      ),
      isEmpty,
    );
    expect(
      prefetchConversationIds(
        directMessages: const [direct],
        groups: const [],
        channels: const [],
        limit: -2,
      ),
      isEmpty,
    );
  });

  test('recognizes only a non-channel self chat with a nonblank user id', () {
    const chat = CachedConversation(
      id: 'self',
      name: 'brandonp2412',
      isGroup: false,
      memberUserIds: ['u1'],
    );
    const channel = CachedConversation(
      id: 'channel',
      name: 'General',
      isGroup: false,
      teamId: 'team',
      memberUserIds: ['u1'],
    );
    expect(isSelfChat(chat, 'u1'), isTrue);
    expect(isSelfChat(chat, '  '), isFalse);
    expect(isSelfChat(chat, null), isFalse);
    expect(isSelfChat(channel, 'u1'), isFalse);
  });

  test('round trips historical message reactions', () {
    const message = CachedMessage(
      id: 'm1',
      sender: 'Ada',
      isFromCurrentUser: false,
      timestamp: 'now',
      content: 'Hello',
      reactions: [CachedReaction(type: 'like', count: 3, selected: true)],
    );

    final restored = CachedMessage.fromJson(message.toJson());
    expect(restored.reactions.single.type, 'like');
    expect(restored.reactions.single.count, 3);
    expect(restored.reactions.single.selected, isTrue);
  });
}
