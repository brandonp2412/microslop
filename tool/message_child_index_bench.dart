import 'dart:math';

int? linear(List<String> ids, String key) {
  for (var index = 0; index < ids.length; index++) {
    if (ids[index] == key) return ids.length - 1 - index;
  }
  return null;
}

Map<String, int> buildIndex(List<String> ids) => {
  for (var index = 0; index < ids.length; index++)
    ids[index]: ids.length - 1 - index,
};

int runLinear(List<String> ids, List<String> lookups) {
  var checksum = 0;
  for (final key in lookups) {
    checksum ^= linear(ids, key) ?? 0;
  }
  return checksum;
}

int runIndexed(List<String> ids, List<String> lookups) {
  final index = buildIndex(ids);
  var checksum = 0;
  for (final key in lookups) {
    checksum ^= index[key] ?? 0;
  }
  return checksum;
}

int measure(
  int Function(List<String>, List<String>) run,
  List<String> ids,
  List<String> lookups,
) {
  final stopwatch = Stopwatch()..start();
  run(ids, lookups);
  stopwatch.stop();
  return stopwatch.elapsedMicroseconds;
}

int percentile(List<int> values, double percentile) {
  values.sort();
  return values[((values.length - 1) * percentile).round()];
}

void main() {
  final ids = List.generate(5000, (index) => 'message-$index');
  final random = Random(17);
  final lookups = List.generate(5000, (_) => ids[random.nextInt(ids.length)]);
  final before = List.generate(15, (_) => measure(runLinear, ids, lookups));
  final after = List.generate(15, (_) => measure(runIndexed, ids, lookups));
  final beforeP50 = percentile(before, .5);
  final afterP50 = percentile(after, .5);
  final beforeP95 = percentile(before, .95);
  final afterP95 = percentile(after, .95);
  print(
    'p50_us=$beforeP50->$afterP50 gain=${((beforeP50 - afterP50) * 100 / beforeP50).toStringAsFixed(1)}% '
    'p95_us=$beforeP95->$afterP95 gain=${((beforeP95 - afterP95) * 100 / beforeP95).toStringAsFixed(1)}%',
  );
}
