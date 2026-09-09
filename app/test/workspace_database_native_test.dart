import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:microslop/workspace_database_native.dart';
import 'package:sqlite3/sqlite3.dart';

void main() {
  test(
    'cached image count survives messages without stored image URLs',
    () async {
      final database = WorkspaceDatabase(inMemory: true);
      addTearDown(database.close);
      const conversation = Conversation.chat(
        id: 'legacy-self',
        name: 'You',
        isGroup: false,
      );
      final message = MessageSummary(
        id: 'legacy-image',
        sender: 'You',
        timestamp: 'now',
        content: '',
        images: [
          MessageImage(
            contentType: 'image/png',
            bytes: Uint8List.fromList([1, 2, 3]),
          ),
        ],
      );

      await database.saveMessages(conversation, [message]);
      final cached = (await database.loadMessages(conversation)).single;

      expect(cached.imageUrls, isEmpty);
      expect(cached.images, isEmpty);
      expect(cached.cachedImageCount, 1);
    },
  );

  test(
    'large history persistence leaves the UI event loop responsive',
    () async {
      final database = WorkspaceDatabase(inMemory: true);
      addTearDown(database.close);
      await database.initialize();
      const conversation = Conversation.chat(
        id: 'self',
        name: 'You',
        isGroup: false,
      );
      final messages = [
        for (var index = 0; index < 10000; index++)
          MessageSummary(
            id: '$index',
            sender: 'You',
            timestamp: 'now',
            content: 'History $index',
          ),
      ];
      var ticks = 0;
      final timer = Timer.periodic(
        const Duration(milliseconds: 1),
        (_) => ticks++,
      );
      try {
        await database.saveMessages(conversation, messages);
        expect(ticks, greaterThan(0));
        ticks = 0;
        expect(await database.loadMessages(conversation), hasLength(10000));
        expect(ticks, greaterThan(0));
      } finally {
        timer.cancel();
      }
    },
  );

  test('failed message replacement rolls back messages and images', () async {
    final database = WorkspaceDatabase(inMemory: true);
    addTearDown(database.close);
    const conversation = Conversation.chat(
      id: 'self',
      name: 'You',
      isGroup: false,
    );
    final message = MessageSummary(
      id: 'message',
      sender: 'You',
      timestamp: 'now',
      content: 'original',
      imageUrls: const ['image-source'],
      reactions: const [
        MessageReaction(
          type: 'like',
          count: 1,
          users: [ReactionUser(id: 'you', name: 'You')],
        ),
      ],
      images: [
        MessageImage(
          contentType: 'image/png',
          bytes: Uint8List.fromList([1, 2, 3]),
        ),
      ],
    );
    await database.saveMessages(conversation, [message]);
    const duplicate = MessageSummary(
      id: 'duplicate',
      sender: 'You',
      timestamp: 'now',
      content: 'replacement',
    );
    await expectLater(
      database.saveMessages(conversation, [duplicate, duplicate]),
      throwsA(isA<SqliteException>()),
    );
    expect(
      (await database.loadMessages(conversation)).single.content,
      'original',
    );
    final cached = (await database.loadMessages(conversation)).single;
    expect(cached.imageUrls, ['image-source']);
    expect(cached.reactions.single.users.single.name, 'You');
    expect((await database.loadImages(conversation, message.id)).single.bytes, [
      1,
      2,
      3,
    ]);
    await database.saveMessages(conversation, [message], replaceImages: false);
    expect((await database.loadImages(conversation, message.id)).single.bytes, [
      1,
      2,
      3,
    ]);
    final updated = MessageSummary(
      id: message.id,
      sender: 'You',
      timestamp: 'now',
      content: 'original',
      images: [
        MessageImage(
          contentType: 'image/png',
          bytes: Uint8List.fromList([4, 5]),
          sourceUrl: 'updated-source',
        ),
      ],
    );
    await database.saveImages(conversation, [updated]);
    final updatedImage = (await database.loadImages(
      conversation,
      message.id,
    )).single;
    expect(updatedImage.bytes, [4, 5]);
    expect(updatedImage.sourceUrl, 'updated-source');
    await database.saveMessages(conversation, []);
    expect(await database.loadImages(conversation, message.id), isEmpty);
  });
}
