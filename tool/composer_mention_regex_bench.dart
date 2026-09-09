final mentionPattern = RegExp(r'@([\p{L}\p{N}_.-]*)$', unicode: true);

String? baseline(String text) => RegExp(
  r'@([\p{L}\p{N}_.-]*)$',
  unicode: true,
).firstMatch(text)?.group(1);

String? optimized(String text) => mentionPattern.firstMatch(text)?.group(1);

int measure(String? Function(String) run, int iterations) {
  const text = 'Please ask @brandonp2412';
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    checksum += run(text)?.length ?? 0;
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
  const iterations = 200000;
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
