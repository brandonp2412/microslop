part of '../workspace.dart';

class _MessageCameraScreen extends StatefulWidget {
  const _MessageCameraScreen();

  @override
  State<_MessageCameraScreen> createState() => _MessageCameraScreenState();
}

class _MessageCameraScreenState extends State<_MessageCameraScreen> {
  bool _ready = false;
  bool _capturing = false;
  Object? _error;

  @override
  void initState() {
    super.initState();
    unawaited(_start());
  }

  Future<void> _start() async {
    try {
      final ready = await PlatformCallVideo.startCamera();
      if (!mounted) {
        if (ready) await PlatformCallVideo.stopCamera();
        return;
      }
      setState(() => _ready = ready);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    }
  }

  Future<void> _capture() async {
    if (!_ready || _capturing) return;
    setState(() => _capturing = true);
    try {
      final bytes = await PlatformMessageMedia.captureCameraFrame();
      if (mounted) Navigator.of(context).pop(bytes);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _capturing = false);
    }
  }

  @override
  void dispose() {
    unawaited(PlatformCallVideo.stopCamera());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Camera')),
    body: SafeArea(
      child: Column(
        children: [
          Expanded(
            child: _ready
                ? const AndroidView(viewType: 'microslop/call_camera_preview')
                : Center(
                    child: _error == null
                        ? const CircularProgressIndicator()
                        : SelectableText(_error.toString()),
                  ),
          ),
          Padding(
            padding: const EdgeInsets.all(24),
            child: IconButton.filled(
              tooltip: 'Take photo',
              iconSize: 36,
              onPressed: _ready && !_capturing ? _capture : null,
              icon: const Icon(Icons.camera_alt_rounded),
            ),
          ),
        ],
      ),
    ),
  );
}
