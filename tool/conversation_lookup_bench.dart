class Conversation {
  const Conversation(this.id);
  final String id;
}

class Team {
  const Team(this.channels);
  final List<Conversation> channels;
}

Conversation? baseline(List<Conversation> chats, List<Team> teams, String id) {
  final conversations = [...chats, for (final team in teams) ...team.channels];
  for (final conversation in conversations) {
    if (conversation.id == id) return conversation;
  }
  return null;
}

Conversation? optimized(List<Conversation> chats, List<Team> teams, String id) {
  for (final conversation in chats) {
    if (conversation.id == id) return conversation;
  }
  for (final team in teams) {
    for (final conversation in team.channels) {
      if (conversation.id == id) return conversation;
    }
  }
  return null;
}

int measure(Conversation? Function() run, int iterations) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) {
    checksum += run()?.id.length ?? 0;
  }
  watch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return watch.elapsedMicroseconds;
}

void main() {
  final chats = List.generate(10000, (index) => Conversation('chat-$index'));
  final teams = List.generate(
    20,
    (team) => Team(
      List.generate(500, (channel) => Conversation('channel-$team-$channel')),
    ),
  );
  for (final target in ['chat-2', 'chat-5000', 'channel-2-100', 'missing']) {
    final beforeSamples = <int>[];
    final afterSamples = <int>[];
    for (var sample = 0; sample < 7; sample++) {
      beforeSamples.add(measure(() => baseline(chats, teams, target), 1000));
      afterSamples.add(measure(() => optimized(chats, teams, target), 1000));
    }
    beforeSamples.sort();
    afterSamples.sort();
    final before = beforeSamples[beforeSamples.length ~/ 2];
    final after = afterSamples[afterSamples.length ~/ 2];
    final gain = (before - after) / before * 100;
    print(
      'target=$target baseline_us=$before optimized_us=$after gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
