Future<int> load(int delayMs) async {
  await Future<void>.delayed(Duration(milliseconds: delayMs));
  return delayMs;
}

Future<int> sequential(List<int> delays) async {
  var total = 0;
  for (final delay in delays) {
    total += await load(delay);
  }
  return total;
}

Future<int> parallel(List<int> delays) async =>
    (await Future.wait(delays.map(load)))
        .fold<int>(0, (sum, value) => sum + value);

Future<int> run(Future<int> Function(List<int>) fn, List<int> delays) async {
  final watch = Stopwatch()..start();
  final total = await fn(delays);
  watch.stop();
  if (total != delays.fold(0, (sum, value) => sum + value)) {
    throw StateError('unexpected');
  }
  return watch.elapsedMicroseconds;
}

Future<void> main() async {
  const delays = [20, 25, 15];
  await sequential(delays);
  await parallel(delays);
  final old = <int>[];
  final next = <int>[];
  for (var i = 0; i < 9; i++) {
    old.add(await run(sequential, delays));
    next.add(await run(parallel, delays));
  }
  old.sort();
  next.sort();
  final oldP50 = old[old.length ~/ 2];
  final newP50 = next[next.length ~/ 2];
  final oldP95 = old[(old.length * .95).ceil() - 1];
  final newP95 = next[(next.length * .95).ceil() - 1];
  print(
    'p50_us=$oldP50->$newP50 gain=${((oldP50 - newP50) * 100 / oldP50).toStringAsFixed(1)}%',
  );
  print(
    'p95_us=$oldP95->$newP95 gain=${((oldP95 - newP95) * 100 / oldP95).toStringAsFixed(1)}%',
  );
}
