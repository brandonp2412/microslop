bool baseline(String content, String name) => RegExp(
  '@\\s*${RegExp.escape(name)}',
  caseSensitive: false,
).hasMatch(content);

final cache = <String, RegExp>{};
bool optimized(String content, String name) => cache
    .putIfAbsent(
      name,
      () => RegExp('@\\s*${RegExp.escape(name)}', caseSensitive: false),
    )
    .hasMatch(content);

int run(bool Function(String, String) fn, int rounds) {
  const content = 'Could you check this with @brandonp2412 before release?';
  const name = 'brandonp2412';
  final watch = Stopwatch()..start();
  var matches = 0;
  for (var i = 0; i < rounds; i++) {
    if (fn(content, name)) matches++;
  }
  watch.stop();
  if (matches != rounds) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  const rounds = 200000;
  run(baseline, 100);
  run(optimized, 100);
  final old = run(baseline, rounds);
  final next = run(optimized, rounds);
  print(
    'baseline_us=$old optimized_us=$next gain=${((old - next) * 100 / old).toStringAsFixed(1)}%',
  );
}
