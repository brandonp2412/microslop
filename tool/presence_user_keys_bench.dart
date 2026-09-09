class Chat {
  const Chat(this.group, this.teamId, this.userId);
  final bool group;
  final String? teamId;
  final String? userId;
}

String key(String id) => id.trim().toLowerCase();

Set<String> baseline(List<Chat> chats) => {
  for (final entry in {
    for (final chat in chats)
      if (!chat.group && chat.teamId == null)
        if (chat.userId?.trim() case final id? when id.isNotEmpty) key(id): id,
  }.keys)
    entry,
};

Set<String> optimized(List<Chat> chats) => {
  for (final chat in chats)
    if (!chat.group && chat.teamId == null)
      if (chat.userId?.trim() case final id? when id.isNotEmpty) key(id),
};

int run(Set<String> Function(List<Chat>) fn, List<Chat> chats, int rounds) {
  final watch = Stopwatch()..start();
  var total = 0;
  for (var i = 0; i < rounds; i++) {
    total += fn(chats).length;
  }
  watch.stop();
  if (total == 0) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  final chats = List.generate(
    1000,
    (i) => Chat(i % 5 == 0, i % 7 == 0 ? 'team' : null, 'User-${i % 700}'),
  );
  const rounds = 20000;
  run(baseline, chats, 100);
  run(optimized, chats, 100);
  final old = run(baseline, chats, rounds);
  final next = run(optimized, chats, rounds);
  print(
    'baseline_us=$old optimized_us=$next gain=${((old - next) * 100 / old).toStringAsFixed(1)}%',
  );
}
