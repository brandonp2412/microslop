part of '../workspace.dart';

final _whitespacePattern = RegExp(r'\s+');
final _avatarInitialsCache = <String, String>{};

final _presenceWordBoundaryPattern = RegExp(
  r'(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])',
);

bool get _desktopPlatform => switch (defaultTargetPlatform) {
  TargetPlatform.windows ||
  TargetPlatform.linux ||
  TargetPlatform.macOS => true,
  _ => false,
};

IconData _actionIcon(String label) => switch (label) {
  String() when label.startsWith('Copy') => Icons.copy_outlined,
  String() when label.startsWith('Hide') => Icons.visibility_off_outlined,
  String() when label.contains('favourites') => Icons.star_outline,
  String() when label.contains('notifications') => Icons.notifications_outlined,
  'React' => Icons.add_reaction_outlined,
  'Save image' => Icons.download_outlined,
  'Open gallery' || 'Image' => Icons.photo_library_outlined,
  'Open link' => Icons.open_in_new,
  'Collapse' => Icons.expand_less,
  'Expand' => Icons.expand_more,
  'Upload file' => Icons.attach_file,
  'Open camera' => Icons.camera_alt_outlined,
  'Record audio' => Icons.mic_none,
  'Audio call' => Icons.call_rounded,
  'Video call' => Icons.videocam_rounded,
  String() when label.endsWith('options') => Icons.tune,
  _ => Icons.chat_bubble_outline,
};

Future<T?> _showActions<T>(
  BuildContext context,
  Map<T, String> actions, {
  bool contextMenu = false,
}) {
  if (_desktopPlatform || contextMenu) {
    return showMenu<T>(
      context: context,
      popUpAnimationStyle: AnimationStyle.noAnimation,
      position: RelativeRect.fromSize(
        (_ContextMenu.position ??
                MediaQuery.sizeOf(context).center(Offset.zero)) &
            Size.zero,
        MediaQuery.sizeOf(context),
      ),
      items: [
        for (final entry in actions.entries)
          PopupMenuItem(
            value: entry.key,
            child: Row(
              children: [
                Icon(_actionIcon(entry.value), size: 20),
                const SizedBox(width: 12),
                Flexible(child: Text(entry.value)),
              ],
            ),
          ),
      ],
    );
  }
  return showModalBottomSheet<T>(
    context: context,
    builder: (context) => SafeArea(
      child: Wrap(
        children: [
          for (final entry in actions.entries)
            ListTile(
              leading: Icon(_actionIcon(entry.value)),
              title: Text(entry.value),
              onTap: () => Navigator.pop(context, entry.key),
            ),
        ],
      ),
    ),
  );
}

class _ContextMenu extends StatelessWidget {
  static Offset? position;
  const _ContextMenu({required this.actions, required this.child});

  final Map<String, VoidCallback> actions;
  final Widget child;

  @override
  Widget build(BuildContext context) => GestureDetector(
    behavior: HitTestBehavior.translucent,
    onSecondaryTapUp: (details) async {
      if (actions.isEmpty) return;
      position = details.globalPosition;
      final action = await _showActions(context, {
        for (final label in actions.keys) label: label,
      }, contextMenu: true);
      if (context.mounted && action != null) actions[action]?.call();
      position = null;
    },
    child: child,
  );
}

class _ConversationAvatar extends StatelessWidget {
  const _ConversationAvatar({
    required this.conversation,
    required this.gateway,
    this.selected = false,
    this.radius = 20,
    this.presence,
  });

  final Conversation conversation;
  final TeamsGateway gateway;
  final bool selected;
  final double radius;
  final PresenceSummary? presence;

  @override
  Widget build(BuildContext context) {
    if (conversation.teamId != null) {
      return _TeamAvatar(
        name: conversation.name,
        teamId: conversation.teamId!,
        gateway: gateway,
        selected: selected,
        radius: radius,
      );
    }
    if (conversation.isGroup) {
      return _GroupAvatar(
        name: conversation.name,
        chatId: conversation.id,
        gateway: gateway,
        selected: selected,
        radius: radius,
      );
    }
    final avatar = _ProfileAvatar(
      name: conversation.name,
      userId: conversation.profilePhotoUserId,
      gateway: gateway,
      selected: selected,
      radius: radius,
    );
    final presence = this.presence;
    return presence == null
        ? avatar
        : _PresenceAvatar(avatar: avatar, presence: presence, radius: radius);
  }
}

class _PresenceAvatar extends StatelessWidget {
  const _PresenceAvatar({
    required this.avatar,
    required this.presence,
    required this.radius,
  });

  final Widget avatar;
  final PresenceSummary presence;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final label = _presenceLabel(presence);
    final size = (radius * .55).clamp(8.0, 12.0);
    return Tooltip(
      message: label,
      child: Semantics(
        label: label,
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            avatar,
            Positioned(
              right: -1,
              bottom: -1,
              child: Container(
                width: size,
                height: size,
                decoration: BoxDecoration(
                  color: _presenceColor(presence.availability),
                  shape: BoxShape.circle,
                  border: Border.all(
                    color: Theme.of(context).colorScheme.surface,
                    width: 2,
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

String _presenceLabel(PresenceSummary presence) {
  final activity = presence.activity.trim();
  if (activity.isNotEmpty && activity != presence.availability) {
    return '${_splitPresenceName(presence.availability)} · ${_splitPresenceName(activity)}';
  }
  return _splitPresenceName(presence.availability);
}

String _splitPresenceName(String value) {
  final spaced = value
      .replaceAllMapped(_presenceWordBoundaryPattern, (_) => ' ')
      .trim();
  if (spaced.isEmpty) return spaced;
  return '${spaced[0]}${spaced.substring(1).toLowerCase()}';
}

Color _presenceColor(String availability) => switch (availability) {
  'Available' || 'AvailableIdle' => Colors.green,
  'Busy' || 'BusyIdle' || 'DoNotDisturb' => Colors.red,
  'Away' || 'BeRightBack' => Colors.amber,
  _ => Colors.grey,
};

class _TeamAvatar extends StatefulWidget {
  const _TeamAvatar({
    required this.name,
    required this.teamId,
    required this.gateway,
    this.selected = false,
    this.radius = 20,
  });

  final String name;
  final String teamId;
  final TeamsGateway gateway;
  final bool selected;
  final double radius;

  @override
  State<_TeamAvatar> createState() => _TeamAvatarState();
}

class _RetryableAvatarPhoto {
  Future<Uint8List?>? future;
  bool _missing = false;
  bool _retriedMissing = false;

  void load(Future<Uint8List?> Function() loader, {bool resetRetry = false}) {
    if (resetRetry) _retriedMissing = false;
    _missing = false;
    future = loader().then((photo) {
      _missing = photo == null;
      return photo;
    });
  }

  void clear({bool resetRetry = false}) {
    if (resetRetry) _retriedMissing = false;
    _missing = false;
    future = null;
  }

  void retryIfMissing(Future<Uint8List?> Function() loader) {
    if (!_missing || _retriedMissing) return;
    _retriedMissing = true;
    load(loader);
  }
}

class _TeamAvatarState extends State<_TeamAvatar> {
  final _photo = _RetryableAvatarPhoto();

  @override
  void initState() {
    super.initState();
    _loadPhoto(resetRetry: true);
  }

  @override
  void didUpdateWidget(covariant _TeamAvatar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.teamId != widget.teamId ||
        oldWidget.gateway != widget.gateway) {
      _loadPhoto(resetRetry: true);
    } else {
      _photo.retryIfMissing(() => widget.gateway.getTeamPhoto(widget.teamId));
    }
  }

  void _loadPhoto({bool resetRetry = false}) => _photo.load(
    () => widget.gateway.getTeamPhoto(widget.teamId),
    resetRetry: resetRetry,
  );

  @override
  Widget build(BuildContext context) => FutureBuilder<Uint8List?>(
    future: _photo.future,
    builder: (context, snapshot) => _avatarCircle(
      context,
      name: widget.name,
      bytes: snapshot.data,
      selected: widget.selected,
      radius: widget.radius,
    ),
  );
}

class _GroupAvatar extends StatefulWidget {
  const _GroupAvatar({
    required this.name,
    required this.chatId,
    required this.gateway,
    required this.selected,
    required this.radius,
  });

  final String name;
  final String chatId;
  final TeamsGateway gateway;
  final bool selected;
  final double radius;

  @override
  State<_GroupAvatar> createState() => _GroupAvatarState();
}

class _GroupAvatarState extends State<_GroupAvatar> {
  final _photo = _RetryableAvatarPhoto();

  bool get _isMeeting => widget.chatId.startsWith('19:meeting_');

  @override
  void initState() {
    super.initState();
    _loadPhotos(resetRetry: true);
  }

  @override
  void didUpdateWidget(covariant _GroupAvatar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.chatId != widget.chatId ||
        oldWidget.gateway != widget.gateway) {
      _loadPhotos(resetRetry: true);
    } else if (!_isMeeting) {
      _photo.retryIfMissing(() => widget.gateway.getChatPhoto(widget.chatId));
    }
  }

  void _loadPhotos({bool resetRetry = false}) {
    if (_isMeeting) {
      _photo.clear(resetRetry: resetRetry);
      AppLog.debug(
        'Group avatar',
        'name=${widget.name}, chatId=${widget.chatId}, source=meeting',
      );
      return;
    }
    _photo.load(
      () => widget.gateway.getChatPhoto(widget.chatId).then((photo) {
        AppLog.debug(
          'Group avatar',
          'name=${widget.name}, chatId=${widget.chatId}, source=${photo == null ? 'initials' : 'custom'}',
        );
        return photo;
      }),
      resetRetry: resetRetry,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_isMeeting) {
      final colours = Theme.of(context).colorScheme;
      return Tooltip(
        message: 'Meeting',
        child: CircleAvatar(
          radius: widget.radius,
          backgroundColor: widget.selected
              ? colours.primary
              : colours.primaryContainer,
          child: Icon(
            Icons.calendar_month_outlined,
            size: widget.radius,
            color: widget.selected
                ? colours.onPrimary
                : colours.onPrimaryContainer,
          ),
        ),
      );
    }
    return FutureBuilder<Uint8List?>(
      future: _photo.future,
      builder: (context, chatSnapshot) {
        if (chatSnapshot.data != null) {
          return _avatarCircle(
            context,
            name: widget.name,
            bytes: chatSnapshot.data,
            selected: widget.selected,
            radius: widget.radius,
          );
        }
        return _avatarCircle(
          context,
          name: widget.name,
          bytes: null,
          selected: widget.selected,
          radius: widget.radius,
        );
      },
    );
  }
}

String _avatarInitials(String name) => _avatarInitialsCache.putIfAbsent(
  name,
  () => name
      .trim()
      .split(_whitespacePattern)
      .where((part) => part.isNotEmpty)
      .take(2)
      .map((part) => String.fromCharCode(part.runes.first).toUpperCase())
      .join(),
);

Widget _avatarCircle(
  BuildContext context, {
  required String name,
  required Uint8List? bytes,
  required bool selected,
  required double radius,
}) {
  final colours = Theme.of(context).colorScheme;
  final initials = _avatarInitials(name);
  return CircleAvatar(
    radius: radius,
    backgroundColor: selected
        ? colours.secondary
        : colours.surfaceContainerHighest,
    foregroundColor: selected ? colours.onSecondary : colours.onSurfaceVariant,
    backgroundImage: bytes == null ? null : MemoryImage(bytes),
    child: bytes == null ? Text(initials.isEmpty ? '?' : initials) : null,
  );
}

class _UserHoverTarget extends StatefulWidget {
  const _UserHoverTarget({
    required this.name,
    required this.userId,
    required this.gateway,
    required this.child,
  });

  final String name;
  final String? userId;
  final TeamsGateway gateway;
  final Widget child;

  @override
  State<_UserHoverTarget> createState() => _UserHoverTargetState();
}

class _UserHoverTargetState extends State<_UserHoverTarget> {
  Future<UserDetailsSummary?>? _details;

  void _loadDetails() {
    if (!_desktopPlatform || _details != null) return;
    final userId = widget.userId?.trim();
    final gateway = widget.gateway;
    if (userId == null ||
        userId.isEmpty ||
        gateway is! UserDetailsTeamsGateway) {
      return;
    }
    setState(() {
      _details = (gateway as UserDetailsTeamsGateway).getUserDetails(userId);
    });
  }

  @override
  void didUpdateWidget(covariant _UserHoverTarget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.userId != widget.userId ||
        oldWidget.gateway != widget.gateway) {
      _details = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!_desktopPlatform) return widget.child;
    return MouseRegion(
      onEnter: (_) => _loadDetails(),
      child: FutureBuilder<UserDetailsSummary?>(
        future: _details,
        builder: (context, snapshot) => Tooltip(
          waitDuration: const Duration(milliseconds: 350),
          preferBelow: false,
          message: _userDetailsLabel(widget.name, snapshot.data),
          child: widget.child,
        ),
      ),
    );
  }
}

String _userDetailsLabel(String fallbackName, UserDetailsSummary? details) {
  final lines = <String>[
    details?.displayName.trim().isNotEmpty == true
        ? details!.displayName.trim()
        : fallbackName,
  ];
  final title = details?.jobTitle?.trim();
  if (title != null && title.isNotEmpty) lines.add(title);
  final status = details?.statusMessage?.trim();
  if (status != null && status.isNotEmpty) lines.add(status);
  final availability = details?.availability?.trim();
  final activity = details?.activity?.trim();
  if (availability != null && availability.isNotEmpty) {
    final presence =
        activity != null &&
            activity.isNotEmpty &&
            activity.toLowerCase() != availability.toLowerCase()
        ? '${_splitPresenceName(availability)} · ${_splitPresenceName(activity)}'
        : _splitPresenceName(availability);
    lines.add(presence);
  }
  final email = details?.email?.trim();
  if (email != null && email.isNotEmpty) lines.add(email);
  return lines.join('\n');
}

class _ProfileAvatar extends StatefulWidget {
  const _ProfileAvatar({
    super.key,
    required this.name,
    required this.userId,
    required this.gateway,
    this.selected = false,
    this.radius,
  });

  final String name;
  final String? userId;
  final TeamsGateway gateway;
  final bool selected;
  final double? radius;

  @override
  State<_ProfileAvatar> createState() => _ProfileAvatarState();
}

class _ProfileAvatarState extends State<_ProfileAvatar> {
  final _photo = _RetryableAvatarPhoto();

  @override
  void initState() {
    super.initState();
    _loadPhoto(resetRetry: true);
  }

  @override
  void didUpdateWidget(covariant _ProfileAvatar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.userId != oldWidget.userId ||
        widget.gateway != oldWidget.gateway) {
      _loadPhoto(resetRetry: true);
    } else if (widget.userId case final userId?) {
      _photo.retryIfMissing(() => widget.gateway.getProfilePhoto(userId));
    }
  }

  void _loadPhoto({bool resetRetry = false}) {
    final userId = widget.userId;
    if (userId == null) {
      _photo.clear(resetRetry: resetRetry);
      return;
    }
    _photo.load(
      () => widget.gateway.getProfilePhoto(userId),
      resetRetry: resetRetry,
    );
  }

  @override
  Widget build(BuildContext context) {
    final colours = Theme.of(context).colorScheme;
    final initials = _avatarInitials(widget.name);
    return FutureBuilder<Uint8List?>(
      future: _photo.future,
      builder: (context, snapshot) {
        return _UserHoverTarget(
          name: widget.name,
          userId: widget.userId,
          gateway: widget.gateway,
          child: CircleAvatar(
            radius: widget.radius,
            backgroundColor: widget.selected
                ? colours.secondary
                : colours.surfaceContainerHighest,
            foregroundColor: widget.selected
                ? colours.onSecondary
                : colours.onSurfaceVariant,
            backgroundImage: snapshot.data == null
                ? null
                : MemoryImage(snapshot.data!),
            child: snapshot.data == null
                ? Text(initials.isEmpty ? '?' : initials)
                : null,
          ),
        );
      },
    );
  }
}

class _ResizeHandle extends StatelessWidget {
  const _ResizeHandle({required this.axis, required this.onDrag});
  final Axis axis;
  final ValueChanged<double> onDrag;

  @override
  Widget build(BuildContext context) {
    final horizontal = axis == Axis.horizontal;
    return MouseRegion(
      cursor: horizontal
          ? SystemMouseCursors.resizeLeftRight
          : SystemMouseCursors.resizeUpDown,
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onHorizontalDragUpdate: horizontal
            ? (details) => onDrag(details.delta.dx)
            : null,
        onVerticalDragUpdate: horizontal
            ? null
            : (details) => onDrag(details.delta.dy),
        child: SizedBox(
          width: horizontal ? 8 : double.infinity,
          height: horizontal ? double.infinity : 8,
          child: Center(
            child: Container(
              width: horizontal ? 1 : 36,
              height: horizontal ? 36 : 1,
              color: Theme.of(context).colorScheme.outlineVariant,
            ),
          ),
        ),
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.error, required this.onDismiss});
  final Object error;
  final VoidCallback onDismiss;
  @override
  Widget build(BuildContext context) => MaterialBanner(
    content: Text(error.toString()),
    actions: [TextButton(onPressed: onDismiss, child: const Text('Dismiss'))],
  );
}

class _ErrorPanel extends StatelessWidget {
  const _ErrorPanel({required this.error, required this.onRetry});
  final Object error;
  final VoidCallback onRetry;
  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'Unable to load Teams',
            style: Theme.of(context).textTheme.headlineSmall,
          ),
          const SizedBox(height: 12),
          SelectableText(error.toString()),
          const SizedBox(height: 16),
          FilledButton(onPressed: onRetry, child: const Text('Retry')),
        ],
      ),
    ),
  );
}

class _ErrorLogScreen extends StatelessWidget {
  const _ErrorLogScreen({required this.entries});
  final List<String> entries;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('Debug log'),
      actions: [
        IconButton(
          tooltip: 'Copy debug log',
          onPressed: entries.isEmpty
              ? null
              : () => Clipboard.setData(
                  ClipboardData(text: entries.join('\n\n')),
                ),
          icon: const Icon(Icons.copy_all_rounded),
        ),
      ],
    ),
    body: SafeArea(
      top: false,
      child: entries.isEmpty
          ? const Center(child: Text('No recorded errors.'))
          : ListView.builder(
              itemCount: entries.length,
              padding: const EdgeInsets.all(16),
              itemBuilder: (_, index) => SelectableText(
                entries[index],
                style: const TextStyle(fontFamily: 'monospace'),
              ),
            ),
    ),
  );
}
