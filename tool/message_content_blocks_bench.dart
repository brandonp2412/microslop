List<(String, String?)> baseline(String text) {
  final lines = text
      .replaceAll('\r\n', '\n')
      .replaceAll('\r', '\n')
      .split('\n');
  final blocks = <(String, String?)>[];
  final current = <String>[];
  var inCode = false;
  String? language;

  void flush() {
    if (current.isEmpty) return;
    blocks.add((current.join('\n'), inCode ? language ?? '' : null));
    current.clear();
  }

  for (final line in lines) {
    final fence = line.trimLeft();
    if (fence.startsWith('```')) {
      flush();
      if (inCode) {
        inCode = false;
        language = null;
      } else {
        inCode = true;
        language = fence.substring(3).trim();
      }
      continue;
    }
    current.add(line);
  }
  flush();
  return blocks.isEmpty ? [(text, null)] : blocks;
}

List<(String, String?)> optimized(String text) {
  if (!text.contains('```')) return [(text, null)];
  return baseline(text);
}

int run(List<(String, String?)> Function(String) fn, String text, int rounds) {
  final watch = Stopwatch()..start();
  var total = 0;
  for (var i = 0; i < rounds; i++) {
    total += fn(text).length;
  }
  watch.stop();
  if (total == 0) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  const plain = 'Checking it now. The new navigation feels much faster.';
  const fenced = 'Example:\n```dart\nprint("hello");\n```\nDone.';
  const rounds = 500000;
  run(baseline, plain, 1000);
  run(optimized, plain, 1000);
  final oldPlain = run(baseline, plain, rounds);
  final newPlain = run(optimized, plain, rounds);
  final oldFenced = run(baseline, fenced, rounds ~/ 10);
  final newFenced = run(optimized, fenced, rounds ~/ 10);
  print(
    'plain baseline_us=$oldPlain optimized_us=$newPlain gain=${((oldPlain - newPlain) * 100 / oldPlain).toStringAsFixed(1)}%',
  );
  print(
    'fenced baseline_us=$oldFenced optimized_us=$newFenced gain=${((oldFenced - newFenced) * 100 / oldFenced).toStringAsFixed(1)}%',
  );
}
