part of '../workspace.dart';

class _MessageSyncState {
  const _MessageSyncState({
    this.running = false,
    this.limit = 100,
    this.completed = 0,
    this.total = 0,
    this.error,
  });

  final bool running;
  final int limit;
  final int completed;
  final int total;
  final Object? error;

  _MessageSyncState copyWith({
    bool? running,
    int? limit,
    int? completed,
    int? total,
    Object? error,
    bool clearError = false,
  }) => _MessageSyncState(
    running: running ?? this.running,
    limit: limit ?? this.limit,
    completed: completed ?? this.completed,
    total: total ?? this.total,
    error: clearError ? null : error ?? this.error,
  );
}

class _SettingsScreen extends StatefulWidget {
  const _SettingsScreen({
    required this.callGateway,
    required this.teamsGateway,
    required this.notifications,
    required this.enabled,
    required this.mentionsEnabled,
    required this.directMessagesEnabled,
    required this.groupsEnabled,
    required this.channelsEnabled,
    required this.reactionsEnabled,
    required this.linkPreviewsEnabled,
    required this.onChanged,
    required this.hiddenSectionIds,
    required this.onRestoreSection,
    required this.hiddenConversations,
    required this.onRestoreConversation,
    required this.onTestCall,
    required this.messageSync,
    required this.onSyncMessages,
    required this.onSyncMessageLimitChanged,
  });

  final DesktopNotifications notifications;
  final CallGateway callGateway;
  final TeamsGateway teamsGateway;
  final bool enabled;
  final bool mentionsEnabled;
  final bool directMessagesEnabled;
  final bool groupsEnabled;
  final bool channelsEnabled;
  final bool reactionsEnabled;
  final bool linkPreviewsEnabled;
  final Future<void> Function(
    bool enabled,
    bool mentionsEnabled,
    bool directMessagesEnabled,
    bool groupsEnabled,
    bool channelsEnabled,
    bool reactionsEnabled,
    bool linkPreviewsEnabled,
  )
  onChanged;
  final Set<String> hiddenSectionIds;
  final ValueChanged<String> onRestoreSection;
  final List<Conversation> hiddenConversations;
  final ValueChanged<Conversation> onRestoreConversation;
  final Future<void> Function() onTestCall;
  final ValueNotifier<_MessageSyncState> messageSync;
  final VoidCallback onSyncMessages;
  final ValueChanged<int> onSyncMessageLimitChanged;

  @override
  State<_SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<_SettingsScreen> {
  late bool enabled = widget.enabled;
  late bool mentionsEnabled = widget.mentionsEnabled;
  late bool directMessagesEnabled = widget.directMessagesEnabled;
  late bool groupsEnabled = widget.groupsEnabled;
  late bool channelsEnabled = widget.channelsEnabled;
  late bool reactionsEnabled = widget.reactionsEnabled;
  late bool linkPreviewsEnabled = widget.linkPreviewsEnabled;
  late final List<Conversation> hiddenConversations = [
    ...widget.hiddenConversations,
  ];
  bool _testCallRunning = false;
  bool _backgroundWatcherEnabled = true;
  bool _backgroundWatcherRunning = false;
  String _settingsQuery = '';
  final _devices = <MediaDeviceKind, List<MediaDevice>>{};
  final _selectedDevices = <MediaDeviceKind, String>{};
  final _mediaPreviewErrors = <MediaDeviceKind, String>{};
  bool _microphonePreviewActive = false;
  bool _microphonePreviewBusy = false;
  Timer? _microphonePreviewTimer;
  int? _microphonePreviewPeak;
  bool _speakerPreviewing = false;
  bool? _speakerPreviewOk;
  bool _cameraPreviewing = false;
  bool _cameraPreviewActive = false;
  bool _cameraFrameBusy = false;
  bool _nativeCameraPreviewStarted = false;
  Timer? _cameraPreviewTimer;
  final _packageInfo = PackageInfo.fromPlatform();
  ui.Image? _cameraPreviewImage;

  MediaDeviceCallGateway? get _mediaGateway =>
      widget.callGateway is MediaDeviceCallGateway
      ? widget.callGateway as MediaDeviceCallGateway
      : null;

  @override
  void initState() {
    super.initState();
    unawaited(_loadMediaDevices());
    unawaited(_loadBackgroundWatcher());
  }

  @override
  void dispose() {
    _microphonePreviewTimer?.cancel();
    _cameraPreviewTimer?.cancel();
    if (_nativeCameraPreviewStarted) {
      unawaited(PlatformCallVideo.stopCamera());
    }
    _cameraPreviewImage?.dispose();
    super.dispose();
  }

  Future<void> _loadMediaDevices() async {
    final gateway = widget.callGateway;
    final prefs = await SharedPreferences.getInstance();
    final results = await Future.wait([
      for (final kind in MediaDeviceKind.values)
        () async {
          try {
            final devices = gateway is MediaDeviceCallGateway
                ? await (gateway as MediaDeviceCallGateway).mediaDevices(kind)
                : const [MediaDevice('', 'System default')];
            return (kind, devices, null as String?);
          } catch (error) {
            return (
              kind,
              const [MediaDevice('', 'System default')],
              error.toString(),
            );
          }
        }(),
    ]);
    if (!mounted) return;
    for (final (kind, devices, error) in results) {
      final selected = prefs.getString('calls.device.${kind.name}') ?? '';
      _devices[kind] = devices;
      _selectedDevices[kind] = devices.any((device) => device.id == selected)
          ? selected
          : '';
      if (error == null) {
        _mediaPreviewErrors.remove(kind);
      } else {
        _mediaPreviewErrors[kind] = error;
      }
    }
    setState(() {});
  }

  Future<void> _selectMediaDevice(MediaDeviceKind kind, String? id) async {
    if (id == null) return;
    final previous = _selectedDevices[kind] ?? '';
    setState(() => _selectedDevices[kind] = id);
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString('calls.device.${kind.name}', id);
      await _mediaGateway?.selectMediaDevice(kind, id);
      if (!mounted) return;
      switch (kind) {
        case MediaDeviceKind.microphone:
          unawaited(_restartMicrophonePreview());
        case MediaDeviceKind.speaker:
          unawaited(_previewSpeaker());
        case MediaDeviceKind.camera:
          unawaited(_restartCameraPreview());
      }
    } catch (error) {
      if (mounted) setState(() => _selectedDevices[kind] = previous);
    }
  }

  Future<void> _startMicrophonePreview() async {
    final gateway = _mediaGateway;
    if (gateway == null || _microphonePreviewActive) return;
    if (!kIsWeb && (Platform.isAndroid || Platform.isIOS || Platform.isMacOS)) {
      final permission = await Permission.microphone.request();
      if (!mounted) return;
      if (!permission.isGranted) {
        setState(() {
          _microphonePreviewPeak = null;
          _mediaPreviewErrors[MediaDeviceKind.microphone] =
              'Microphone permission is required to test input';
        });
        return;
      }
    }
    setState(() {
      _microphonePreviewActive = true;
      _microphonePreviewPeak = null;
      _mediaPreviewErrors.remove(MediaDeviceKind.microphone);
    });
    await _refreshMicrophonePreview();
    if (!mounted || !_microphonePreviewActive) return;
    _microphonePreviewTimer?.cancel();
    _microphonePreviewTimer = Timer.periodic(
      const Duration(milliseconds: 100),
      (_) => unawaited(_refreshMicrophonePreview()),
    );
  }

  Future<void> _refreshMicrophonePreview() async {
    final gateway = _mediaGateway;
    if (gateway == null ||
        !_microphonePreviewActive ||
        _microphonePreviewBusy) {
      return;
    }
    _microphonePreviewBusy = true;
    try {
      final peak = await gateway.previewMicrophone();
      if (mounted && _microphonePreviewActive) {
        setState(() => _microphonePreviewPeak = peak);
      }
    } catch (error) {
      _microphonePreviewTimer?.cancel();
      _microphonePreviewTimer = null;
      if (mounted) {
        setState(() {
          _microphonePreviewActive = false;
          _mediaPreviewErrors[MediaDeviceKind.microphone] = error.toString();
        });
      }
    } finally {
      _microphonePreviewBusy = false;
    }
  }

  void _stopMicrophonePreview() {
    _microphonePreviewTimer?.cancel();
    _microphonePreviewTimer = null;
    if (mounted) setState(() => _microphonePreviewActive = false);
  }

  Future<void> _restartMicrophonePreview() async {
    _stopMicrophonePreview();
    await _startMicrophonePreview();
  }

  double get _microphoneLevel {
    final peak = _microphonePreviewPeak ?? 0;
    if (peak == 0) return 0;
    return ((20 * log(peak / 32768) / ln10 + 60) / 60).clamp(0.0, 1.0);
  }

  Future<void> _previewSpeaker() async {
    final gateway = _mediaGateway;
    if (gateway == null || _speakerPreviewing) return;
    setState(() {
      _speakerPreviewing = true;
      _mediaPreviewErrors.remove(MediaDeviceKind.speaker);
    });
    try {
      final ok = await gateway.previewSpeaker();
      if (mounted) {
        setState(() {
          _speakerPreviewOk = ok;
          if (!ok) {
            _mediaPreviewErrors[MediaDeviceKind.speaker] =
                'Could not open the selected output device';
          }
        });
      }
    } catch (error) {
      if (mounted) {
        setState(
          () => _mediaPreviewErrors[MediaDeviceKind.speaker] = error.toString(),
        );
      }
    } finally {
      if (mounted) setState(() => _speakerPreviewing = false);
    }
  }

  Future<void> _previewCamera() async {
    final gateway = _mediaGateway;
    if (gateway == null || _cameraPreviewing || _cameraPreviewActive) return;
    setState(() {
      _cameraPreviewing = true;
      _mediaPreviewErrors.remove(MediaDeviceKind.camera);
    });
    try {
      if (PlatformCallVideo.supported) {
        final permission = await Permission.camera.request();
        if (!mounted) return;
        if (!permission.isGranted) {
          throw StateError('Camera permission is required for the preview');
        }
        if (!await PlatformCallVideo.startCamera()) {
          throw StateError('Could not open the selected camera');
        }
        if (!mounted) {
          await PlatformCallVideo.stopCamera();
          return;
        }
        _nativeCameraPreviewStarted = true;
        setState(() => _cameraPreviewActive = true);
        return;
      }
      if (mounted) setState(() => _cameraPreviewActive = true);
      await _refreshCameraPreviewFrame();
      if (!_cameraPreviewActive || !mounted) return;
      _cameraPreviewTimer = Timer.periodic(
        const Duration(milliseconds: 33),
        (_) => unawaited(_refreshCameraPreviewFrame()),
      );
    } catch (error) {
      if (mounted) {
        setState(() {
          _cameraPreviewActive = false;
          _mediaPreviewErrors[MediaDeviceKind.camera] = error.toString();
        });
      }
    } finally {
      if (mounted) setState(() => _cameraPreviewing = false);
    }
  }

  Future<void> _refreshCameraPreviewFrame() async {
    final gateway = _mediaGateway;
    if (gateway == null || !_cameraPreviewActive || _cameraFrameBusy) return;
    _cameraFrameBusy = true;
    try {
      final frame = await gateway.previewCamera();
      if (frame == null) {
        if (mounted && _cameraPreviewImage == null) {
          setState(
            () => _mediaPreviewErrors[MediaDeviceKind.camera] =
                'Could not capture from the selected camera',
          );
        }
        return;
      }
      final completer = Completer<ui.Image>();
      ui.decodeImageFromPixels(
        frame.rgba,
        frame.width,
        frame.height,
        ui.PixelFormat.rgba8888,
        completer.complete,
      );
      final image = await completer.future;
      if (!mounted || !_cameraPreviewActive) {
        image.dispose();
        return;
      }
      final previous = _cameraPreviewImage;
      setState(() {
        _cameraPreviewImage = image;
        _mediaPreviewErrors.remove(MediaDeviceKind.camera);
      });
      previous?.dispose();
    } catch (error) {
      if (mounted) {
        setState(
          () => _mediaPreviewErrors[MediaDeviceKind.camera] = error.toString(),
        );
      }
    } finally {
      _cameraFrameBusy = false;
    }
  }

  Future<void> _stopCameraPreview() async {
    _cameraPreviewTimer?.cancel();
    _cameraPreviewTimer = null;
    if (_nativeCameraPreviewStarted) {
      await PlatformCallVideo.stopCamera();
      _nativeCameraPreviewStarted = false;
    }
    if (!mounted) return;
    setState(() => _cameraPreviewActive = false);
  }

  Future<void> _restartCameraPreview() async {
    await _stopCameraPreview();
    if (!mounted) return;
    await _previewCamera();
  }

  bool _matchesSettings(String terms) =>
      _settingsQuery.isEmpty || terms.contains(_settingsQuery);

  Future<void> _save() => widget.onChanged(
    enabled,
    mentionsEnabled,
    directMessagesEnabled,
    groupsEnabled,
    channelsEnabled,
    reactionsEnabled,
    linkPreviewsEnabled,
  );

  Widget _settingsHeading(
    BuildContext context,
    String title, {
    Widget? action,
    Widget? compactAction,
  }) {
    final scheme = Theme.of(context).colorScheme;
    final icon = switch (title) {
      'Notifications' => Icons.notifications_outlined,
      'Messages' => Icons.chat_bubble_outline_rounded,
      'Data sync' => Icons.cloud_sync_outlined,
      'Calls' => Icons.call_outlined,
      'Audio and video' => Icons.tune_rounded,
      'Debugging' => Icons.bug_report_outlined,
      _ => Icons.settings_outlined,
    };
    return LayoutBuilder(
      builder: (context, constraints) {
        final visibleAction = constraints.maxWidth < 520
            ? compactAction ?? action
            : action;
        return Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 12, 8),
          child: Row(
            children: [
              DecoratedBox(
                decoration: BoxDecoration(
                  color: scheme.secondaryContainer,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: SizedBox.square(
                  dimension: 42,
                  child: Icon(icon, color: scheme.onSecondaryContainer),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  title,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(
                    context,
                  ).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.w700),
                ),
              ),
              if (visibleAction != null) ...[
                const SizedBox(width: 8),
                visibleAction,
              ],
            ],
          ),
        );
      },
    );
  }

  Future<void> _loadBackgroundWatcher() async {
    if (!BackgroundMessageWatcher.supported) return;
    final prefs = await SharedPreferences.getInstance();
    final watcherEnabled =
        prefs.getBool('notifications.backgroundWatcher') ?? true;
    if (enabled && watcherEnabled) {
      await BackgroundMessageWatcher.setEnabled(true);
    }
    final running = await BackgroundMessageWatcher.isRunning();
    if (mounted) {
      setState(() {
        _backgroundWatcherEnabled = watcherEnabled;
        _backgroundWatcherRunning = running;
      });
    }
  }

  Future<void> _setBackgroundWatcher(bool value) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool('notifications.backgroundWatcher', value);
    await BackgroundMessageWatcher.setEnabled(value && enabled);
    final running = await BackgroundMessageWatcher.isRunning();
    if (mounted) {
      setState(() {
        _backgroundWatcherEnabled = value;
        _backgroundWatcherRunning = running;
      });
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Settings')),
    body: SafeArea(
      top: false,
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 840),
          child: ListView(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 24),
            children: [
              TextField(
                key: const ValueKey('settings-search'),
                onChanged: (value) =>
                    setState(() => _settingsQuery = value.trim().toLowerCase()),
                decoration: const InputDecoration(
                  prefixIcon: Icon(Icons.search),
                  hintText: 'Search settings',
                  filled: true,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.all(Radius.circular(16)),
                    borderSide: BorderSide.none,
                  ),
                ),
              ),
              const SizedBox(height: 16),
              if (_matchesSettings(
                'about version build flavor debug release profile',
              ))
                FutureBuilder<PackageInfo>(
                  future: _packageInfo,
                  builder: (context, snapshot) => ListTile(
                    title: const Text('Microslop'),
                    subtitle: Text(
                      [
                        if (snapshot.data case final info?)
                          '${info.version}+${info.buildNumber}',
                        kReleaseMode
                            ? 'Release'
                            : kProfileMode
                            ? 'Profile'
                            : 'Debug',
                        if (appFlavor case final flavor?) flavor,
                      ].join(' · '),
                    ),
                  ),
                ),
              if (_matchesSettings(
                'notifications app direct messages mentions group chats channels reactions real-time background',
              ))
                _settingsHeading(context, 'Notifications'),
              if (_matchesSettings(
                'notifications app notifications notify dms mentions message activity',
              ))
                SwitchListTile(
                  title: const Text('Enable notifications'),
                  subtitle: const Text(
                    'Master switch for all message notifications',
                  ),
                  value: enabled,
                  onChanged: (value) {
                    setState(() => enabled = value);
                    unawaited(_save());
                    if (BackgroundMessageWatcher.supported) {
                      unawaited(
                        BackgroundMessageWatcher.setEnabled(
                              value && _backgroundWatcherEnabled,
                            )
                            .then((_) => BackgroundMessageWatcher.isRunning())
                            .then((running) {
                              if (mounted) {
                                setState(
                                  () => _backgroundWatcherRunning = running,
                                );
                              }
                            }),
                      );
                    }
                  },
                ),
              if (_matchesSettings(
                'notifications direct messages one-to-one chats',
              ))
                SwitchListTile(
                  title: const Text('Direct messages'),
                  subtitle: const Text(
                    'Notify for new messages in one-to-one chats',
                  ),
                  value: directMessagesEnabled,
                  onChanged: enabled
                      ? (value) {
                          setState(() => directMessagesEnabled = value);
                          unawaited(_save());
                        }
                      : null,
                ),
              if (_matchesSettings(
                'notifications mentions @mentions groups channels',
              ))
                SwitchListTile(
                  title: const Text('@mentions'),
                  subtitle: const Text(
                    'Notify when you are mentioned in groups or channels',
                  ),
                  value: mentionsEnabled,
                  onChanged: enabled
                      ? (value) {
                          setState(() => mentionsEnabled = value);
                          unawaited(_save());
                        }
                      : null,
                ),
              if (_matchesSettings('notifications group chats messages'))
                SwitchListTile(
                  title: const Text('Group chats'),
                  subtitle: const Text(
                    'Notify for every message in group chats',
                  ),
                  value: groupsEnabled,
                  onChanged: enabled
                      ? (value) {
                          setState(() => groupsEnabled = value);
                          unawaited(_save());
                        }
                      : null,
                ),
              if (_matchesSettings('notifications channels messages'))
                SwitchListTile(
                  title: const Text('Channels'),
                  subtitle: const Text('Notify for every message in channels'),
                  value: channelsEnabled,
                  onChanged: enabled
                      ? (value) {
                          setState(() => channelsEnabled = value);
                          unawaited(_save());
                        }
                      : null,
                ),
              if (_matchesSettings('notifications reactions messages'))
                SwitchListTile(
                  title: const Text('Reactions'),
                  subtitle: const Text(
                    'Notify for new reactions to your messages',
                  ),
                  value: reactionsEnabled,
                  onChanged: enabled
                      ? (value) {
                          setState(() => reactionsEnabled = value);
                          unawaited(_save());
                        }
                      : null,
                ),
              if (BackgroundMessageWatcher.supported &&
                  _matchesSettings(
                    'notifications real-time background foreground service message connection android',
                  ))
                SwitchListTile(
                  title: const Text('Real-time background notifications'),
                  subtitle: Text(
                    _backgroundWatcherRunning
                        ? 'Active · Android is keeping the Teams message connection alive'
                        : 'Uses a foreground service so Android does not suspend the message connection',
                  ),
                  value: _backgroundWatcherEnabled,
                  onChanged: enabled
                      ? (value) => unawaited(_setBackgroundWatcher(value))
                      : null,
                ),
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings('messages link previews'))
                _settingsHeading(context, 'Messages'),
              if (_matchesSettings('messages link previews links preview card'))
                SwitchListTile(
                  title: const Text('Link previews'),
                  subtitle: const Text(
                    'Show a compact preview card under links in messages',
                  ),
                  value: linkPreviewsEnabled,
                  onChanged: (value) {
                    setState(() => linkPreviewsEnabled = value);
                    unawaited(_save());
                  },
                ),
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings(
                'hidden items conversations navigation sections',
              ))
                ListTile(
                  leading: const Icon(Icons.visibility_off_outlined),
                  title: const Text('Hidden items'),
                  subtitle: Text(
                    '${hiddenConversations.length + widget.hiddenSectionIds.length} hidden',
                  ),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => Navigator.of(context)
                      .push(
                        MaterialPageRoute<void>(
                          builder: (_) => _HiddenItemsScreen(
                            hiddenConversations: hiddenConversations,
                            hiddenSectionIds: widget.hiddenSectionIds,
                            onRestoreConversation: (conversation) {
                              widget.onRestoreConversation(conversation);
                              hiddenConversations.remove(conversation);
                            },
                            onRestoreSection: widget.onRestoreSection,
                            sectionTitle: _sectionTitle,
                          ),
                        ),
                      )
                      .then((_) {
                        if (mounted) setState(() {});
                      }),
                ),
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings(
                'data sync messages per conversation cache sync messages now',
              ))
                ValueListenableBuilder<_MessageSyncState>(
                  valueListenable: widget.messageSync,
                  builder: (context, sync, _) => Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _settingsHeading(
                        context,
                        'Data sync',
                        action: TextButton.icon(
                          onPressed:
                              sync.running ||
                                  widget.teamsGateway
                                      is! MessageSyncTeamsGateway
                              ? null
                              : widget.onSyncMessages,
                          icon: sync.running
                              ? const SizedBox.square(
                                  dimension: 16,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                  ),
                                )
                              : const Icon(Icons.sync_rounded),
                          label: Text(
                            sync.running
                                ? 'Syncing messages…'
                                : 'Sync messages now',
                          ),
                        ),
                        compactAction: IconButton(
                          tooltip: sync.running
                              ? 'Syncing messages'
                              : 'Sync messages now',
                          onPressed:
                              sync.running ||
                                  widget.teamsGateway
                                      is! MessageSyncTeamsGateway
                              ? null
                              : widget.onSyncMessages,
                          icon: sync.running
                              ? const SizedBox.square(
                                  dimension: 18,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                  ),
                                )
                              : const Icon(Icons.sync_rounded),
                        ),
                      ),
                      ListTile(
                        leading: const Icon(Icons.cloud_download_outlined),
                        title: const Text('Messages per conversation'),
                        subtitle: Text(
                          'Cache the latest ${sync.limit} messages in every chat and channel',
                        ),
                        trailing: DropdownButton<int>(
                          value: sync.limit,
                          items: const [25, 50, 100, 250, 500]
                              .map(
                                (value) => DropdownMenuItem(
                                  value: value,
                                  child: Text('$value'),
                                ),
                              )
                              .toList(),
                          onChanged: sync.running
                              ? null
                              : (value) {
                                  if (value != null) {
                                    widget.onSyncMessageLimitChanged(value);
                                  }
                                },
                        ),
                      ),
                      if (sync.running || sync.total > 0) ...[
                        Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 16),
                          child: LinearProgressIndicator(
                            value: sync.total == 0
                                ? 0
                                : sync.completed / sync.total,
                          ),
                        ),
                        const SizedBox(height: 8),
                        Text(
                          sync.running
                              ? '${sync.completed} of ${sync.total} conversations cached'
                              : 'Cached ${sync.completed} of ${sync.total} conversations',
                          textAlign: TextAlign.center,
                        ),
                      ],
                      if (sync.error != null)
                        Padding(
                          padding: const EdgeInsets.all(16),
                          child: SelectableText('Sync failed: ${sync.error}'),
                        ),
                    ],
                  ),
                ),
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings('calls make test call bot recording'))
                _settingsHeading(context, 'Calls'),
              if (_matchesSettings('calls make test call bot recording'))
                ListTile(
                  leading: const Icon(Icons.graphic_eq_outlined),
                  title: const Text('Make a test call'),
                  subtitle: Text(
                    _testCallRunning
                        ? 'Test call in progress…'
                        : 'Talk to the bot and hear your recording played back',
                  ),
                  trailing: _testCallRunning
                      ? const SizedBox.square(
                          dimension: 24,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.chevron_right),
                  onTap: _testCallRunning ? null : _makeTestCall,
                ),
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings(
                'audio video microphone speaker headset camera devices',
              ))
                _settingsHeading(context, 'Audio and video'),
              if (_matchesSettings('audio video microphone device')) ...[
                _MediaDeviceSelector(
                  icon: Icons.mic_outlined,
                  title: 'Microphone',
                  devices: _devices[MediaDeviceKind.microphone],
                  selected: _selectedDevices[MediaDeviceKind.microphone],
                  onChanged: (id) =>
                      _selectMediaDevice(MediaDeviceKind.microphone, id),
                ),
                ListTile(
                  key: const ValueKey('microphone-preview'),
                  dense: true,
                  onTap: _microphonePreviewActive
                      ? _stopMicrophonePreview
                      : () => unawaited(_startMicrophonePreview()),
                  leading: const Icon(Icons.graphic_eq_rounded),
                  title: LayoutBuilder(
                    builder: (context, constraints) {
                      return TweenAnimationBuilder<double>(
                        tween: Tween(end: _microphoneLevel),
                        duration: const Duration(milliseconds: 90),
                        builder: (context, animatedValue, _) => ClipRRect(
                          borderRadius: BorderRadius.circular(999),
                          child: LinearProgressIndicator(
                            key: const ValueKey('microphone-level'),
                            value: animatedValue,
                            minHeight: 8,
                          ),
                        ),
                      );
                    },
                  ),
                  subtitle: Text(
                    _mediaPreviewErrors[MediaDeviceKind.microphone] ??
                        (_microphonePreviewActive
                            ? _microphonePreviewPeak == null
                                  ? 'Listening…'
                                  : _microphonePreviewPeak == 0
                                  ? 'Listening · no input detected'
                                  : 'Listening · ${(((_microphonePreviewPeak! / 32768) * 100).round()).clamp(1, 100)}% current level'
                            : _microphonePreviewPeak == null
                            ? 'Select a microphone or tap Test for a live level meter'
                            : _microphonePreviewPeak == 0
                            ? 'Stopped · no input detected'
                            : 'Stopped · last level ${(((_microphonePreviewPeak! / 32768) * 100).round()).clamp(1, 100)}%'),
                  ),
                ),
              ],
              if (_matchesSettings('audio video speaker headset device')) ...[
                _MediaDeviceSelector(
                  icon: Icons.headset_outlined,
                  title: 'Speaker or headset',
                  devices: _devices[MediaDeviceKind.speaker],
                  selected: _selectedDevices[MediaDeviceKind.speaker],
                  onChanged: (id) =>
                      _selectMediaDevice(MediaDeviceKind.speaker, id),
                ),
                ListTile(
                  key: const ValueKey('speaker-preview'),
                  dense: true,
                  onTap: _speakerPreviewing
                      ? null
                      : () => unawaited(_previewSpeaker()),
                  leading: const Icon(Icons.volume_up_outlined),
                  title: const Text('Speaker preview'),
                  subtitle: Text(
                    _mediaPreviewErrors[MediaDeviceKind.speaker] ??
                        (_speakerPreviewing
                            ? 'Playing test tone…'
                            : _speakerPreviewOk == true
                            ? 'Test tone played through the selected output'
                            : 'Play a short tone through this output'),
                  ),
                ),
              ],
              if (_matchesSettings('audio video camera device')) ...[
                _MediaDeviceSelector(
                  icon: Icons.videocam_outlined,
                  title: 'Camera',
                  devices: _devices[MediaDeviceKind.camera],
                  selected: _selectedDevices[MediaDeviceKind.camera],
                  onChanged: (id) =>
                      _selectMediaDevice(MediaDeviceKind.camera, id),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      ListTile(
                        key: const ValueKey('camera-preview'),
                        contentPadding: EdgeInsets.zero,
                        title: const Text('Camera preview'),
                        enabled: !_cameraPreviewing,
                        onTap: _cameraPreviewing
                            ? null
                            : () => unawaited(
                                _cameraPreviewActive
                                    ? _stopCameraPreview()
                                    : _previewCamera(),
                              ),
                        trailing: Icon(
                          _cameraPreviewActive
                              ? Icons.stop_rounded
                              : Icons.play_arrow_rounded,
                        ),
                      ),
                      Center(
                        child: ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 420),
                          child: AspectRatio(
                            aspectRatio: 16 / 9,
                            child: DecoratedBox(
                              decoration: BoxDecoration(
                                color: Colors.black,
                                borderRadius: BorderRadius.circular(12),
                              ),
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(12),
                                child:
                                    PlatformCallVideo.supported &&
                                        _cameraPreviewActive
                                    ? const AndroidView(
                                        viewType:
                                            'microslop/call_camera_preview',
                                      )
                                    : _cameraPreviewImage != null
                                    ? RawImage(
                                        image: _cameraPreviewImage,
                                        fit: BoxFit.cover,
                                      )
                                    : Center(
                                        child: _cameraPreviewing
                                            ? const CircularProgressIndicator()
                                            : Icon(
                                                Icons.videocam_off_outlined,
                                                color: Colors.white.withValues(
                                                  alpha: .7,
                                                ),
                                              ),
                                      ),
                              ),
                            ),
                          ),
                        ),
                      ),
                      if (_mediaPreviewErrors[MediaDeviceKind.camera]
                          case final error?)
                        Padding(
                          padding: const EdgeInsets.only(top: 6),
                          child: Text(
                            error,
                            style: TextStyle(
                              color: Theme.of(context).colorScheme.error,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ],
              if (_settingsQuery.isEmpty) const Divider(height: 32),
              if (_matchesSettings(
                'debugging send test notification notification history debug log',
              ))
                _settingsHeading(context, 'Debugging'),
              if (_matchesSettings('debugging send test notification'))
                ListTile(
                  leading: const Icon(Icons.notifications_active_outlined),
                  title: const Text('Send test notification'),
                  onTap: _sendTestNotification,
                ),
              if (kDebugMode &&
                  _matchesSettings('debugging notification history'))
                ValueListenableBuilder<List<String>>(
                  valueListenable: NotificationHistory.entries,
                  builder: (context, entries, _) => ExpansionTile(
                    title: const Text('Notification history'),
                    children: [
                      for (final entry in entries.reversed)
                        ListTile(title: Text(entry)),
                    ],
                  ),
                ),
              if (_matchesSettings('debugging debug log application log'))
                ValueListenableBuilder<List<String>>(
                  valueListenable: AppLog.entries,
                  builder: (context, entries, _) => ListTile(
                    leading: const Icon(Icons.bug_report_outlined),
                    title: const Text('Debug log'),
                    subtitle: Text(
                      entries.isEmpty
                          ? 'No recorded entries'
                          : '${entries.length} recorded ${entries.length == 1 ? 'entry' : 'entries'}',
                    ),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) =>
                            _ErrorLogScreen(entries: List.of(entries)),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    ),
  );

  Future<void> _makeTestCall() async {
    _stopMicrophonePreview();
    setState(() => _testCallRunning = true);
    try {
      await widget.onTestCall();
    } catch (error) {
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Test call failed'),
          content: Text(error.toString()),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Done'),
            ),
          ],
        ),
      );
    } finally {
      if (mounted) setState(() => _testCallRunning = false);
    }
  }

  Future<void> _sendTestNotification() async {
    try {
      await widget.notifications.show(
        title: 'Microslop test notification',
        body: 'Notifications are working.',
      );
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('Test notification sent.')));
    } catch (error, stackTrace) {
      AppLog.record('Test notification', error, stackTrace);
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Notification failed: $error')));
    }
  }

  String _sectionTitle(String id) => switch (id) {
    _TeamsWorkspaceState._channelsSectionId => 'Channels',
    _TeamsWorkspaceState._groupsSectionId => 'Groups',
    _ => 'Direct messages',
  };
}
