class ImageItem {
  const ImageItem(this.sourceUrl);
  final String? sourceUrl;
}

int baseline(List<ImageItem> images, List<String> urls) {
  var count = images.length;
  for (var index = 0; index < urls.length; index++) {
    if (!images.any((image) => image.sourceUrl == urls[index]) &&
        !(index < images.length && images[index].sourceUrl == null)) {
      count++;
    }
  }
  return count;
}

int indexed(List<ImageItem> images, List<String> urls) {
  final sourceUrls = {
    for (final image in images)
      if (image.sourceUrl != null) image.sourceUrl,
  };
  var count = images.length;
  for (var index = 0; index < urls.length; index++) {
    if (!sourceUrls.contains(urls[index]) &&
        !(index < images.length && images[index].sourceUrl == null)) {
      count++;
    }
  }
  return count;
}

int measure(
  int Function(List<ImageItem>, List<String>) run,
  List<ImageItem> images,
  List<String> urls,
) {
  final watch = Stopwatch()..start();
  var checksum = 0;
  for (var iteration = 0; iteration < 20000; iteration++) {
    checksum ^= run(images, urls);
  }
  watch.stop();
  if (checksum < 0) throw StateError('invalid');
  return watch.elapsedMicroseconds;
}

int percentile(List<int> values, double percentile) {
  values.sort();
  return values[((values.length - 1) * percentile).round()];
}

void main() {
  final images = List.generate(
    96,
    (index) => ImageItem('https://example.test/$index.jpg'),
  );
  final urls = List.generate(
    128,
    (index) => 'https://example.test/${index % 110}.jpg',
  );
  final before = List.generate(15, (_) => measure(baseline, images, urls));
  final after = List.generate(15, (_) => measure(indexed, images, urls));
  final beforeP50 = percentile(before, .5);
  final afterP50 = percentile(after, .5);
  final beforeP95 = percentile(before, .95);
  final afterP95 = percentile(after, .95);
  print(
    'p50_us=$beforeP50->$afterP50 gain=${((beforeP50 - afterP50) * 100 / beforeP50).toStringAsFixed(1)}% '
    'p95_us=$beforeP95->$afterP95 gain=${((beforeP95 - afterP95) * 100 / beforeP95).toStringAsFixed(1)}%',
  );
}
