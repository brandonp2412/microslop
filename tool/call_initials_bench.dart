final whitespacePattern = RegExp(r'\s+');

String initials(String name) {
  final words = name
      .trim()
      .split(whitespacePattern)
      .where((word) => word.isNotEmpty)
      .toList();
  if (words.isEmpty) return '?';
  if (words.length == 1) return words.first[0].toUpperCase();
  return '${words.first[0]}${words.last[0]}'.toUpperCase();
}

void main() {
  const name = 'Grace Brewster Murray Hopper';
  final cached = initials(name);
  for (var i = 0; i < 20000; i++) {
    initials(name);
    cached.length;
  }
  int run(bool useCached) {
    final watch = Stopwatch()..start();
    var total = 0;
    for (var i = 0; i < 200000; i++) {
      total += (useCached ? cached : initials(name)).length;
    }
    watch.stop();
    if (total != 400000) throw StateError('Bad benchmark result');
    return watch.elapsedMicroseconds;
  }

  final oldTimes = <int>[];
  final newTimes = <int>[];
  for (var i = 0; i < 21; i++) {
    oldTimes.add(run(false));
    newTimes.add(run(true));
  }
  oldTimes.sort();
  newTimes.sort();
  final old = oldTimes[10];
  final fresh = newTimes[10];
  print(
    'recompute=${old}us cached=${fresh}us gain=${((old - fresh) * 100 / old).toStringAsFixed(1)}%',
  );
}
