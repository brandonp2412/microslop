import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:microslop/workspace_database_native.dart';

void main() {
  test('bench cached plain-message load', () async {
    final database = WorkspaceDatabase(inMemory: true);
    addTearDown(database.close);
    await database.initialize();
    const conversation = Conversation.chat(
      id: 'benchmark',
      name: 'Benchmark',
      isGroup: false,
    );
    final messages = [
      for (var index = 0; index < 10000; index++)
        MessageSummary(
          id: '$index',
          sender: 'Benchmark',
          timestamp: '2026-09-09T00:00:00Z',
          content: 'Plain cached message $index',
        ),
    ];
    await database.saveMessages(conversation, messages);
    for (var i = 0; i < 2; i++) {
      await database.loadMessages(conversation);
    }
    final samples = <int>[];
    for (var i = 0; i < 9; i++) {
      final watch = Stopwatch()..start();
      final loaded = await database.loadMessages(conversation);
      watch.stop();
      expect(loaded, hasLength(messages.length));
      samples.add(watch.elapsedMicroseconds);
    }
    samples.sort();
    final p50 = samples[samples.length ~/ 2];
    final p95 = samples[(samples.length * .95).ceil() - 1];
    print('samples_us=$samples p50_us=$p50 p95_us=$p95');
  });
}
