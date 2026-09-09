final terms = [
  'about version build flavor debug release profile',
  'notifications app direct messages mentions group chats channels reactions real-time background',
  'notifications app notifications notify dms mentions message activity',
  'notifications direct messages one-to-one chats',
  'notifications mentions @mentions groups channels',
  'notifications group chats messages',
  'notifications channels messages',
  'notifications reactions messages',
  'notifications real-time background foreground service message connection android',
  'messages link previews',
  'messages link previews links preview card',
  'hidden items conversations navigation sections',
  'data sync messages per conversation cache sync messages now',
  'calls make test call bot recording',
  'audio video microphone speaker headset camera devices',
  'audio video microphone device',
  'audio video speaker headset device',
  'audio video camera device',
  'debugging send test notification notification history debug log',
  'debugging send test notification',
  'debugging notification history',
  'debugging debug log application log',
];

int baseline(String rawQuery) {
  var matches = 0;
  for (final term in terms) {
    final query = rawQuery.trim().toLowerCase();
    if (query.isEmpty || term.toLowerCase().contains(query)) matches++;
  }
  return matches;
}

int optimized(String query) {
  var matches = 0;
  for (final term in terms) {
    if (query.isEmpty || term.contains(query)) matches++;
  }
  return matches;
}

int measure(int Function() run, int iterations) {
  final stopwatch = Stopwatch()..start();
  var checksum = 0;
  for (var i = 0; i < iterations; i++) checksum += run();
  stopwatch.stop();
  if (checksum == -1) throw StateError('unreachable');
  return stopwatch.elapsedMicroseconds;
}

int median(List<int> values) {
  values.sort();
  return values[values.length ~/ 2];
}

void main() {
  for (final rawQuery in ['', '  Notification  ', 'CAMERA']) {
    final normalized = rawQuery.trim().toLowerCase();
    final before = <int>[];
    final after = <int>[];
    for (var sample = 0; sample < 9; sample++) {
      before.add(measure(() => baseline(rawQuery), 100000));
      after.add(measure(() => optimized(normalized), 100000));
    }
    final baselineUs = median(before);
    final optimizedUs = median(after);
    final gain = (baselineUs - optimizedUs) / baselineUs * 100;
    print(
      'query=${rawQuery.isEmpty ? '<empty>' : rawQuery.trim()} '
      'baseline_us=$baselineUs optimized_us=$optimizedUs gain=${gain.toStringAsFixed(1)}%',
    );
  }
}
