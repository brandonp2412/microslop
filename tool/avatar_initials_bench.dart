final whitespace = RegExp(r'\s+');

String compute(String name) => name
    .trim()
    .split(whitespace)
    .where((part) => part.isNotEmpty)
    .take(2)
    .map((part) => String.fromCharCode(part.runes.first).toUpperCase())
    .join();

void main() {
  final names = [for (var i = 0; i < 250; i++) 'Person $i Example'];
  final cache = <String, String>{};
  for (final name in names) {
    compute(name);
    cache.putIfAbsent(name, () => compute(name));
  }

  int run(bool cached) {
    final watch = Stopwatch()..start();
    var total = 0;
    for (var rebuild = 0; rebuild < 2000; rebuild++) {
      for (final name in names) {
        total += (cached ? cache[name]! : compute(name)).length;
      }
    }
    watch.stop();
    if (total != 1000000) throw StateError('Bad benchmark result');
    return watch.elapsedMicroseconds;
  }

  final oldTimes = <int>[];
  final newTimes = <int>[];
  for (var i = 0; i < 15; i++) {
    oldTimes.add(run(false));
    newTimes.add(run(true));
  }
  oldTimes.sort();
  newTimes.sort();
  final old = oldTimes[oldTimes.length ~/ 2];
  final fresh = newTimes[newTimes.length ~/ 2];
  print(
    'recompute=${old}us cached=${fresh}us gain=${((old - fresh) * 100 / old).toStringAsFixed(1)}%',
  );
}
