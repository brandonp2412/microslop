class User {
  const User(this.id, this.name);
  final String id;
  final String name;
}

class Reaction {
  const Reaction(this.type, this.count, this.selected, this.users);
  final String type;
  final int count;
  final bool selected;
  final List<User> users;
}

Map<String, (int, bool, List<User>)> baseline(List<Reaction> source) {
  final reactions = <String, (int, bool)>{};
  for (final reaction in source) {
    final type = reaction.type.trim();
    if (type.isEmpty || reaction.count <= 0) continue;
    final existing = reactions[type];
    reactions[type] = (
      (existing?.$1 ?? 0) + reaction.count,
      (existing?.$2 ?? false) || reaction.selected,
    );
  }
  return {
    for (final reaction in reactions.entries)
      reaction.key: (
        reaction.value.$1,
        reaction.value.$2,
        [
          for (final value in source.where(
            (value) => value.type.trim() == reaction.key,
          ))
            ...value.users,
        ],
      ),
  };
}

Map<String, (int, bool, List<User>)> optimized(List<Reaction> source) {
  final reactions = <String, (int, bool, List<User>)>{};
  for (final reaction in source) {
    final type = reaction.type.trim();
    if (type.isEmpty || reaction.count <= 0) continue;
    final existing = reactions[type];
    reactions[type] = (
      (existing?.$1 ?? 0) + reaction.count,
      (existing?.$2 ?? false) || reaction.selected,
      [...?existing?.$3, ...reaction.users],
    );
  }
  return reactions;
}

int measure(
  Map<String, (int, bool, List<User>)> Function() run,
  int iterations,
) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    final result = run();
    checksum +=
        result.length +
        result.values.fold(0, (sum, value) => sum + value.$3.length);
  }
  watch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  for (final typeCount in [6, 30, 120]) {
    final source = List.generate(
      typeCount * 4,
      (index) => Reaction('reaction-${index % typeCount}', 1, index.isEven, [
        User('u$index', 'User $index'),
      ]),
      growable: false,
    );
    final iterations = 2000 ~/ (typeCount ~/ 6);
    final beforeSamples = <int>[];
    final afterSamples = <int>[];
    for (var sample = 0; sample < 7; sample++) {
      beforeSamples.add(measure(() => baseline(source), iterations));
      afterSamples.add(measure(() => optimized(source), iterations));
    }
    beforeSamples.sort();
    afterSamples.sort();
    final before = beforeSamples[beforeSamples.length ~/ 2];
    final after = afterSamples[afterSamples.length ~/ 2];
    final gain = (before - after) / before * 100;
    print(
      'types=$typeCount entries=${source.length} baseline_us=$before optimized_us=$after gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
