part of '../workspace.dart';

class _ActiveCallScreen extends StatefulWidget {
  const _ActiveCallScreen({
    required this.call,
    required this.displayName,
    required this.busy,
    required this.video,
    required this.callGateway,
    required this.onAccept,
    required this.onDecline,
    required this.onHangUp,
  });

  final CallUpdate call;
  final String displayName;
  final bool busy;
  final bool video;
  final CallGateway callGateway;
  final VoidCallback onAccept;
  final VoidCallback onDecline;
  final VoidCallback onHangUp;

  @override
  State<_ActiveCallScreen> createState() => _ActiveCallScreenState();
}

class _ActiveCallScreenState extends State<_ActiveCallScreen> {
  bool _microphoneEnabled = true;
  bool _speakerEnabled = true;
  String? _changingControl;
  String? _callAction;
  DateTime? _connectedAt;
  Timer? _durationTicker;
  late String _initials;

  Future<void> _toggleMicrophone() async {
    if (_changingControl != null) return;
    final previous = _microphoneEnabled;
    final next = !previous;
    setState(() {
      _changingControl = 'microphone';
      _microphoneEnabled = next;
    });
    try {
      await widget.callGateway.setMicrophoneEnabled(next);
    } catch (error) {
      if (!mounted) return;
      setState(() => _microphoneEnabled = previous);
      _showSnackBar(context, 'Could not change microphone: $error');
    } finally {
      if (mounted) setState(() => _changingControl = null);
    }
  }

  Future<void> _toggleSpeaker() async {
    if (_changingControl != null) return;
    final previous = _speakerEnabled;
    final next = !previous;
    setState(() {
      _changingControl = 'speaker';
      _speakerEnabled = next;
    });
    try {
      if (PlatformCallAudio.supportsSpeakerRouting) {
        await PlatformCallAudio.setSpeakerphone(next);
      } else {
        await widget.callGateway.setSpeakerEnabled(next);
      }
    } catch (error) {
      if (!mounted) return;
      setState(() => _speakerEnabled = previous);
      _showSnackBar(context, 'Could not change audio output: $error');
    } finally {
      if (mounted) setState(() => _changingControl = null);
    }
  }

  @override
  void initState() {
    super.initState();
    _initials = _computeInitials();
    _syncConnectedTimer();
  }

  @override
  void didUpdateWidget(covariant _ActiveCallScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.displayName != widget.displayName) {
      _initials = _computeInitials();
    }
    if (oldWidget.call.kind != widget.call.kind) {
      _callAction = null;
      _syncConnectedTimer();
    } else if (oldWidget.busy && !widget.busy) {
      _callAction = null;
    }
  }

  void _syncConnectedTimer() {
    if (widget.call.kind == CallUpdateKind.connected) {
      _connectedAt ??= DateTime.now();
      _durationTicker ??= Timer.periodic(const Duration(seconds: 1), (_) {
        if (mounted) setState(() {});
      });
    } else if (_connectedAt != null) {
      _durationTicker?.cancel();
      _durationTicker = null;
      _connectedAt = null;
    }
  }

  String get _durationLabel {
    final connectedAt = _connectedAt;
    if (connectedAt == null) return '';
    final elapsed = DateTime.now().difference(connectedAt);
    final minutes = elapsed.inMinutes.toString().padLeft(2, '0');
    final seconds = (elapsed.inSeconds % 60).toString().padLeft(2, '0');
    return '$minutes:$seconds';
  }

  String _computeInitials() {
    final words = widget.displayName
        .trim()
        .split(_whitespacePattern)
        .where((word) => word.isNotEmpty)
        .toList();
    if (words.isEmpty) return '?';
    if (words.length == 1) return words.first.characters.first.toUpperCase();
    return '${words.first.characters.first}${words.last.characters.first}'
        .toUpperCase();
  }

  void _runCallAction(String action, VoidCallback callback) {
    if (widget.busy) return;
    setState(() => _callAction = action);
    callback();
  }

  @override
  void dispose() {
    _durationTicker?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final incoming = widget.call.kind == CallUpdateKind.incoming;
    final connected = widget.call.kind == CallUpdateKind.connected;
    final status = switch (widget.call.kind) {
      CallUpdateKind.incoming =>
        widget.video ? 'Incoming video call' : 'Incoming call',
      CallUpdateKind.dialing => 'Calling…',
      CallUpdateKind.ringing => 'Ringing…',
      CallUpdateKind.connected => 'Connected · $_durationLabel',
      CallUpdateKind.ended => 'Call ended',
      CallUpdateKind.error => 'Call failed',
    };
    final size = MediaQuery.sizeOf(context);
    final compact = size.width < 360 || size.height < 620;
    final wide = size.width >= 720;
    final remoteVideoVisible =
        widget.video && connected && PlatformCallVideo.remoteSupported;
    final showLocalPreview =
        widget.video &&
        (PlatformCallVideo.supported || (!kIsWeb && Platform.isWindows)) &&
        !incoming &&
        widget.call.conversationId != null;
    final identityColor = widget.video ? Colors.white : scheme.onSurface;
    final secondaryColor = widget.video
        ? Colors.white.withValues(alpha: .78)
        : scheme.onSurfaceVariant;
    final avatarSize = compact
        ? 116.0
        : wide
        ? 164.0
        : 148.0;
    final previewWidth = compact
        ? 86.0
        : wide
        ? 132.0
        : 108.0;
    final statusIcon = switch (widget.call.kind) {
      CallUpdateKind.incoming => Icons.call_received_rounded,
      CallUpdateKind.dialing => Icons.call_made_rounded,
      CallUpdateKind.ringing => Icons.notifications_active_rounded,
      CallUpdateKind.connected => Icons.call_rounded,
      CallUpdateKind.ended => Icons.call_end_rounded,
      CallUpdateKind.error => Icons.error_outline_rounded,
    };
    return Scaffold(
      backgroundColor: widget.video ? Colors.black : scheme.surface,
      body: SafeArea(
        child: Stack(
          children: [
            Positioned.fill(
              child: remoteVideoVisible
                  ? Platform.isAndroid
                        ? const AndroidView(
                            viewType: 'microslop/call_remote_video',
                          )
                        : const _WindowsRemoteVideoView()
                  : DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: widget.video
                            ? const LinearGradient(
                                begin: Alignment.topCenter,
                                end: Alignment.bottomCenter,
                                colors: [Color(0xFF151515), Colors.black],
                              )
                            : RadialGradient(
                                center: const Alignment(0, -.28),
                                radius: 1.05,
                                colors: [
                                  scheme.primaryContainer.withValues(
                                    alpha: .46,
                                  ),
                                  scheme.surface,
                                ],
                              ),
                      ),
                      child: Center(
                        child: Padding(
                          padding: EdgeInsets.fromLTRB(
                            24,
                            compact ? 24 : 40,
                            24,
                            compact ? 124 : 150,
                          ),
                          child: ConstrainedBox(
                            constraints: const BoxConstraints(maxWidth: 520),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Container(
                                  width: avatarSize + 18,
                                  height: avatarSize + 18,
                                  padding: const EdgeInsets.all(7),
                                  decoration: BoxDecoration(
                                    shape: BoxShape.circle,
                                    border: Border.all(
                                      color: widget.video
                                          ? Colors.white.withValues(alpha: .16)
                                          : scheme.primary.withValues(
                                              alpha: .18,
                                            ),
                                    ),
                                  ),
                                  child: Container(
                                    decoration: BoxDecoration(
                                      shape: BoxShape.circle,
                                      color: widget.video
                                          ? Colors.white.withValues(alpha: .12)
                                          : scheme.primaryContainer,
                                      boxShadow: [
                                        BoxShadow(
                                          color: scheme.primary.withValues(
                                            alpha: .12,
                                          ),
                                          blurRadius: 28,
                                          spreadRadius: 2,
                                        ),
                                      ],
                                    ),
                                    alignment: Alignment.center,
                                    child: Text(
                                      _initials,
                                      style: Theme.of(context)
                                          .textTheme
                                          .displaySmall
                                          ?.copyWith(
                                            color: widget.video
                                                ? Colors.white
                                                : scheme.onPrimaryContainer,
                                            fontWeight: FontWeight.w600,
                                            letterSpacing: 1,
                                          ),
                                    ),
                                  ),
                                ),
                                SizedBox(height: compact ? 20 : 28),
                                Text(
                                  widget.displayName,
                                  textAlign: TextAlign.center,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style:
                                      (compact
                                              ? Theme.of(
                                                  context,
                                                ).textTheme.headlineSmall
                                              : Theme.of(
                                                  context,
                                                ).textTheme.headlineMedium)
                                          ?.copyWith(
                                            color: identityColor,
                                            fontWeight: FontWeight.w600,
                                            height: 1.12,
                                          ),
                                ),
                                const SizedBox(height: 12),
                                Semantics(
                                  liveRegion: true,
                                  child: AnimatedSwitcher(
                                    duration: const Duration(milliseconds: 180),
                                    child: Row(
                                      key: ValueKey(status),
                                      mainAxisSize: MainAxisSize.min,
                                      children: [
                                        Icon(
                                          statusIcon,
                                          size: 18,
                                          color: secondaryColor,
                                        ),
                                        const SizedBox(width: 7),
                                        Flexible(
                                          child: Text(
                                            status,
                                            textAlign: TextAlign.center,
                                            style: Theme.of(context)
                                                .textTheme
                                                .titleMedium
                                                ?.copyWith(
                                                  color: secondaryColor,
                                                ),
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                                const SizedBox(height: 10),
                                Container(
                                  padding: const EdgeInsets.symmetric(
                                    horizontal: 10,
                                    vertical: 5,
                                  ),
                                  decoration: BoxDecoration(
                                    color: widget.video
                                        ? Colors.white.withValues(alpha: .1)
                                        : scheme.surfaceContainerHighest,
                                    borderRadius: BorderRadius.circular(999),
                                  ),
                                  child: Text(
                                    widget.video ? 'Video call' : 'Audio call',
                                    style: Theme.of(context)
                                        .textTheme
                                        .labelMedium
                                        ?.copyWith(color: secondaryColor),
                                  ),
                                ),
                                if (widget.video && !connected) ...[
                                  const SizedBox(height: 10),
                                  Text(
                                    incoming
                                        ? 'Your camera stays off when you answer'
                                        : 'Your camera is on',
                                    textAlign: TextAlign.center,
                                    style: Theme.of(context)
                                        .textTheme
                                        .bodyMedium
                                        ?.copyWith(color: secondaryColor),
                                  ),
                                ],
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
            ),
            if (widget.video) ...[
              const Positioned(
                top: 0,
                left: 0,
                right: 0,
                height: 150,
                child: IgnorePointer(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [Color(0x99000000), Color(0x00000000)],
                      ),
                    ),
                  ),
                ),
              ),
              const Positioned(
                left: 0,
                right: 0,
                bottom: 0,
                height: 190,
                child: IgnorePointer(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.bottomCenter,
                        end: Alignment.topCenter,
                        colors: [Color(0xB3000000), Color(0x00000000)],
                      ),
                    ),
                  ),
                ),
              ),
            ],
            if (remoteVideoVisible)
              Positioned(
                left: compact ? 14 : 20,
                right: showLocalPreview ? previewWidth + 42 : 20,
                top: compact ? 12 : 18,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      widget.displayName,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.titleLarge?.copyWith(
                        color: Colors.white,
                        fontWeight: FontWeight.w600,
                        shadows: const [Shadow(blurRadius: 8)],
                      ),
                    ),
                    const SizedBox(height: 6),
                    Semantics(
                      liveRegion: true,
                      child: AnimatedSwitcher(
                        duration: const Duration(milliseconds: 180),
                        child: Container(
                          key: ValueKey(status),
                          padding: const EdgeInsets.symmetric(
                            horizontal: 9,
                            vertical: 5,
                          ),
                          decoration: BoxDecoration(
                            color: Colors.black.withValues(alpha: .38),
                            borderRadius: BorderRadius.circular(999),
                            border: Border.all(
                              color: Colors.white.withValues(alpha: .12),
                            ),
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(statusIcon, size: 14, color: Colors.white70),
                              const SizedBox(width: 5),
                              Flexible(
                                child: Text(
                                  status,
                                  overflow: TextOverflow.ellipsis,
                                  style: Theme.of(context).textTheme.labelMedium
                                      ?.copyWith(color: Colors.white),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            if (showLocalPreview)
              Positioned(
                top: compact ? 12 : 18,
                right: compact ? 12 : 16,
                child: Container(
                  width: previewWidth,
                  height: previewWidth * 4 / 3,
                  decoration: BoxDecoration(
                    color: Colors.black,
                    borderRadius: BorderRadius.circular(compact ? 15 : 18),
                    border: Border.all(
                      color: Colors.white.withValues(alpha: .3),
                    ),
                    boxShadow: const [
                      BoxShadow(
                        color: Color(0x66000000),
                        blurRadius: 18,
                        offset: Offset(0, 6),
                      ),
                    ],
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      PlatformCallVideo.supported
                          ? const AndroidView(
                              viewType: 'microslop/call_camera_preview',
                            )
                          : const _WindowsLocalVideoView(),
                      Positioned(
                        left: 7,
                        bottom: 7,
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 7,
                            vertical: 3,
                          ),
                          decoration: BoxDecoration(
                            color: Colors.black.withValues(alpha: .55),
                            borderRadius: BorderRadius.circular(999),
                          ),
                          child: Text(
                            'You',
                            style: Theme.of(context).textTheme.labelSmall
                                ?.copyWith(color: Colors.white),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            Positioned(
              left: compact ? 10 : 16,
              right: compact ? 10 : 16,
              bottom: compact ? 10 : 16,
              child: Center(
                child: ConstrainedBox(
                  constraints: BoxConstraints(maxWidth: incoming ? 320 : 440),
                  child: Container(
                    padding: EdgeInsets.symmetric(
                      horizontal: compact ? 8 : 14,
                      vertical: compact ? 10 : 14,
                    ),
                    decoration: BoxDecoration(
                      color: widget.video
                          ? Colors.black.withValues(alpha: .64)
                          : scheme.surfaceContainerHigh,
                      borderRadius: BorderRadius.circular(compact ? 24 : 30),
                      border: Border.all(
                        color: widget.video
                            ? Colors.white.withValues(alpha: .14)
                            : scheme.outlineVariant.withValues(alpha: .55),
                      ),
                      boxShadow: [
                        BoxShadow(
                          color: Colors.black.withValues(alpha: .16),
                          blurRadius: 24,
                          offset: const Offset(0, 8),
                        ),
                      ],
                    ),
                    child: incoming
                        ? Row(
                            mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                            children: [
                              _CallControl(
                                key: const ValueKey('call-decline'),
                                icon: Icons.call_end_rounded,
                                label: _callAction == 'decline'
                                    ? 'Declining…'
                                    : 'Decline',
                                destructive: true,
                                darkSurface: widget.video,
                                compact: compact,
                                busy: widget.busy && _callAction == 'decline',
                                onPressed: widget.busy
                                    ? null
                                    : () => _runCallAction(
                                        'decline',
                                        widget.onDecline,
                                      ),
                              ),
                              _CallControl(
                                key: const ValueKey('call-accept'),
                                icon: widget.video
                                    ? Icons.videocam_rounded
                                    : Icons.call_rounded,
                                label: _callAction == 'accept'
                                    ? 'Answering…'
                                    : 'Accept',
                                positive: true,
                                darkSurface: widget.video,
                                compact: compact,
                                busy: widget.busy && _callAction == 'accept',
                                onPressed: widget.busy
                                    ? null
                                    : () => _runCallAction(
                                        'accept',
                                        widget.onAccept,
                                      ),
                              ),
                            ],
                          )
                        : Row(
                            mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                            children: [
                              _CallControl(
                                key: const ValueKey('call-microphone'),
                                icon: _microphoneEnabled
                                    ? Icons.mic_rounded
                                    : Icons.mic_off_rounded,
                                label: _microphoneEnabled ? 'Mute' : 'Unmute',
                                selected: !_microphoneEnabled,
                                darkSurface: widget.video,
                                compact: compact,
                                busy: _changingControl == 'microphone',
                                onPressed: _changingControl != null
                                    ? null
                                    : () => unawaited(_toggleMicrophone()),
                              ),
                              _CallControl(
                                key: const ValueKey('call-hang-up'),
                                icon: Icons.call_end_rounded,
                                label: _callAction == 'hangup'
                                    ? 'Ending…'
                                    : 'Hang up',
                                destructive: true,
                                darkSurface: widget.video,
                                compact: compact,
                                busy: widget.busy && _callAction == 'hangup',
                                onPressed: widget.busy
                                    ? null
                                    : () => _runCallAction(
                                        'hangup',
                                        widget.onHangUp,
                                      ),
                              ),
                              _CallControl(
                                key: const ValueKey('call-speaker'),
                                icon: PlatformCallAudio.supportsSpeakerRouting
                                    ? (_speakerEnabled
                                          ? Icons.volume_up_rounded
                                          : Icons.phone_in_talk_rounded)
                                    : (_speakerEnabled
                                          ? Icons.volume_up_rounded
                                          : Icons.volume_off_rounded),
                                label: PlatformCallAudio.supportsSpeakerRouting
                                    ? (_speakerEnabled ? 'Speaker' : 'Earpiece')
                                    : (_speakerEnabled ? 'Audio' : 'Muted'),
                                selected:
                                    PlatformCallAudio.supportsSpeakerRouting
                                    ? _speakerEnabled
                                    : !_speakerEnabled,
                                darkSurface: widget.video,
                                compact: compact,
                                busy: _changingControl == 'speaker',
                                onPressed: _changingControl != null
                                    ? null
                                    : () => unawaited(_toggleSpeaker()),
                              ),
                            ],
                          ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _WindowsRemoteVideoView extends StatefulWidget {
  const _WindowsRemoteVideoView();

  @override
  State<_WindowsRemoteVideoView> createState() =>
      _WindowsRemoteVideoViewState();
}

class _WindowsRemoteVideoViewState extends State<_WindowsRemoteVideoView> {
  Timer? _timer;
  ui.Image? _image;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    _timer = Timer.periodic(
      const Duration(milliseconds: 66),
      (_) => _refresh(),
    );
  }

  Future<void> _refresh() async {
    if (_loading) return;
    _loading = true;
    try {
      final frame = await PlatformCallVideo.remoteFrame();
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
        ? const SizedBox.expand()
        : RawImage(image: _image, fit: BoxFit.contain),
  );
}

class _WindowsLocalVideoView extends StatefulWidget {
  const _WindowsLocalVideoView();

  @override
  State<_WindowsLocalVideoView> createState() => _WindowsLocalVideoViewState();
}

class _WindowsLocalVideoViewState extends State<_WindowsLocalVideoView> {
  Timer? _timer;
  ui.Image? _image;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    _timer = Timer.periodic(
      const Duration(milliseconds: 66),
      (_) => _refresh(),
    );
  }

  Future<void> _refresh() async {
    if (_loading) return;
    _loading = true;
    try {
      final frame = await PlatformCallVideo.localFrame();
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

class _CallControl extends StatelessWidget {
  const _CallControl({
    super.key,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.selected = false,
    this.destructive = false,
    this.positive = false,
    this.darkSurface = false,
    this.compact = false,
    this.busy = false,
  });

  final IconData icon;
  final String label;
  final VoidCallback? onPressed;
  final bool selected;
  final bool destructive;
  final bool positive;
  final bool darkSurface;
  final bool compact;
  final bool busy;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final backgroundColor = destructive
        ? scheme.error
        : positive
        ? const Color(0xFF107C10)
        : selected
        ? (darkSurface ? Colors.white : scheme.primary)
        : (darkSurface
              ? Colors.white.withValues(alpha: .14)
              : scheme.surfaceContainerHighest);
    final foregroundColor = destructive
        ? scheme.onError
        : positive
        ? Colors.white
        : selected
        ? (darkSurface ? Colors.black : scheme.onPrimary)
        : (darkSurface ? Colors.white : scheme.onSurfaceVariant);
    final labelColor = darkSurface
        ? Colors.white
        : destructive
        ? scheme.error
        : scheme.onSurfaceVariant;
    return Semantics(
      button: true,
      enabled: onPressed != null,
      selected: selected,
      label: label,
      child: SizedBox(
        width: compact ? 82 : 96,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            IconButton.filled(
              iconSize: compact ? 25 : 28,
              padding: EdgeInsets.all(compact ? 13 : 15),
              style: IconButton.styleFrom(
                minimumSize: Size.square(compact ? 52 : 58),
                backgroundColor: backgroundColor,
                foregroundColor: foregroundColor,
                disabledBackgroundColor: backgroundColor.withValues(alpha: .5),
                disabledForegroundColor: foregroundColor.withValues(alpha: .7),
              ),
              tooltip: label,
              onPressed: onPressed,
              icon: busy
                  ? SizedBox.square(
                      dimension: compact ? 20 : 22,
                      child: CircularProgressIndicator(
                        strokeWidth: 2.4,
                        color: foregroundColor,
                      ),
                    )
                  : Icon(icon),
            ),
            SizedBox(height: compact ? 5 : 7),
            AnimatedSwitcher(
              duration: const Duration(milliseconds: 160),
              child: Text(
                label,
                key: ValueKey(label),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.labelMedium?.copyWith(
                  color: labelColor,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _CallBanner extends StatelessWidget {
  const _CallBanner({
    required this.call,
    required this.displayName,
    required this.busy,
    required this.onAccept,
    required this.onDecline,
    required this.onHangUp,
  });

  final CallUpdate call;
  final String displayName;
  final bool busy;
  final VoidCallback onAccept;
  final VoidCallback onDecline;
  final VoidCallback onHangUp;

  @override
  Widget build(BuildContext context) {
    final incoming = call.kind == CallUpdateKind.incoming;
    final title = switch (call.kind) {
      CallUpdateKind.incoming => 'Incoming call from $displayName',
      CallUpdateKind.dialing => 'Calling $displayName…',
      CallUpdateKind.ringing => 'Ringing $displayName…',
      CallUpdateKind.connected => 'In call with $displayName',
      CallUpdateKind.ended => 'Call ended',
      CallUpdateKind.error => 'Call failed',
    };
    return Material(
      color: Theme.of(context).colorScheme.surfaceContainerHigh,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 8),
        child: Row(
          children: [
            Icon(incoming ? Icons.call_received : Icons.call),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.titleSmall,
              ),
            ),
            if (incoming) ...[
              TextButton(
                onPressed: busy ? null : onDecline,
                child: const Text('Decline'),
              ),
              const SizedBox(width: 6),
              FilledButton.icon(
                onPressed: busy ? null : onAccept,
                icon: const Icon(Icons.call),
                label: const Text('Accept'),
              ),
            ] else
              FilledButton.tonalIcon(
                onPressed: busy ? null : onHangUp,
                icon: const Icon(Icons.call_end),
                label: const Text('Hang up'),
              ),
          ],
        ),
      ),
    );
  }
}
