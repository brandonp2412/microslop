part of '../workspace.dart';

class _AudioRecordingSheet extends StatefulWidget {
  const _AudioRecordingSheet();

  @override
  State<_AudioRecordingSheet> createState() => _AudioRecordingSheetState();
}

class _AudioRecordingSheetState extends State<_AudioRecordingSheet> {
  late final DateTime _started = DateTime.now();
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final seconds = DateTime.now().difference(_started).inSeconds;
    final elapsed =
        '${(seconds ~/ 60).toString().padLeft(2, '0')}:${(seconds % 60).toString().padLeft(2, '0')}';
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.mic_rounded, size: 40),
            const SizedBox(height: 12),
            Text('Recording · $elapsed'),
            const SizedBox(height: 24),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton(
                    onPressed: () => Navigator.of(context).pop(false),
                    child: const Text('Cancel'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: FilledButton.icon(
                    onPressed: () => Navigator.of(context).pop(true),
                    icon: const Icon(Icons.send_rounded),
                    label: const Text('Stop & send'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
