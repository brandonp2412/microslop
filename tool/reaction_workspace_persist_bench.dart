import 'dart:convert';

Map<String, Object?> workspace(int chats, int teams, int channels) => {
  'schemaVersion': 2,
  'chats': [
    for (var i = 0; i < chats; i++)
      {
        'id': 'chat-$i',
        'name': 'Conversation $i',
        'isGroup': i % 5 == 0,
        'teamId': null,
        'profilePhotoUserId': 'user-$i',
        'memberUserIds': ['user-$i'],
        'lastMessageId': 'message-$i',
        'preview': 'Latest message preview for conversation $i',
      },
  ],
  'teams': [
    for (var team = 0; team < teams; team++)
      {
        'id': 'team-$team',
        'name': 'Team $team',
        'channels': [
          for (var channel = 0; channel < channels; channel++)
            {
              'id': 'channel-$team-$channel',
              'name': 'Channel $channel',
              'isGroup': false,
              'teamId': 'team-$team',
              'profilePhotoUserId': null,
              'memberUserIds': const <String>[],
              'lastMessageId': null,
              'preview': null,
            },
        ],
      },
  ],
  'selectedChatId': 'chat-0',
  'messages': const <String, Object?>{},
  'hiddenConversationIds': const <String>[],
  'hiddenSectionIds': const <String>[],
};

int baseline(Map<String, Object?> snapshot) => jsonEncode(snapshot).length;
int optimized(Map<String, Object?> snapshot) => snapshot.length;

int measure(int Function() run, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    checksum += run();
  }
  watch.stop();
  if (checksum == 0) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  for (final chats in [100, 500, 2000]) {
    final snapshot = workspace(chats, 20, 20);
    final before = <int>[];
    final after = <int>[];
    for (var sample = 0; sample < 7; sample++) {
      before.add(measure(() => baseline(snapshot), 100));
      after.add(measure(() => optimized(snapshot), 100));
    }
    before.sort();
    after.sort();
    final b = before[before.length ~/ 2];
    final a = after[after.length ~/ 2];
    final gain = (b - a) * 100 / b;
    print(
      'chats=$chats baseline_us=$b optimized_us=$a gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
