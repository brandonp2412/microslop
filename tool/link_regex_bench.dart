String? baseline(String text) =>
    RegExp(r'https?://[^\s<>()]+').firstMatch(text)?.group(0);

final linkPattern = RegExp(r'https?://[^\s<>()]+');
String? optimized(String text) => linkPattern.firstMatch(text)?.group(0);

int run(String? Function(String) fn, int rounds) {
  const text =
      'See the deployment notes at https://example.com/build/123?x=1 and reply when done.';
  final watch = Stopwatch()..start();
  var matches = 0;
  for (var i = 0; i < rounds; i++) {
    if (fn(text) != null) matches++;
  }
  watch.stop();
  if (matches != rounds) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  const rounds = 500000;
  run(baseline, 100);
  run(optimized, 100);
  final old = run(baseline, rounds);
  final next = run(optimized, rounds);
  print(
    'baseline_us=$old optimized_us=$next gain=${((old - next) * 100 / old).toStringAsFixed(1)}%',
  );
}
