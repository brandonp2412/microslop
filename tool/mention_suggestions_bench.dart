import 'dart:math';

List<String> baseline(String query, List<String> messages) {
  final names = <String>{for (final sender in messages) sender.trim()};
  final normalized = query.toLowerCase();
  return names
      .where((name) => name.toLowerCase().startsWith(normalized))
      .take(6)
      .toList();
}

List<(String, String)> prepare(List<String> messages) => [
  for (final name in <String>{for (final sender in messages) sender.trim()})
    (name, name.toLowerCase()),
];

List<String> cached(String query, List<(String, String)> names) {
  final normalized = query.toLowerCase();
  return names
      .where((entry) => entry.$2.startsWith(normalized))
      .map((entry) => entry.$1)
      .take(6)
      .toList();
}

List<String> direct(String query, List<(String, String)> names) {
  final normalized = query.toLowerCase();
  final suggestions = <String>[];
  for (final entry in names) {
    if (!entry.$2.startsWith(normalized)) continue;
    suggestions.add(entry.$1);
    if (suggestions.length == 6) break;
  }
  return suggestions;
}

int runBaseline(List<String> messages) {
  var checksum = 0;
  for (var i = 0; i < 5000; i++) {
    checksum ^= baseline('person ${i % 40}', messages).length;
  }
  return checksum;
}

int runCached(List<String> messages) {
  final names = prepare(messages);
  var checksum = 0;
  for (var i = 0; i < 5000; i++) {
    checksum ^= cached('person ${i % 40}', names).length;
  }
  return checksum;
}

int runDirect(List<String> messages) {
  final names = prepare(messages);
  var checksum = 0;
  for (var i = 0; i < 5000; i++) {
    checksum ^= direct('person ${i % 40}', names).length;
  }
  return checksum;
}

int measure(int Function(List<String>) run, List<String> messages) {
  final stopwatch = Stopwatch()..start();
  run(messages);
  stopwatch.stop();
  return stopwatch.elapsedMicroseconds;
}

int percentile(List<int> values, double percentile) {
  values.sort();
  return values[((values.length - 1) * percentile).round()];
}

void main() {
  final random = Random(7);
  final messages = List.generate(
    5000,
    (index) => 'Person ${random.nextInt(400)}',
  );
  final before = List.generate(15, (_) => measure(runCached, messages));
  final after = List.generate(15, (_) => measure(runDirect, messages));
  final beforeP50 = percentile(before, .5);
  final afterP50 = percentile(after, .5);
  final beforeP95 = percentile(before, .95);
  final afterP95 = percentile(after, .95);
  print(
    'p50_us=$beforeP50->$afterP50 gain=${((beforeP50 - afterP50) * 100 / beforeP50).toStringAsFixed(1)}% '
    'p95_us=$beforeP95->$afterP95 gain=${((beforeP95 - afterP95) * 100 / beforeP95).toStringAsFixed(1)}%',
  );
}
