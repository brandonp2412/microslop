class Reaction {
  const Reaction(this.type, this.count);
  final String type;
  final int count;
}

String? baseline(List<Reaction> previous, List<Reaction> current) {
  final previousCounts = {
    for (final reaction in previous) reaction.type: reaction.count,
  };
  for (final reaction in current) {
    if (reaction.count > (previousCounts[reaction.type] ?? 0)) {
      return reaction.type;
    }
  }
  return null;
}

String? optimized(List<Reaction> previous, List<Reaction> current) {
  for (final reaction in current) {
    var previousCount = 0;
    for (final old in previous.reversed) {
      if (old.type == reaction.type) {
        previousCount = old.count;
        break;
      }
    }
    if (reaction.count > previousCount) return reaction.type;
  }
  return null;
}

int measure(
  String? Function(List<Reaction>, List<Reaction>) run,
  List<Reaction> previous,
  List<Reaction> current,
  int iterations,
) {
  final stopwatch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    checksum += run(previous, current)?.length ?? 0;
  }
  stopwatch.stop();
  if (checksum == 0) throw StateError('benchmark result unused');
  return stopwatch.elapsedMicroseconds;
}

int percentile(List<int> values, int numerator, int denominator) {
  values.sort();
  return values[(values.length - 1) * numerator ~/ denominator];
}

void main() {
  final previous = [
    const Reaction('like', 4),
    const Reaction('heart', 2),
    const Reaction('laugh', 3),
    const Reaction('surprised', 1),
    const Reaction('sad', 1),
    const Reaction('angry', 1),
  ];
  final current = [...previous.take(5), const Reaction('angry', 2)];
  const iterations = 1000000;
  final before = [
    for (var i = 0; i < 11; i++)
      measure(baseline, previous, current, iterations),
  ];
  final after = [
    for (var i = 0; i < 11; i++)
      measure(optimized, previous, current, iterations),
  ];
  final beforeP50 = percentile([...before], 1, 2);
  final afterP50 = percentile([...after], 1, 2);
  final beforeP95 = percentile([...before], 19, 20);
  final afterP95 = percentile([...after], 19, 20);
  print(
    'p50_us=$beforeP50->$afterP50 gain=${((beforeP50 - afterP50) * 100 / beforeP50).toStringAsFixed(1)}% '
    'p95_us=$beforeP95->$afterP95 gain=${((beforeP95 - afterP95) * 100 / beforeP95).toStringAsFixed(1)}%',
  );
}
