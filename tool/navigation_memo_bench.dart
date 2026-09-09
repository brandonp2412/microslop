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

({List<Chat> chats, List<Team> teams}) filter(
  List<Chat> chats,
  List<Team> teams,
  String query,
) {
  final filteredChats = chats
      .where(
        (chat) =>
            chat.name.toLowerCase().contains(query) ||
            (chat.preview?.toLowerCase().contains(query) ?? false),
      )
      .toList();
  final filteredTeams = teams
      .map((team) {
        final teamMatches = team.name.toLowerCase().contains(query);
        return Team(
          team.name,
          team.channels
              .where(
                (channel) =>
                    teamMatches || channel.name.toLowerCase().contains(query),
              )
              .toList(),
        );
      })
      .where((team) => team.channels.isNotEmpty)
      .toList();
  return (chats: filteredChats, teams: filteredTeams);
}

int baseline(List<Chat> chats, List<Team> teams, String query) {
  var checksum = 0;
  for (var i = 0; i < 5000; i++) {
    final result = filter(chats, teams, query);
    checksum ^= result.chats.length + result.teams.length;
  }
  return checksum;
}

int memoized(List<Chat> chats, List<Team> teams, String query) {
  final result = filter(chats, teams, query);
  var checksum = 0;
  for (var i = 0; i < 5000; i++) {
    checksum ^= result.chats.length + result.teams.length;
  }
  return checksum;
}

int measure(
  int Function(List<Chat>, List<Team>, String) run,
  List<Chat> chats,
  List<Team> teams,
  String query,
) {
  final watch = Stopwatch()..start();
  run(chats, teams, query);
  watch.stop();
  return watch.elapsedMicroseconds;
}

int percentile(List<int> values, double percentile) {
  values.sort();
  return values[((values.length - 1) * percentile).round()];
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
  const query = 'person 42';
  final before = List.generate(
    15,
    (_) => measure(baseline, chats, teams, query),
  );
  final after = List.generate(
    15,
    (_) => measure(memoized, chats, teams, query),
  );
  final beforeP50 = percentile(before, .5);
  final afterP50 = percentile(after, .5);
  final beforeP95 = percentile(before, .95);
  final afterP95 = percentile(after, .95);
  print(
    'p50_us=$beforeP50->$afterP50 gain=${((beforeP50 - afterP50) * 100 / beforeP50).toStringAsFixed(1)}% '
    'p95_us=$beforeP95->$afterP95 gain=${((beforeP95 - afterP95) * 100 / beforeP95).toStringAsFixed(1)}%',
  );
}
