final whitespacePattern = RegExp(r'\s+');

int baseline(String value) => value
    .trim()
    .split(RegExp(r'\s+'))
    .where((part) => part.isNotEmpty)
    .length;

int optimized(String value) => value
    .trim()
    .split(whitespacePattern)
    .where((part) => part.isNotEmpty)
    .length;

int measure(int Function(String) run, int iterations) {
  const values = [
    'brandonp2412',
    'Ada   Lovelace',
    'Grace\tBrewster\nMurray Hopper',
  ];
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    for (final value in values) checksum += run(value);
  }
  watch.stop();
  if (checksum == 0) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

int median(List<int> values) {
  values.sort();
  return values[values.length ~/ 2];
}

void main() {
  const iterations = 100000;
  final before = <int>[];
  final after = <int>[];
  for (var sample = 0; sample < 9; sample++) {
    before.add(measure(baseline, iterations));
    after.add(measure(optimized, iterations));
  }
  final baselineUs = median(before);
  final optimizedUs = median(after);
  print(
    'baseline_us=$baselineUs optimized_us=$optimizedUs gain=${((baselineUs - optimizedUs) * 100 / baselineUs).toStringAsFixed(1)}%',
  );
}
