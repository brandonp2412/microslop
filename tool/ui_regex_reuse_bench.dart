final presencePattern = RegExp(
  r'(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])',
);
final videoPattern = RegExp(r'/video\d+$');

String baselinePresence(String value) => value.replaceAllMapped(
  RegExp(r'(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])'),
  (_) => ' ',
);

String optimizedPresence(String value) =>
    value.replaceAllMapped(presencePattern, (_) => ' ');

bool baselineVideo(String path) => RegExp(r'/video\d+$').hasMatch(path);
bool optimizedVideo(String path) => videoPattern.hasMatch(path);

int runPresence(String Function(String) fn, int rounds) {
  const values = ['AvailableIdle', 'DoNotDisturb', 'BeRightBack', 'Offline'];
  final watch = Stopwatch()..start();
  var total = 0;
  for (var i = 0; i < rounds; i++) {
    total += fn(values[i % values.length]).length;
  }
  watch.stop();
  if (total == 0) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

int runVideo(bool Function(String) fn, int rounds) {
  const paths = [
    '/dev/video0',
    '/dev/video12',
    '/dev/null',
    '/dev/snd/controlC0',
  ];
  final watch = Stopwatch()..start();
  var matches = 0;
  for (var i = 0; i < rounds; i++) {
    if (fn(paths[i % paths.length])) matches++;
  }
  watch.stop();
  if (matches == 0) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  const rounds = 500000;
  runPresence(baselinePresence, 1000);
  runPresence(optimizedPresence, 1000);
  runVideo(baselineVideo, 1000);
  runVideo(optimizedVideo, 1000);
  final oldPresence = runPresence(baselinePresence, rounds);
  final newPresence = runPresence(optimizedPresence, rounds);
  final oldVideo = runVideo(baselineVideo, rounds);
  final newVideo = runVideo(optimizedVideo, rounds);
  print(
    'presence baseline_us=$oldPresence optimized_us=$newPresence gain=${((oldPresence - newPresence) * 100 / oldPresence).toStringAsFixed(1)}%',
  );
  print(
    'video baseline_us=$oldVideo optimized_us=$newVideo gain=${((oldVideo - newVideo) * 100 / oldVideo).toStringAsFixed(1)}%',
  );
}
