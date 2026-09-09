import 'dart:math';

List<String> baseline(List<String> userIds) => userIds
    .map((id) => id.trim())
    .where((id) => id.isNotEmpty)
    .toSet()
    .take(650)
    .toList(growable: false);

List<String> optimized(List<String> userIds) {
  final ids = <String>[];
  final seen = <String>{};
  for (final rawId in userIds) {
    final id = rawId.trim();
    if (id.isEmpty || !seen.add(id)) continue;
    ids.add(id);
    if (ids.length == 650) break;
  }
  return ids;
}

int measure(
  List<String> Function(List<String>) run,
  List<String> ids,
  int iterations,
) {
  final stopwatch = Stopwatch()..start();
  var checksum = 0;
  for (var iteration = 0; iteration < iterations; iteration++) {
    checksum += run(ids).length;
  }
  if (checksum == Random(0).nextInt(1)) throw StateError('unreachable');
  return stopwatch.elapsedMicroseconds;
}

int median(List<int> values) {
  values.sort();
  return values[values.length ~/ 2];
}

void main() {
  for (final count in [100, 650, 5000]) {
    final ids = List.generate(
      count,
      (index) => '  user-${index % 900}-0123456789abcdef  ',
    );
    final before = <int>[];
    final after = <int>[];
    for (var sample = 0; sample < 9; sample++) {
      before.add(measure(baseline, ids, 2000));
      after.add(measure(optimized, ids, 2000));
    }
    final baselineUs = median(before);
    final optimizedUs = median(after);
    final gain = (baselineUs - optimizedUs) / baselineUs * 100;
    print(
      'count=$count baseline_us=$baselineUs optimized_us=$optimizedUs gain=${gain.toStringAsFixed(1)}%',
    );
    assert(baseline(ids).join('|') == optimized(ids).join('|'));
  }
}
