import 'dart:convert';
import 'dart:isolate';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:path_provider/path_provider.dart';
import 'package:sqlite3/sqlite3.dart';

import 'teams_gateway.dart';

class WorkspaceDatabase {
  WorkspaceDatabase({String namespace = '', bool inMemory = false})
    : _namespace = namespace.trim(),
      _inMemory = inMemory;

  static const _baseKeyName = 'workspace.database.key.v1';
  static const _baseDatabaseName = 'workspace-cache.db';

  final String _namespace;
  final bool _inMemory;
  final FlutterSecureStorage _secureStorage = const FlutterSecureStorage();
  Future<SendPort>? _worker;

  String get _namespaceToken =>
      base64Url.encode(utf8.encode(_namespace)).replaceAll('=', '');

  String get _keyName =>
      _namespace.isEmpty ? _baseKeyName : '$_baseKeyName.$_namespaceToken';

  String get _databaseName => _namespace.isEmpty
      ? _baseDatabaseName
      : 'workspace-cache-$_namespaceToken.db';

  Future<SendPort> _startWorker() async {
    final path = _inMemory
        ? null
        : '${(await getApplicationSupportDirectory()).path}/$_databaseName';
    final key = _inMemory ? null : await _databaseKey();
    final ready = ReceivePort();
    try {
      await Isolate.spawn(_serveDatabase, (ready.sendPort, path, key));
      return await ready.first as SendPort;
    } finally {
      ready.close();
    }
  }

  Future<T> _run<T>(Future<T> Function(_WorkspaceDatabase) operation) async {
    final pending = _worker ??= _startWorker();
    final SendPort worker;
    try {
      worker = await pending;
    } catch (_) {
      if (identical(_worker, pending)) _worker = null;
      rethrow;
    }
    final reply = ReceivePort();
    try {
      worker.send((reply.sendPort, operation));
      final result = await reply.first as List<Object?>;
      if (result.length == 2) {
        Error.throwWithStackTrace(result[0]!, result[1]! as StackTrace);
      }
      return result[0] as T;
    } finally {
      reply.close();
    }
  }

  Future<void> initialize() => _run((database) => database.initialize());

  Future<void> close() async {
    final pending = _worker;
    if (pending == null) return;
    _worker = null;
    final reply = ReceivePort();
    try {
      (await pending).send((reply.sendPort, null));
      await reply.first;
    } finally {
      reply.close();
    }
  }

  Future<void> clear() => _run((database) => database.clear());

  Future<List<MessageSummary>> loadMessages(Conversation conversation) =>
      _run((database) => database.loadMessages(conversation));

  Future<List<MessageImage>> loadImages(
    Conversation conversation,
    String messageId,
  ) => _run((database) => database.loadImages(conversation, messageId));

  Future<void> saveMessages(
    Conversation conversation,
    List<MessageSummary> messages, {
    bool replaceImages = true,
  }) => _run(
    (database) => database.saveMessages(
      conversation,
      messages,
      replaceImages: replaceImages,
    ),
  );

  Future<void> saveImages(
    Conversation conversation,
    List<MessageSummary> messages,
  ) => _run((database) => database.saveImages(conversation, messages));

  Future<String> _databaseKey() async {
    final existing = await _secureStorage.read(key: _keyName);
    if (existing != null && existing.length == 64) return existing;
    final random = Random.secure();
    final key = List<int>.generate(
      32,
      (_) => random.nextInt(256),
    ).map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
    await _secureStorage.write(key: _keyName, value: key);
    return key;
  }
}

Future<void> _serveDatabase((SendPort, String?, String?) configuration) async {
  final (ready, path, key) = configuration;
  final requests = ReceivePort();
  final database = _WorkspaceDatabase(path, key);
  ready.send(requests.sendPort);
  await for (final request in requests) {
    final (reply, operation) =
        request as (SendPort, Future<Object?> Function(_WorkspaceDatabase)?);
    try {
      if (operation == null) {
        await database.close();
        reply.send([null]);
        requests.close();
        break;
      }
      reply.send([await operation(database)]);
    } catch (error, stack) {
      reply.send([error, stack]);
    }
  }
}

class _WorkspaceDatabase {
  _WorkspaceDatabase(this.path, this.key);

  final String? path;
  final String? key;
  Future<Database>? _database;

  Future<Database> _open() {
    final pending = _database;
    if (pending != null) return pending;
    return _database = _openDatabaseResetting();
  }

  Future<Database> _openDatabaseResetting() async {
    try {
      return await _openDatabase();
    } catch (_) {
      _database = null;
      rethrow;
    }
  }

  Future<void> initialize() async {
    try {
      await _open();
    } catch (_) {
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await _open();
    }
  }

  Future<void> close() async {
    final database = _database;
    _database = null;
    if (database != null) (await database).close();
  }

  Future<void> clear() async {
    final database = await _open();
    database.execute('BEGIN IMMEDIATE');
    try {
      database.execute('DELETE FROM message_images');
      database.execute('DELETE FROM messages');
      database.execute('COMMIT');
    } catch (_) {
      database.execute('ROLLBACK');
      rethrow;
    }
  }

  Future<Database> _openDatabase() async {
    final database = path == null
        ? sqlite3.openInMemory()
        : sqlite3.open(path!);
    try {
      if (path != null) {
        database.execute("PRAGMA key = \"x'$key'\"");
        final cipher = database.select('PRAGMA cipher_version');
        if (cipher.isEmpty ||
            cipher.first.values.first.toString().trim().isEmpty) {
          throw StateError('SQLCipher is unavailable');
        }
        database.execute('PRAGMA journal_mode = WAL');
        database.execute('PRAGMA synchronous = NORMAL');
      }
      database.execute('''
        CREATE TABLE IF NOT EXISTS messages (
          conversation_key TEXT NOT NULL,
          row_key TEXT NOT NULL,
          message_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          sender TEXT NOT NULL,
          sender_id TEXT,
          is_from_current_user INTEGER NOT NULL,
          timestamp TEXT NOT NULL,
          content TEXT NOT NULL,
          quotes_json TEXT NOT NULL DEFAULT '[]',
          reactions_json TEXT NOT NULL,
          image_urls_json TEXT NOT NULL DEFAULT '[]',
          PRIMARY KEY (conversation_key, row_key)
        )
      ''');
      final messageColumns = database.select('PRAGMA table_info(messages)');
      if (!messageColumns.any((row) => row['name'] == 'quotes_json')) {
        database.execute(
          "ALTER TABLE messages ADD COLUMN quotes_json TEXT NOT NULL DEFAULT '[]'",
        );
      }
      if (!messageColumns.any((row) => row['name'] == 'image_urls_json')) {
        database.execute(
          "ALTER TABLE messages ADD COLUMN image_urls_json TEXT NOT NULL DEFAULT '[]'",
        );
      }
      database.execute('''
        CREATE INDEX IF NOT EXISTS messages_by_conversation
        ON messages (conversation_key, ordinal)
      ''');
      database.execute('''
        CREATE TABLE IF NOT EXISTS message_images (
          conversation_key TEXT NOT NULL,
          message_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          content_type TEXT NOT NULL,
          source_url TEXT,
          bytes BLOB NOT NULL,
          PRIMARY KEY (conversation_key, message_id, ordinal)
        )
      ''');
      final imageColumns = database.select('PRAGMA table_info(message_images)');
      if (!imageColumns.any((row) => row['name'] == 'source_url')) {
        database.execute(
          'ALTER TABLE message_images ADD COLUMN source_url TEXT',
        );
      }
      return database;
    } catch (_) {
      database.close();
      rethrow;
    }
  }

  String _conversationKey(Conversation conversation) =>
      '${conversation.kind.name}:${conversation.teamId ?? ''}:${conversation.id}';

  Future<List<MessageSummary>> loadMessages(Conversation conversation) async {
    final database = await _open();
    final rows = database.select(
      '''
        SELECT message_id, sender, sender_id, is_from_current_user,
               timestamp, content, quotes_json, reactions_json, image_urls_json,
               (
                 SELECT COUNT(*) FROM message_images
                 WHERE conversation_key = messages.conversation_key
                   AND message_id = messages.message_id
               ) AS cached_image_count
        FROM messages
        WHERE conversation_key = ?
        ORDER BY ordinal
      ''',
      [_conversationKey(conversation)],
    );
    return [
      for (final row in rows)
        MessageSummary(
          id: row['message_id'] as String,
          sender: row['sender'] as String,
          senderId: row['sender_id'] as String?,
          isFromCurrentUser: (row['is_from_current_user'] as int) != 0,
          timestamp: row['timestamp'] as String,
          content: row['content'] as String,
          quotes: _decodeQuotes(row['quotes_json'] as String),
          reactions: _decodeReactions(row['reactions_json'] as String),
          imageUrls: row['image_urls_json'] == '[]'
              ? const []
              : (jsonDecode(row['image_urls_json'] as String) as List)
                    .cast<String>(),
          cachedImageCount: row['cached_image_count'] as int,
        ),
    ];
  }

  List<MessageQuote> _decodeQuotes(String encoded) {
    if (encoded == '[]') return const [];
    try {
      final values = jsonDecode(encoded);
      if (values is! List) return const [];
      return [
        for (final value in values)
          if (value is Map &&
              (value['sender'] is String || value['content'] is String))
            MessageQuote(
              messageId: value['messageId'] is String
                  ? value['messageId'] as String
                  : null,
              sender: value['sender'] is String
                  ? value['sender'] as String
                  : '',
              content: value['content'] is String
                  ? value['content'] as String
                  : '',
            ),
      ];
    } on FormatException {
      return const [];
    }
  }

  List<MessageReaction> _decodeReactions(String encoded) {
    if (encoded == '[]') return const [];
    try {
      final values = jsonDecode(encoded);
      if (values is! List) return const [];
      return [
        for (final value in values)
          if (value is Map &&
              value['type'] is String &&
              value['count'] is num &&
              (value['count'] as num).toInt() > 0)
            MessageReaction(
              type: value['type'] as String,
              count: (value['count'] as num).toInt(),
              users: [
                for (final user in (value['users'] as List? ?? const []))
                  ReactionUser(
                    id: user['id'] as String,
                    name: user['name'] as String,
                  ),
              ],
              selected: value['selected'] == true,
            ),
      ];
    } on FormatException {
      return const [];
    }
  }

  Future<List<MessageImage>> loadImages(
    Conversation conversation,
    String messageId,
  ) async {
    if (messageId.isEmpty || messageId.startsWith('local:')) return const [];
    final database = await _open();
    final rows = database.select(
      '''
        SELECT content_type, bytes, source_url
        FROM message_images
        WHERE conversation_key = ? AND message_id = ?
        ORDER BY ordinal
      ''',
      [_conversationKey(conversation), messageId],
    );
    return [
      for (final row in rows)
        MessageImage(
          contentType: row['content_type'] as String,
          sourceUrl: row['source_url'] as String?,
          bytes: row['bytes'] as Uint8List,
        ),
    ];
  }

  Future<void> saveMessages(
    Conversation conversation,
    List<MessageSummary> messages, {
    bool replaceImages = true,
  }) async {
    final database = await _open();
    final conversationKey = _conversationKey(conversation);
    PreparedStatement? insertMessage;
    PreparedStatement? deleteImages;
    PreparedStatement? insertImage;
    database.execute('BEGIN IMMEDIATE');
    try {
      database.execute('DELETE FROM messages WHERE conversation_key = ?', [
        conversationKey,
      ]);
      insertMessage = database.prepare('''
        INSERT INTO messages (
          conversation_key, row_key, message_id, ordinal, sender, sender_id,
          is_from_current_user, timestamp, content, quotes_json, reactions_json, image_urls_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ''');
      deleteImages = replaceImages
          ? database.prepare('''
              DELETE FROM message_images
              WHERE conversation_key = ? AND message_id = ?
            ''')
          : null;
      insertImage = replaceImages
          ? database.prepare('''
              INSERT INTO message_images (
                conversation_key, message_id, ordinal, content_type, bytes, source_url
              ) VALUES (?, ?, ?, ?, ?, ?)
            ''')
          : null;
      for (var index = 0; index < messages.length; index++) {
        final message = messages[index];
        final rowKey = message.id.isEmpty ? '@$index' : message.id;
        insertMessage.execute([
          conversationKey,
          rowKey,
          message.id,
          index,
          message.sender,
          message.senderId,
          message.isFromCurrentUser ? 1 : 0,
          message.timestamp,
          message.content,
          message.quotes.isEmpty
              ? '[]'
              : jsonEncode([
                  for (final quote in message.quotes)
                    {
                      'messageId': quote.messageId,
                      'sender': quote.sender,
                      'content': quote.content,
                    },
                ]),
          message.reactions.isEmpty
              ? '[]'
              : jsonEncode([
                  for (final reaction in message.reactions)
                    {
                      'type': reaction.type,
                      'count': reaction.count,
                      'selected': reaction.selected,
                      'users': [
                        for (final user in reaction.users)
                          {'id': user.id, 'name': user.name},
                      ],
                    },
                ]),
          message.imageUrls.isEmpty ? '[]' : jsonEncode(message.imageUrls),
        ]);
        if (replaceImages && message.id.isNotEmpty) {
          deleteImages!.execute([conversationKey, message.id]);
          for (
            var imageIndex = 0;
            imageIndex < message.images.length;
            imageIndex++
          ) {
            final image = message.images[imageIndex];
            insertImage!.execute([
              conversationKey,
              message.id,
              imageIndex,
              image.contentType,
              image.bytes,
              image.sourceUrl,
            ]);
          }
        }
      }

      database.execute(
        '''
        DELETE FROM message_images
        WHERE conversation_key = ?
          AND message_id NOT IN (
            SELECT message_id FROM messages
            WHERE conversation_key = ? AND message_id <> ''
          )
      ''',
        [conversationKey, conversationKey],
      );
      database.execute('COMMIT');
    } catch (_) {
      database.execute('ROLLBACK');
      rethrow;
    } finally {
      insertMessage?.close();
      deleteImages?.close();
      insertImage?.close();
    }
  }

  Future<void> saveImages(
    Conversation conversation,
    List<MessageSummary> messages,
  ) async {
    final database = await _open();
    final conversationKey = _conversationKey(conversation);
    PreparedStatement? deleteImages;
    PreparedStatement? insertImage;
    database.execute('BEGIN IMMEDIATE');
    try {
      deleteImages = database.prepare('''
        DELETE FROM message_images
        WHERE conversation_key = ? AND message_id = ?
      ''');
      insertImage = database.prepare('''
        INSERT INTO message_images (
          conversation_key, message_id, ordinal, content_type, bytes, source_url
        ) VALUES (?, ?, ?, ?, ?, ?)
      ''');
      for (final message in messages) {
        if (message.id.isEmpty || message.images.isEmpty) continue;
        deleteImages.execute([conversationKey, message.id]);
        for (
          var imageIndex = 0;
          imageIndex < message.images.length;
          imageIndex++
        ) {
          final image = message.images[imageIndex];
          insertImage.execute([
            conversationKey,
            message.id,
            imageIndex,
            image.contentType,
            image.bytes,
            image.sourceUrl,
          ]);
        }
      }

      database.execute('COMMIT');
    } catch (_) {
      database.execute('ROLLBACK');
      rethrow;
    } finally {
      deleteImages?.close();
      insertImage?.close();
    }
  }
}
