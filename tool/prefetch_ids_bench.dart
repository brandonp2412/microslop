import 'dart:math';

class Conversation {
  const Conversation(this.id);
  final String id;
}

List<String> baseline({
  required List<Conversation> directMessages,
  required List<Conversation> groups,
  required List<Conversation> channels,
  int limit = 8,
}) {
  if (limit <= 0) return const [];
  final ids = <String>[];
  final seen = <String>{};
  for (final conversation in [...directMessages, ...groups, ...channels]) {
    final id = conversation.id.trim();
    if (id.isEmpty || !seen.add(id)) continue;
    ids.add(id);
    if (ids.length >= limit) break;
  }
  return ids;
}

List<String> optimized({
  required List<Conversation> directMessages,
  required List<Conversation> groups,
  required List<Conversation> channels,
  int limit = 8,
}) {
  if (limit <= 0) return const [];
  final ids = <String>[];
  final seen = <String>{};
  for (final conversations in [directMessages, groups, channels]) {
    for (final conversation in conversations) {
      final id = conversation.id.trim();
      if (id.isEmpty || !seen.add(id)) continue;
      ids.add(id);
      if (ids.length >= limit) return ids;
    }
  }
  return ids;
}

int measure(List<String> Function() run, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    checksum += run().length;
  }
  watch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  final random = Random(42);
  List<Conversation> build(int count, int offset) => List.generate(
    count,
    (index) => Conversation(
      index % 17 == 0
          ? '  '
          : 'id-${offset + index % 9000}-${random.nextInt(7)}',
    ),
    growable: false,
  );

  final directMessages = build(10000, 0);
  final groups = build(10000, 10000);
  final channels = build(10000, 20000);
  const iterations = 500;

  for (final limit in [8, 100, 30000]) {
    final baselineSamples = <int>[];
    final optimizedSamples = <int>[];
    for (var sample = 0; sample < 7; sample++) {
      baselineSamples.add(
        measure(
          () => baseline(
            directMessages: directMessages,
            groups: groups,
            channels: channels,
            limit: limit,
          ),
          iterations,
        ),
      );
      optimizedSamples.add(
        measure(
          () => optimized(
            directMessages: directMessages,
            groups: groups,
            channels: channels,
            limit: limit,
          ),
          iterations,
        ),
      );
    }
    baselineSamples.sort();
    optimizedSamples.sort();
    final before = baselineSamples[baselineSamples.length ~/ 2];
    final after = optimizedSamples[optimizedSamples.length ~/ 2];
    final gain = (before - after) / before * 100;
    print(
      'limit=$limit baseline_us=$before optimized_us=$after gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
