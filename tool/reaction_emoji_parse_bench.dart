final codepointPattern = RegExp(r'^([0-9a-f]{4,6})_');

String baseline(String type) {
  final name = type.split(';').first.trim().toLowerCase();
  return RegExp(r'^([0-9a-f]{4,6})_').firstMatch(name)?.group(1) ?? name;
}

String optimized(String type) {
  final separator = type.indexOf(';');
  final name = (separator < 0 ? type : type.substring(0, separator))
      .trim()
      .toLowerCase();
  return codepointPattern.firstMatch(name)?.group(1) ?? name;
}

int measure(String Function(String) run, List<String> values, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    for (final value in values) {
      checksum += run(value).length;
    }
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
  const values = [
    'like',
    '1f9e0_brain',
    'party-parrot;0-sau-d4-asset',
    'approved-with-comments;0-eau-d1-asset',
  ];
  const iterations = 100000;
  final before = <int>[];
  final after = <int>[];
  for (var sample = 0; sample < 9; sample++) {
    before.add(measure(baseline, values, iterations));
    after.add(measure(optimized, values, iterations));
  }
  final baselineUs = median(before);
  final optimizedUs = median(after);
  print(
    'baseline_us=$baselineUs optimized_us=$optimizedUs gain=${((baselineUs - optimizedUs) * 100 / baselineUs).toStringAsFixed(1)}%',
  );
}
