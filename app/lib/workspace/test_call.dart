part of '../workspace.dart';

class _TestCallScreen extends StatefulWidget {
  const _TestCallScreen({required this.callGateway});

  final CallGateway callGateway;

  @override
  State<_TestCallScreen> createState() => _TestCallScreenState();
}

class _TestCallScreenState extends State<_TestCallScreen> {
  StreamSubscription<CallUpdate>? _events;
  Timer? _ticker;
  CallUpdateKind? _kind;
  DateTime? _connectedAt;
  TestCallResult? _result;
  Object? _error;
  bool _starting = true;
  bool _ending = false;
  bool _mediaControlBusy = false;
  bool _microphoneEnabled = true;
  bool _speakerEnabled = true;
  bool _cameraEnabled = true;
  bool _cameraStarted = false;
  bool _cameraVerified = false;
  MediaDeviceKind? _selectingDevice;
  final _devices = <MediaDeviceKind, List<MediaDevice>>{};
  final _selectedDevices = <MediaDeviceKind, String>{};

  bool get _callInProgress =>
      _result == null &&
      _error == null &&
      (_starting || _kind != CallUpdateKind.ended);

  bool get _desktopCameraPreview =>
      !kIsWeb &&
      (Platform.isWindows || Platform.isLinux) &&
      widget.callGateway is MediaDeviceCallGateway;

  bool get _showCameraPreview =>
      _cameraEnabled && (_cameraStarted || _desktopCameraPreview);

  @override
  void initState() {
    super.initState();
    _events = widget.callGateway.events().listen(
      _onCallUpdate,
      onError: (Object error) {
        unawaited(PlatformCallAudio.reset());
        if (mounted) setState(() => _error = error);
      },
    );
    unawaited(_beginLocalCallAudio());
    WidgetsBinding.instance.addPostFrameCallback((_) => _start());
  }

  Future<void> _beginLocalCallAudio() async {
    try {
      await PlatformCallAudio.setSpeakerphone(true);
    } catch (error, stackTrace) {
      AppLog.record('Start local call audio', error, stackTrace);
    }
  }

  Future<void> _loadMediaDevices() async {
    final gateway = widget.callGateway is MediaDeviceCallGateway
        ? widget.callGateway as MediaDeviceCallGateway
        : null;
    if (gateway == null) return;
    final prefs = await SharedPreferences.getInstance();
    for (final kind in MediaDeviceKind.values) {
      List<MediaDevice> devices;
      try {
        devices = await gateway.mediaDevices(kind);
      } catch (error) {
        AppLog.record('List ${kind.name} devices for test call', error);
        devices = const [MediaDevice('', 'System default')];
      }
      final saved = prefs.getString('calls.device.${kind.name}') ?? '';
      final selected = devices.any((device) => device.id == saved) ? saved : '';
      try {
        await gateway.selectMediaDevice(kind, selected);
      } catch (error) {
        AppLog.record('Select ${kind.name} for test call', error);
      }
      if (!mounted) return;
      setState(() {
        _devices[kind] = devices;
        _selectedDevices[kind] = selected;
      });
    }
  }

  Future<void> _selectMediaDevice(MediaDeviceKind kind, String? id) async {
    final gateway = widget.callGateway is MediaDeviceCallGateway
        ? widget.callGateway as MediaDeviceCallGateway
        : null;
    if (gateway == null || id == null || _selectingDevice != null) {
      return;
    }
    final previous = _selectedDevices[kind] ?? '';
    setState(() {
      _selectingDevice = kind;
      _selectedDevices[kind] = id;
      if (kind == MediaDeviceKind.camera) _cameraVerified = false;
    });
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString('calls.device.${kind.name}', id);
      await gateway.selectMediaDevice(kind, id);
      if (!mounted) return;
      if (kind == MediaDeviceKind.camera && PlatformCallVideo.supported) {
        if (_cameraStarted) await PlatformCallVideo.stopCamera();
        final cameraStarted = await PlatformCallVideo.startCamera();
        if (!mounted) {
          if (cameraStarted) await PlatformCallVideo.stopCamera();
          return;
        }
        _cameraStarted = cameraStarted;
        _cameraVerified = cameraStarted;
      }
    } catch (error) {
      if (!mounted) return;
      setState(() => _selectedDevices[kind] = previous);
      _showSnackBar(context, 'Could not select ${kind.name}: $error');
    } finally {
      if (mounted) setState(() => _selectingDevice = null);
    }
  }

  Future<void> _start() async {
    try {
      await _loadMediaDevices();
      if (!mounted) return;
      if (PlatformCallVideo.supported && _cameraEnabled) {
        final cameraStarted = await PlatformCallVideo.startCamera();
        if (!mounted) {
          if (cameraStarted) await PlatformCallVideo.stopCamera();
          return;
        }
        _cameraStarted = cameraStarted;
        _cameraVerified = cameraStarted;
      }
      await widget.callGateway.startCall(microsoftTestCallConversationId);
    } catch (error) {
      unawaited(PlatformCallAudio.reset());
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _starting = false);
    }
  }

  void _onCallUpdate(CallUpdate update) {
    if (update.conversationId != microsoftTestCallConversationId || !mounted) {
      return;
    }
    AppLog.debug(
      'Test call screen',
      '${update.kind.name} callId=${update.callId} ${update.detail ?? ''}',
    );
    if (update.kind == CallUpdateKind.error) {
      _ticker?.cancel();
      unawaited(PlatformCallAudio.reset());
      if (_cameraStarted) unawaited(PlatformCallVideo.stopCamera());
      setState(() {
        _cameraStarted = false;
        _kind = update.kind;
        _ending = false;
        _error = update.detail ?? 'The Microsoft test call failed.';
      });
      return;
    }
    if (update.kind == CallUpdateKind.ended) {
      _ticker?.cancel();
      unawaited(PlatformCallAudio.reset());
      if (_cameraStarted) unawaited(PlatformCallVideo.stopCamera());
      setState(() {
        _cameraStarted = false;
        _kind = update.kind;
        _ending = false;
        _result = parseMicrosoftTestCallResult(update.detail);
      });
      return;
    }
    if (update.kind == CallUpdateKind.connected && _connectedAt == null) {
      unawaited(PlatformCallAudio.stopRingback());
      _connectedAt = DateTime.now();
      _ticker = Timer.periodic(const Duration(seconds: 1), (_) {
        if (mounted) setState(() {});
      });
    }
    setState(() => _kind = update.kind);
  }

  Future<void> _hangUp() async {
    if (_ending || !_callInProgress) return;
    setState(() => _ending = true);
    try {
      if (_cameraStarted) {
        await PlatformCallVideo.stopCamera();
        _cameraStarted = false;
      }
      await widget.callGateway.hangUp();
    } catch (error) {
      if (mounted) {
        setState(() {
          _ending = false;
          _error = error;
        });
      }
    }
  }

  Future<void> _setMicrophoneEnabled(bool enabled) async {
    if (_mediaControlBusy || !_callInProgress) return;
    final previous = _microphoneEnabled;
    setState(() {
      _mediaControlBusy = true;
      _microphoneEnabled = enabled;
    });
    try {
      await widget.callGateway.setMicrophoneEnabled(enabled);
    } catch (error) {
      if (!mounted) return;
      setState(() => _microphoneEnabled = previous);
      _showSnackBar(context, 'Could not change microphone state: $error');
    } finally {
      if (mounted) setState(() => _mediaControlBusy = false);
    }
  }

  Future<void> _setCameraEnabled(bool enabled) async {
    if (_mediaControlBusy || !_callInProgress) return;
    setState(() => _mediaControlBusy = true);
    try {
      if (PlatformCallVideo.supported) {
        if (enabled) {
          final cameraStarted = await PlatformCallVideo.startCamera();
          if (!mounted) {
            if (cameraStarted) await PlatformCallVideo.stopCamera();
            return;
          }
          _cameraStarted = cameraStarted;
          _cameraVerified = cameraStarted;
        } else if (_cameraStarted) {
          await PlatformCallVideo.stopCamera();
          _cameraStarted = false;
          _cameraVerified = false;
        }
      }
      if (mounted) setState(() => _cameraEnabled = enabled);
    } catch (error) {
      if (!mounted) return;
      _showSnackBar(context, 'Could not change camera state: $error');
    } finally {
      if (mounted) setState(() => _mediaControlBusy = false);
    }
  }

  Future<void> _setSpeakerEnabled(bool enabled) async {
    if (_mediaControlBusy || !_callInProgress) return;
    final previous = _speakerEnabled;
    setState(() {
      _mediaControlBusy = true;
      _speakerEnabled = enabled;
    });
    try {
      if (PlatformCallAudio.supportsSpeakerRouting) {
        await PlatformCallAudio.setSpeakerphone(enabled);
      } else {
        await widget.callGateway.setSpeakerEnabled(enabled);
      }
    } catch (error) {
      if (!mounted) return;
      setState(() => _speakerEnabled = previous);
      _showSnackBar(context, 'Could not change audio output: $error');
    } finally {
      if (mounted) setState(() => _mediaControlBusy = false);
    }
  }

  String get _durationLabel {
    final connectedAt = _connectedAt;
    if (connectedAt == null) return '00:00';
    final elapsed = DateTime.now().difference(connectedAt);
    final minutes = elapsed.inMinutes.toString().padLeft(2, '0');
    final seconds = (elapsed.inSeconds % 60).toString().padLeft(2, '0');
    return '$minutes:$seconds';
  }

  String get _statusLabel {
    if (_ending) return 'Ending call…';
    if (_error != null) return 'Call failed';
    if (_result != null) return 'Test call complete';
    if (_starting && _kind == null) return 'Preparing test call…';
    return switch (_kind) {
      CallUpdateKind.dialing => 'Starting test call…',
      CallUpdateKind.ringing => 'Calling Microsoft Test Call Bot…',
      CallUpdateKind.connected => 'Connected · $_durationLabel',
      _ => 'Connecting…',
    };
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _events?.cancel();
    unawaited(PlatformCallAudio.reset());
    if (_cameraStarted) unawaited(PlatformCallVideo.stopCamera());
    if (_callInProgress) unawaited(widget.callGateway.hangUp());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final connected = _kind == CallUpdateKind.connected && _result == null;
    final result = _result;
    return PopScope(
      canPop: !_callInProgress,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && _callInProgress) unawaited(_hangUp());
      },
      child: Scaffold(
        appBar: AppBar(
          title: const Text('Test call'),
          automaticallyImplyLeading: !_callInProgress,
        ),
        body: SafeArea(
          top: false,
          child: ListView(
            padding: const EdgeInsets.fromLTRB(24, 28, 24, 24),
            children: [
              const SizedBox(height: 16),
              if (_showCameraPreview)
                Center(
                  child: SizedBox(
                    width: 220,
                    child: AspectRatio(
                      aspectRatio: 4 / 3,
                      child: ClipRRect(
                        borderRadius: const BorderRadius.all(
                          Radius.circular(24),
                        ),
                        child: PlatformCallVideo.supported
                            ? const AndroidView(
                                viewType: 'microslop/call_camera_preview',
                              )
                            : _TestCameraView(
                                gateway:
                                    widget.callGateway
                                        as MediaDeviceCallGateway,
                                onFrame: () {
                                  if (mounted && !_cameraVerified) {
                                    setState(() => _cameraVerified = true);
                                  }
                                },
                              ),
                      ),
                    ),
                  ),
                )
              else
                CircleAvatar(
                  radius: 62,
                  backgroundColor: scheme.primaryContainer,
                  foregroundColor: scheme.onPrimaryContainer,
                  child: const Icon(Icons.graphic_eq_rounded, size: 58),
                ),
              const SizedBox(height: 24),
              Text(
                'Microsoft Test Call Bot',
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.headlineSmall,
              ),
              const SizedBox(height: 8),
              Text(
                _statusLabel,
                style: Theme.of(context).textTheme.titleMedium?.copyWith(
                  color: scheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: 24),
              if (widget.callGateway is MediaDeviceCallGateway)
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 620),
                  child: Card(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Padding(
                            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                            child: Text(
                              'Device settings',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                          ),
                          _MediaDeviceSelector(
                            icon: Icons.mic_outlined,
                            title: 'Microphone',
                            devices: _devices[MediaDeviceKind.microphone],
                            selected:
                                _selectedDevices[MediaDeviceKind.microphone],
                            onChanged: (id) => _selectMediaDevice(
                              MediaDeviceKind.microphone,
                              id,
                            ),
                          ),
                          _MediaDeviceSelector(
                            icon: Icons.headset_outlined,
                            title: 'Speaker or headset',
                            devices: _devices[MediaDeviceKind.speaker],
                            selected: _selectedDevices[MediaDeviceKind.speaker],
                            onChanged: (id) =>
                                _selectMediaDevice(MediaDeviceKind.speaker, id),
                          ),
                          _MediaDeviceSelector(
                            icon: Icons.videocam_outlined,
                            title: 'Camera',
                            devices: _devices[MediaDeviceKind.camera],
                            selected: _selectedDevices[MediaDeviceKind.camera],
                            onChanged: (id) =>
                                _selectMediaDevice(MediaDeviceKind.camera, id),
                          ),
                          if (_selectingDevice != null)
                            const Padding(
                              padding: EdgeInsets.fromLTRB(16, 0, 16, 12),
                              child: LinearProgressIndicator(),
                            ),
                        ],
                      ),
                    ),
                  ),
                ),
              const SizedBox(height: 18),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 520),
                child: Card(
                  child: Padding(
                    padding: const EdgeInsets.all(20),
                    child: result != null
                        ? Column(
                            children: [
                              Icon(
                                result.passed
                                    ? Icons.check_circle_outline
                                    : Icons.error_outline,
                                size: 34,
                                color: result.passed
                                    ? scheme.primary
                                    : scheme.error,
                              ),
                              const SizedBox(height: 12),
                              Text(
                                result.passed
                                    ? 'Audio diagnostics passed'
                                    : 'Audio diagnostics failed',
                                textAlign: TextAlign.center,
                                style: Theme.of(context).textTheme.titleMedium,
                              ),
                              const SizedBox(height: 8),
                              Text(
                                result.passed
                                    ? 'Live microphone audio was captured and Microsoft audio was rendered to the selected speaker. The full test only passes if you also heard your recording played back.'
                                    : (result.rejectionReason ??
                                          'The test did not capture microphone audio and render Microsoft audio through the selected speaker.'),
                                textAlign: TextAlign.center,
                              ),
                              const SizedBox(height: 14),
                              Text(
                                '${result.packetsSent} sent · ${result.packetsReceived} received · ${result.microphoneNonSilentFrames} live mic frames · peak ${result.microphonePeak}',
                                textAlign: TextAlign.center,
                                style: Theme.of(context).textTheme.bodySmall,
                              ),
                              Text(
                                '${result.speakerFramesReceived} speaker frames · ${result.speakerSamplesRendered} samples rendered',
                                textAlign: TextAlign.center,
                                style: Theme.of(context).textTheme.bodySmall,
                              ),
                              if (result.videoPacketsSent > 0 ||
                                  result.videoPacketsReceived > 0)
                                Text(
                                  '${result.videoPacketsSent} video packets sent · ${result.videoPacketsReceived} received',
                                  textAlign: TextAlign.center,
                                  style: Theme.of(context).textTheme.bodySmall,
                                ),
                              Text(
                                _cameraVerified
                                    ? 'Camera preview opened successfully'
                                    : 'Camera not verified · no live preview was available',
                                textAlign: TextAlign.center,
                                style: Theme.of(context).textTheme.bodySmall
                                    ?.copyWith(
                                      color: _cameraVerified
                                          ? scheme.primary
                                          : scheme.error,
                                    ),
                              ),
                            ],
                          )
                        : _error != null
                        ? Column(
                            children: [
                              Icon(
                                Icons.error_outline,
                                size: 34,
                                color: scheme.error,
                              ),
                              const SizedBox(height: 12),
                              SelectableText(
                                _error.toString(),
                                textAlign: TextAlign.center,
                              ),
                              const SizedBox(height: 12),
                              OutlinedButton.icon(
                                onPressed: () => Clipboard.setData(
                                  ClipboardData(text: _error.toString()),
                                ),
                                icon: const Icon(Icons.copy_rounded),
                                label: const Text('Copy error'),
                              ),
                            ],
                          )
                        : Text(
                            connected
                                ? _showCameraPreview
                                      ? 'Listen to the bot and speak when prompted. The camera preview above stays live locally while Microsoft verifies your microphone and speaker.'
                                      : 'Listen to the bot and speak when prompted for a few seconds. It will record you and play your voice back through the speaker.'
                                : _showCameraPreview
                                ? 'This calls Microsoft’s real Test Call Bot for audio while keeping a live local camera preview above.'
                                : 'This calls Microsoft’s real Test Call Bot. Your microphone and speaker will become active when the call connects.',
                            textAlign: TextAlign.center,
                          ),
                  ),
                ),
              ),
              const SizedBox(height: 36),
              if (_callInProgress) ...[
                Wrap(
                  alignment: WrapAlignment.center,
                  spacing: 16,
                  runSpacing: 12,
                  children: [
                    _TestCallDeviceControl(
                      icon: _microphoneEnabled
                          ? Icons.mic_rounded
                          : Icons.mic_off_rounded,
                      label: _microphoneEnabled ? 'Mic on' : 'Muted',
                      active: _microphoneEnabled,
                      busy: _mediaControlBusy,
                      onPressed: () =>
                          _setMicrophoneEnabled(!_microphoneEnabled),
                    ),
                    _TestCallDeviceControl(
                      icon: _cameraEnabled
                          ? Icons.videocam_rounded
                          : Icons.videocam_off_rounded,
                      label: _cameraEnabled ? 'Camera' : 'Camera off',
                      active: _cameraEnabled,
                      busy: _mediaControlBusy,
                      onPressed: () => _setCameraEnabled(!_cameraEnabled),
                    ),
                    _TestCallDeviceControl(
                      icon: _speakerEnabled
                          ? Icons.volume_up_rounded
                          : Icons.phone_in_talk_rounded,
                      label: _speakerEnabled
                          ? 'Speaker'
                          : PlatformCallAudio.supportsSpeakerRouting
                          ? 'Earpiece'
                          : 'Speaker off',
                      active: _speakerEnabled,
                      busy: _mediaControlBusy,
                      onPressed: () => _setSpeakerEnabled(!_speakerEnabled),
                    ),
                  ],
                ),
                const SizedBox(height: 28),
                FilledButton.icon(
                  style: FilledButton.styleFrom(
                    backgroundColor: scheme.error,
                    foregroundColor: scheme.onError,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 28,
                      vertical: 16,
                    ),
                  ),
                  onPressed: _ending ? null : _hangUp,
                  icon: const Icon(Icons.call_end_rounded),
                  label: Text(_ending ? 'Ending…' : 'Hang up'),
                ),
              ] else
                FilledButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Done'),
                ),
              const SizedBox(height: 16),
            ],
          ),
        ),
      ),
    );
  }
}

class _TestCameraView extends StatefulWidget {
  const _TestCameraView({required this.gateway, required this.onFrame});

  final MediaDeviceCallGateway gateway;
  final VoidCallback onFrame;

  @override
  State<_TestCameraView> createState() => _TestCameraViewState();
}

class _TestCameraViewState extends State<_TestCameraView> {
  Timer? _timer;
  ui.Image? _image;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    _timer = Timer.periodic(
      const Duration(milliseconds: 33),
      (_) => _refresh(),
    );
  }

  Future<void> _refresh() async {
    if (_loading) return;
    _loading = true;
    try {
      final frame = await widget.gateway.previewCamera();
      if (frame == null || !mounted) return;
      ui.decodeImageFromPixels(
        frame.rgba,
        frame.width,
        frame.height,
        ui.PixelFormat.rgba8888,
        (image) {
          if (!mounted) {
            image.dispose();
            return;
          }
          final previous = _image;
          setState(() => _image = image);
          previous?.dispose();
          widget.onFrame();
        },
      );
    } finally {
      _loading = false;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    _image?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ColoredBox(
    color: Colors.black,
    child: _image == null
        ? const Center(
            child: Icon(Icons.videocam_off_outlined, color: Colors.white54),
          )
        : RawImage(image: _image, fit: BoxFit.cover),
  );
}

class _TestCallDeviceControl extends StatelessWidget {
  const _TestCallDeviceControl({
    required this.icon,
    required this.label,
    required this.active,
    required this.busy,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final bool active;
  final bool busy;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final button = active
        ? FilledButton.tonalIcon(
            onPressed: busy ? null : onPressed,
            icon: Icon(icon),
            label: Text(label),
          )
        : OutlinedButton.icon(
            onPressed: busy ? null : onPressed,
            icon: Icon(icon),
            label: Text(label),
          );
    return button;
  }
}

class _MediaDeviceSelector extends StatelessWidget {
  const _MediaDeviceSelector({
    required this.icon,
    required this.title,
    required this.devices,
    required this.selected,
    required this.onChanged,
  });

  final IconData icon;
  final String title;
  final List<MediaDevice>? devices;
  final String? selected;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final available = <String, MediaDevice>{
        '': const MediaDevice('', 'System default'),
        for (final device in devices ?? const <MediaDevice>[])
          device.id: device,
      };
      final dropdown = DropdownButton<String>(
        isExpanded: true,
        value: available.containsKey(selected) ? selected : '',
        items: [
          for (final device in available.values)
            DropdownMenuItem(
              value: device.id,
              child: Text(device.label, overflow: TextOverflow.ellipsis),
            ),
        ],
        onChanged: devices == null ? null : onChanged,
      );
      if (constraints.maxWidth < 520) {
        return ListTile(
          leading: Icon(icon),
          title: Text(title),
          subtitle: dropdown,
        );
      }
      return ListTile(
        leading: Icon(icon),
        title: Text(title),
        trailing: SizedBox(width: 220, child: dropdown),
      );
    },
  );
}
