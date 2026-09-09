import 'dart:math';

(String, String?) baseline(String sender, String? senderId) => (
  sender.trim().isEmpty ? '?' : sender.trim(),
  senderId?.trim().isEmpty == true ? null : senderId?.trim(),
);

(String, String?) optimized(String sender, String? senderId) {
  final trimmedSender = sender.trim();
  final trimmedSenderId = senderId?.trim();
  return (
    trimmedSender.isEmpty ? '?' : trimmedSender,
    trimmedSenderId?.isEmpty == true ? null : trimmedSenderId,
  );
}

int measure(
  (String, String?) Function(String, String?) run,
  List<String> senders,
) {
  final stopwatch = Stopwatch()..start();
  var checksum = 0;
  for (var iteration = 0; iteration < 50000; iteration++) {
    for (final sender in senders) {
      final result = run(sender, '  8:orgid:0123456789abcdef  ');
      checksum += result.$1.length + (result.$2?.length ?? 0);
    }
  }
  if (checksum == Random(0).nextInt(1)) throw StateError('unreachable');
  return stopwatch.elapsedMicroseconds;
}

int median(List<int> values) {
  values.sort();
  return values[values.length ~/ 2];
}

void main() {
  final senders = ['  Ada Lovelace  ', ' Grace Hopper ', '   ', 'brandonp2412'];
  final before = <int>[];
  final after = <int>[];
  for (var sample = 0; sample < 9; sample++) {
    before.add(measure(baseline, senders));
    after.add(measure(optimized, senders));
  }
  final baselineUs = median(before);
  final optimizedUs = median(after);
  final gain = (baselineUs - optimizedUs) / baselineUs * 100;
  print(
    'baseline_us=$baselineUs optimized_us=$optimizedUs gain=${gain.toStringAsFixed(1)}%',
  );
}
