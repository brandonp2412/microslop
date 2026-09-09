import 'dart:convert';

bool listEquals<T>(List<T> a, List<T> b) {
  if (identical(a, b)) return true;
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

bool baseline(List<String> a, List<String> b) => jsonEncode(a) == jsonEncode(b);
bool optimized(List<String> a, List<String> b) => listEquals(a, b);

int run(
  bool Function(List<String>, List<String>) fn,
  List<String> a,
  List<String> b,
  int rounds,
) {
  final watch = Stopwatch()..start();
  var equal = 0;
  for (var i = 0; i < rounds; i++) {
    if (fn(a, b)) equal++;
  }
  watch.stop();
  if (equal != rounds) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  final a = List.generate(20, (i) => 'user-$i');
  final b = List<String>.from(a);
  const rounds = 200000;
  run(baseline, a, b, 100);
  run(optimized, a, b, 100);
  final old = run(baseline, a, b, rounds);
  final next = run(optimized, a, b, rounds);
  print(
    'baseline_us=$old optimized_us=$next gain=${((old - next) * 100 / old).toStringAsFixed(1)}%',
  );
}
