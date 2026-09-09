final pattern = RegExp(r'^[0-9a-fA-F-]{32,36}$');

bool baseline(String name) =>
    name == 'Chat' ||
    name == 'Group chat' ||
    RegExp(r'^[0-9a-fA-F-]{32,36}$').hasMatch(name);

bool optimized(String name) =>
    name == 'Chat' || name == 'Group chat' || pattern.hasMatch(name);

int run(bool Function(String) fn, int rounds) {
  const names = [
    'Chat',
    'Group chat',
    'eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee',
    'brandonp2412',
    'Project group',
    '1234567890abcdef1234567890abcdef',
  ];
  final watch = Stopwatch()..start();
  var matches = 0;
  for (var i = 0; i < rounds; i++) {
    if (fn(names[i % names.length])) matches++;
  }
  watch.stop();
  if (matches == 0) throw StateError('unexpected');
  return watch.elapsedMicroseconds;
}

void main() {
  const rounds = 1000000;
  run(baseline, 1000);
  run(optimized, 1000);
  final old = run(baseline, rounds);
  final next = run(optimized, rounds);
  print(
    'baseline_us=$old optimized_us=$next gain=${((old - next) * 100 / old).toStringAsFixed(1)}%',
  );
}
