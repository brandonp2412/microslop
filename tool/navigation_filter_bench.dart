class Chat {
  Chat(this.name, this.preview, this.isGroup);
  final String name;
  final String? preview;
  final bool isGroup;
}

class Team {
  Team(this.name, this.channels);
  final String name;
  final List<Chat> channels;
}

({
  List<Chat> chats,
  List<Chat> groups,
  List<Chat> directMessages,
  List<Team> teams,
})
baseline(List<Chat> sourceChats, List<Team> sourceTeams, String rawQuery) {
  final query = rawQuery.trim().toLowerCase();
  final chats = sourceChats.where((chat) {
    return query.isEmpty ||
        chat.name.toLowerCase().contains(query) ||
        (chat.preview?.toLowerCase().contains(query) ?? false);
  }).toList();
  final groups = chats.where((chat) => chat.isGroup).toList();
  final directMessages = chats.where((chat) => !chat.isGroup).toList();
  final teams = sourceTeams
      .map((team) {
        final teamMatches = team.name.toLowerCase().contains(query);
        final channels = team.channels
            .where(
              (channel) =>
                  query.isEmpty ||
                  teamMatches ||
                  channel.name.toLowerCase().contains(query),
            )
            .toList();
        return Team(team.name, channels);
      })
      .where((team) => team.channels.isNotEmpty)
      .toList();
  return (
    chats: chats,
    groups: groups,
    directMessages: directMessages,
    teams: teams,
  );
}

({
  List<Chat> chats,
  List<Chat> groups,
  List<Chat> directMessages,
  List<Team> teams,
})
optimized(List<Chat> sourceChats, List<Team> sourceTeams, String rawQuery) {
  final query = rawQuery.trim().toLowerCase();
  if (query.isNotEmpty) return baseline(sourceChats, sourceTeams, rawQuery);
  final groups = <Chat>[];
  final directMessages = <Chat>[];
  for (final chat in sourceChats) {
    (chat.isGroup ? groups : directMessages).add(chat);
  }
  return (
    chats: sourceChats,
    groups: groups,
    directMessages: directMessages,
    teams: sourceTeams,
  );
}

int run(
  ({
    List<Chat> chats,
    List<Chat> groups,
    List<Chat> directMessages,
    List<Team> teams,
  })
  Function(List<Chat>, List<Team>, String)
  fn,
  List<Chat> chats,
  List<Team> teams,
  String query,
  int iterations,
) {
  var checksum = 0;
  final watch = Stopwatch()..start();
  for (var i = 0; i < iterations; i++) {
    final result = fn(chats, teams, query);
    checksum += result.chats.length + result.teams.length;
  }
  watch.stop();
  if (checksum == 0) throw StateError('bad benchmark');
  return watch.elapsedMicroseconds;
}

void main() {
  final chats = List.generate(
    4000,
    (i) => Chat(
      'Conversation $i Person ${i % 97}',
      'Preview message $i lorem ipsum',
      i % 5 == 0,
    ),
  );
  final teams = List.generate(
    80,
    (i) => Team(
      'Team $i',
      List.generate(30, (j) => Chat('Channel $i-$j', null, true)),
    ),
  );
  const iterations = 1500;
  for (final query in ['', 'person 42']) {
    for (var warmup = 0; warmup < 3; warmup++) {
      run(baseline, chats, teams, query, 50);
      run(optimized, chats, teams, query, 50);
    }
    final baselineSamples = <int>[];
    final optimizedSamples = <int>[];
    for (var sample = 0; sample < 7; sample++) {
      baselineSamples.add(run(baseline, chats, teams, query, iterations));
      optimizedSamples.add(run(optimized, chats, teams, query, iterations));
    }
    baselineSamples.sort();
    optimizedSamples.sort();
    final b = baselineSamples[baselineSamples.length ~/ 2];
    final o = optimizedSamples[optimizedSamples.length ~/ 2];
    final gain = (b - o) * 100 / b;
    print(
      'query=${query.isEmpty ? '<empty>' : query} baseline_us=$b optimized_us=$o gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
