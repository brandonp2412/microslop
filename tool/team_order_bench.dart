class Conversation {
  const Conversation(this.id);
  final String id;
}

class Team {
  const Team(this.id, this.channels);
  final String id;
  final List<Conversation> channels;
}

List<Team> baseline(List<Team> teams, Set<String> favorites) => [
  ...teams.where(
    (team) => team.channels.any((channel) => favorites.contains(channel.id)),
  ),
  ...teams.where(
    (team) => !team.channels.any((channel) => favorites.contains(channel.id)),
  ),
];

List<Team> optimized(List<Team> teams, Set<String> favorites) {
  final ordered = <Team>[];
  final others = <Team>[];
  for (final team in teams) {
    (team.channels.any((channel) => favorites.contains(channel.id))
            ? ordered
            : others)
        .add(team);
  }
  return ordered..addAll(others);
}

int measure(List<Team> Function() run, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    final result = run();
    checksum += result.length + result.first.id.length + result.last.id.length;
  }
  watch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  for (final teamCount in [20, 100, 500]) {
    final teams = List.generate(
      teamCount,
      (team) => Team(
        'team-$team',
        List.generate(50, (channel) => Conversation('channel-$team-$channel')),
      ),
    );
    final favorites = {
      for (var team = 0; team < teamCount; team += 7) 'channel-$team-49',
    };
    final iterations = 20000 ~/ teamCount;
    final beforeSamples = <int>[];
    final afterSamples = <int>[];
    for (var sample = 0; sample < 9; sample++) {
      beforeSamples.add(measure(() => baseline(teams, favorites), iterations));
      afterSamples.add(measure(() => optimized(teams, favorites), iterations));
    }
    beforeSamples.sort();
    afterSamples.sort();
    final before = beforeSamples[beforeSamples.length ~/ 2];
    final after = afterSamples[afterSamples.length ~/ 2];
    final gain = (before - after) / before * 100;
    print(
      'teams=$teamCount baseline_us=$before optimized_us=$after gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
