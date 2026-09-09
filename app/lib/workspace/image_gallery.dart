part of '../workspace.dart';

Future<void> _imageAction(
  BuildContext context,
  MessageImage image, {
  bool copy = false,
}) async {
  try {
    if (copy) {
      await MessageClipboard.writeImage(image.bytes, image.contentType);
    } else {
      final extension = image.contentType
          .split('/')
          .last
          .replaceAll('jpeg', 'jpg');
      final path = await FilePicker.saveFile(
        dialogTitle: 'Save image',
        fileName: 'image.$extension',
        bytes: image.bytes,
      );
      if (path != null && !kIsWeb && !Platform.isAndroid && !Platform.isIOS) {
        await File(path).writeAsBytes(image.bytes);
      }
    }
  } catch (error) {
    if (context.mounted) {
      _showSnackBar(
        context,
        'Could not ${copy ? 'copy' : 'save'} image: $error',
      );
    }
  }
}

class _GalleryEntry {
  const _GalleryEntry({required this.id, this.image, this.url});
  final String id;
  final MessageImage? image;
  final String? url;
}

class _ImageGallery extends StatefulWidget {
  const _ImageGallery({
    required this.clickedKey,
    required this.clicked,
    required this.initial,
    required this.loadImages,
    required this.loadImage,
  });

  final String clickedKey;
  final MessageImage clicked;
  final List<_GalleryEntry> initial;
  final Future<List<_GalleryEntry>> Function() loadImages;
  final Future<MessageImage?> Function(String url) loadImage;

  @override
  State<_ImageGallery> createState() => _ImageGalleryState();
}

class _ImageGalleryState extends State<_ImageGallery> {
  late final PageController _pages;
  late List<_GalleryEntry> _entries = widget.initial;
  final _images = <String, MessageImage>{};
  final _loads = <String>{};
  final _failed = <String>{};
  late int _index = _entries.indexWhere(
    (entry) => entry.id == widget.clickedKey,
  );
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _pages = PageController(initialPage: _index);
    for (final entry in _entries) {
      if (entry.image case final image?) _images[entry.id] = image;
    }
    _images[widget.clickedKey] = widget.clicked;
    unawaited(_load());
  }

  Future<void> _load() async {
    try {
      final entries = await widget.loadImages();
      if (!mounted) return;
      final current = _entries[_index].id;
      final ids = entries.map((entry) => entry.id).toSet();
      for (final entry in _entries) {
        if (ids.add(entry.id)) entries.add(entry);
      }
      for (final entry in entries) {
        if (entry.image case final image?) {
          _images.putIfAbsent(entry.id, () => image);
        }
      }
      setState(() {
        _entries = entries;
        _index = entries.indexWhere((entry) => entry.id == current);
      });
      final nextIndex = _index;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _pages.jumpToPage(nextIndex);
      });
      unawaited(_loadCurrent());
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _loadCurrent() async {
    final entry = _entries[_index];
    if (_images.containsKey(entry.id) || !_loads.add(entry.id)) return;
    try {
      final image = entry.url == null
          ? null
          : await widget.loadImage(entry.url!);
      if (!mounted) return;
      setState(() {
        if (image == null) {
          _failed.add(entry.id);
        } else {
          _images[entry.id] = image;
        }
      });
    } catch (_) {
      if (mounted) setState(() => _failed.add(entry.id));
    }
  }

  void _move(int delta) {
    final next = _index + delta;
    if (next < 0 || next >= _entries.length) return;
    _pages.jumpToPage(next);
  }

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final current = _images[_entries[_index].id];
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            Navigator.pop(context),
        const SingleActivator(LogicalKeyboardKey.arrowLeft): () => _move(-1),
        const SingleActivator(LogicalKeyboardKey.arrowRight): () => _move(1),
      },
      child: Focus(
        autofocus: true,
        child: Scaffold(
          backgroundColor: Colors.black,
          appBar: AppBar(
            backgroundColor: Colors.black,
            foregroundColor: Colors.white,
            title: Text(
              '${_index + 1} / ${_entries.length}${_loading ? '+' : ''}',
            ),
            actions: [
              IconButton(
                tooltip: 'Copy image',
                onPressed: current == null
                    ? null
                    : () => _imageAction(context, current, copy: true),
                icon: const Icon(Icons.copy),
              ),
              IconButton(
                tooltip: 'Save image',
                onPressed: current == null
                    ? null
                    : () => _imageAction(context, current),
                icon: const Icon(Icons.download),
              ),
              if (_loading)
                const Padding(
                  padding: EdgeInsets.all(16),
                  child: SizedBox.square(
                    dimension: 24,
                    child: CircularProgressIndicator(),
                  ),
                ),
            ],
          ),
          body: Column(
            children: [
              if (_error != null)
                TextButton(
                  onPressed: () {
                    setState(() {
                      _loading = true;
                      _error = null;
                    });
                    unawaited(_load());
                  },
                  child: const Text('Could not load image history. Retry'),
                ),
              Expanded(
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    PageView.builder(
                      controller: _pages,
                      itemCount: _entries.length,
                      onPageChanged: (index) {
                        setState(() => _index = index);
                        unawaited(_loadCurrent());
                      },
                      itemBuilder: (context, index) {
                        final entry = _entries[index];
                        final image = _images[entry.id];
                        return GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: () => Navigator.pop(context),
                          child: image == null
                              ? Center(
                                  child: _failed.contains(entry.id)
                                      ? TextButton.icon(
                                          onPressed: () {
                                            setState(() {
                                              _loads.remove(entry.id);
                                              _failed.remove(entry.id);
                                            });
                                            unawaited(_loadCurrent());
                                          },
                                          icon: const Icon(Icons.refresh),
                                          label: const Text('Retry image'),
                                        )
                                      : const CircularProgressIndicator(),
                                )
                              : InteractiveViewer(
                                  minScale: 1,
                                  maxScale: 5,
                                  child: Center(
                                    child: GestureDetector(
                                      onTap: () {},
                                      child: Image.memory(
                                        image.bytes,
                                        fit: BoxFit.contain,
                                        errorBuilder: (_, _, _) => const Icon(
                                          Icons.broken_image_outlined,
                                          color: Colors.white,
                                        ),
                                      ),
                                    ),
                                  ),
                                ),
                        );
                      },
                    ),
                    if (_index > 0)
                      Align(
                        alignment: Alignment.centerLeft,
                        child: IconButton.filled(
                          tooltip: 'Previous image',
                          onPressed: () => _move(-1),
                          icon: const Icon(Icons.chevron_left),
                        ),
                      ),
                    if (_index + 1 < _entries.length)
                      Align(
                        alignment: Alignment.centerRight,
                        child: IconButton.filled(
                          tooltip: 'Next image',
                          onPressed: () => _move(1),
                          icon: const Icon(Icons.chevron_right),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
