class Reaction {
  const Reaction(this.type, this.count, this.selected);
  final String type;
  final int count;
  final bool selected;
}

class Message {
  const Message(this.id, this.content, this.reactions);
  final String id;
  final String content;
  final List<Reaction> reactions;

  Message withReactions(List<Reaction> reactions) =>
      Message(id, content, reactions);
}

List<Message> baseline(List<Message> messages) => [
  for (final message in messages)
    message.withReactions([
      for (final reaction in message.reactions)
        Reaction(reaction.type, reaction.count, reaction.selected),
    ]),
];

List<Message> candidate(List<Message> messages, bool hasOverrides) =>
    hasOverrides ? baseline(messages) : messages;

void main() {
  final messages = [
    for (var i = 0; i < 500; i++)
      Message('message-$i', 'A representative Teams message body $i', [
        if (i % 3 == 0) const Reaction('like', 3, false),
        if (i % 7 == 0) const Reaction('heart', 1, false),
      ]),
  ];
  for (var i = 0; i < 5000; i++) {
    baseline(messages);
    candidate(messages, false);
  }
  int run(bool fast) {
    final watch = Stopwatch()..start();
    var total = 0;
    for (var i = 0; i < 5000; i++) {
      final result = fast ? candidate(messages, false) : baseline(messages);
      total += result.length + result[0].id.length;
    }
    watch.stop();
    if (total != 2545000) throw StateError('$total');
    return watch.elapsedMicroseconds;
  }

  final old = <int>[];
  final fresh = <int>[];
  for (var i = 0; i < 21; i++) {
    old.add(run(false));
    fresh.add(run(true));
  }
  old.sort();
  fresh.sort();
  final baselineUs = old[10];
  final fastUs = fresh[10];
  print(
    'baseline=${baselineUs}us fast=${fastUs}us gain=${((baselineUs - fastUs) * 100 / baselineUs).toStringAsFixed(1)}%',
  );
}
