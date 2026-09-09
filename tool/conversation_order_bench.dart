class Conversation {
  const Conversation(this.id);
  final String id;
}

List<Conversation> baseline(
  Iterable<Conversation> conversations,
  Set<String> favorites,
) => [
  ...conversations.where((conversation) => favorites.contains(conversation.id)),
  ...conversations.where(
    (conversation) => !favorites.contains(conversation.id),
  ),
];

List<Conversation> optimized(
  Iterable<Conversation> conversations,
  Set<String> favorites,
) {
  final ordered = <Conversation>[];
  final others = <Conversation>[];
  for (final conversation in conversations) {
    (favorites.contains(conversation.id) ? ordered : others).add(conversation);
  }
  return ordered..addAll(others);
}

int measure(List<Conversation> Function() run, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    final result = run();
    checksum += result.length + result.first.id.length + result.last.id.length;
  }
  watch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  for (final count in [100, 1000, 10000]) {
    final source = List.generate(count, (index) => Conversation('chat-$index'));
    final favorites = {for (var i = 0; i < count; i += 7) 'chat-$i'};
    Iterable<Conversation> decorated() => source
        .where((conversation) => conversation.id.hashCode.isEven)
        .map((conversation) => Conversation('${conversation.id}-decorated'));
    final beforeSamples = <int>[];
    final afterSamples = <int>[];
    final iterations = 100000 ~/ count;
    for (var sample = 0; sample < 9; sample++) {
      beforeSamples.add(
        measure(() => baseline(decorated(), favorites), iterations),
      );
      afterSamples.add(
        measure(() => optimized(decorated(), favorites), iterations),
      );
    }
    beforeSamples.sort();
    afterSamples.sort();
    final before = beforeSamples[beforeSamples.length ~/ 2];
    final after = afterSamples[afterSamples.length ~/ 2];
    final gain = (before - after) / before * 100;
    print(
      'count=$count baseline_us=$before optimized_us=$after gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
