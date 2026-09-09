import 'dart:math';

final class Chat {
  const Chat(this.id);
  final String id;
}

int baseline(List<Chat> chats, bool notify) {
  final previous = {for (final chat in chats) chat.id: chat};
  return notify ? previous.length : chats.length;
}

int optimized(List<Chat> chats, bool notify) {
  final previous = notify ? {for (final chat in chats) chat.id: chat} : null;
  return previous?.length ?? chats.length;
}

int measure(int Function(List<Chat>, bool) run, List<Chat> chats, bool notify) {
  final stopwatch = Stopwatch()..start();
  var checksum = 0;
  for (var iteration = 0; iteration < 20000; iteration++) {
    checksum += run(chats, notify);
  }
  if (checksum == Random(0).nextInt(1)) throw StateError('unreachable');
  return stopwatch.elapsedMicroseconds;
}

int median(List<int> values) {
  values.sort();
  return values[values.length ~/ 2];
}

void main() {
  final chats = List.generate(500, (index) => Chat('chat-$index'));
  for (final notify in [false, true]) {
    final before = <int>[];
    final after = <int>[];
    for (var sample = 0; sample < 9; sample++) {
      before.add(measure(baseline, chats, notify));
      after.add(measure(optimized, chats, notify));
    }
    final baselineUs = median(before);
    final optimizedUs = median(after);
    final gain = (baselineUs - optimizedUs) / baselineUs * 100;
    print(
      'notify=$notify baseline_us=$baselineUs optimized_us=$optimizedUs gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
