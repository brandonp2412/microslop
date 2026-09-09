part of '../workspace.dart';

class _MessageContent extends StatefulWidget {
  const _MessageContent({
    super.key,
    required this.mobileLayout,
    required this.message,
    required this.gateway,
    required this.showHeader,
    required this.isCurrentUser,
    required this.reacting,
    required this.reactionLabels,
    required this.customReactions,
    required this.loadCachedImages,
    required this.onReact,
    required this.linkPreviewsEnabled,
    required this.onOpenImage,
    required this.onRetry,
  });

  final bool mobileLayout;
  final MessageSummary message;
  final TeamsGateway gateway;
  final bool showHeader;
  final bool isCurrentUser;
  final bool reacting;
  final Map<String, String> reactionLabels;
  final Map<String, TeamCustomReaction> customReactions;
  final Future<List<MessageImage>> Function(MessageSummary message)
  loadCachedImages;
  final void Function(MessageSummary message, String reactionType) onReact;
  final bool linkPreviewsEnabled;
  final void Function(MessageImage image, int index) onOpenImage;
  final VoidCallback? onRetry;

  @override
  State<_MessageContent> createState() => _MessageContentState();
}

class _MessageContentState extends State<_MessageContent> {
  bool _hovered = false;
  final _reactionOverlay = OverlayPortalController();
  final _reactionLink = LayerLink();
  Timer? _hoverExit;

  void _setHovered(bool hovered) {
    _hoverExit?.cancel();
    if (!mounted) return;
    setState(() => _hovered = hovered);
    if (!widget.mobileLayout && hovered && _canReact) {
      _reactionOverlay.show();
    } else {
      _reactionOverlay.hide();
    }
  }

  void _leaveHover() {
    _hoverExit?.cancel();
    _hoverExit = Timer(const Duration(milliseconds: 120), () {
      if (!_reactionMenuOpen) _setHovered(false);
    });
  }

  @override
  void dispose() {
    _hoverExit?.cancel();
    super.dispose();
  }

  Future<List<MessageImage>> _loadImages() async {
    final cached = await widget.loadCachedImages(widget.message);
    if (cached.isNotEmpty) return cached;
    final gateway = widget.gateway;
    if (gateway is! LazyImageTeamsGateway) return const [];
    final images = await Future.wait(
      widget.message.imageUrls.map(
        (gateway as LazyImageTeamsGateway).loadMessageImage,
      ),
    );
    return images.whereType<MessageImage>().toList();
  }

  bool _reactionMenuOpen = false;
  final MenuController _reactionMenuController = MenuController();
  Future<List<MessageImage>>? _cachedImageLoad;

  @override
  void didUpdateWidget(covariant _MessageContent oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.message.id != widget.message.id ||
        widget.message.images.isNotEmpty ||
        !listEquals(oldWidget.message.imageUrls, widget.message.imageUrls)) {
      _cachedImageLoad = null;
    }
  }

  bool get _canReact =>
      widget.message.id.isNotEmpty &&
      !widget.message.id.startsWith('local:') &&
      !widget.reacting;

  void _openReactionMenu() {
    if (!_canReact) return;
    AppLog.info(
      'Message reactions',
      'Opening reaction bar from mobile long press',
    );
    _setHovered(true);
    if (!widget.mobileLayout) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _reactionMenuController.open();
    });
  }

  void _selectReaction(String reactionType) {
    _reactionMenuController.close();
    _setHovered(false);
    if (mounted) {
      setState(() {
        _reactionMenuOpen = false;
        _hovered = false;
      });
    }
    widget.onReact(widget.message, reactionType);
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final message = widget.message;
    final contentBlocks = _messageContentBlocks(message.content);
    final cachedImageLoad =
        message.images.isEmpty &&
            (message.cachedImageCount > 0 ||
                message.imageUrls.isNotEmpty ||
                (message.content.isEmpty && message.quotes.isEmpty))
        ? (_cachedImageLoad ??= _loadImages())
        : null;
    return MouseRegion(
      onEnter: (_) => _setHovered(true),
      onExit: (_) => _leaveHover(),
      child: Column(
        crossAxisAlignment: widget.isCurrentUser
            ? CrossAxisAlignment.end
            : CrossAxisAlignment.start,
        children: [
          if (widget.showHeader) ...[
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: _UserHoverTarget(
                    name: message.sender,
                    userId: message.senderId,
                    gateway: widget.gateway,
                    child: Text(
                      message.sender,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.titleSmall,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  _messageTimestamp(context, message.timestamp),
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: scheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
          ],
          _ContextMenu(
            actions: {
              if (message.content.isNotEmpty)
                'Copy message': () =>
                    Clipboard.setData(ClipboardData(text: message.content)),
              if (_canReact) 'React': _openReactionMenu,
              if (_firstLink(message.content) case final link?) ...{
                'Open link': () => unawaited(
                  launchUrl(
                    Uri.parse(link),
                    mode: LaunchMode.externalApplication,
                  ),
                ),
                'Copy link': () => Clipboard.setData(ClipboardData(text: link)),
              },
            },
            child: GestureDetector(
              onLongPress: widget.mobileLayout && _canReact
                  ? _openReactionMenu
                  : null,
              child: CompositedTransformTarget(
                link: _reactionLink,
                child: OverlayPortal(
                  controller: _reactionOverlay,
                  overlayChildBuilder: (context) => Positioned(
                    left: 0,
                    top: 0,
                    child: CompositedTransformFollower(
                      link: _reactionLink,
                      showWhenUnlinked: false,
                      targetAnchor: widget.isCurrentUser
                          ? Alignment.topRight
                          : Alignment.topLeft,
                      followerAnchor: widget.isCurrentUser
                          ? Alignment.bottomRight
                          : Alignment.bottomLeft,
                      offset: const Offset(0, -4),
                      child: MouseRegion(
                        onEnter: (_) => _setHovered(true),
                        onExit: (_) => _leaveHover(),
                        child: _reactionBar(context),
                      ),
                    ),
                  ),
                  child: Stack(
                    clipBehavior: Clip.none,
                    alignment: widget.isCurrentUser
                        ? Alignment.topRight
                        : Alignment.topLeft,
                    children: [
                      if (!widget.mobileLayout)
                        const SizedBox(width: double.infinity),
                      Padding(
                        padding: EdgeInsets.only(
                          bottom: message.reactions.isNotEmpty ? 17 : 0,
                        ),
                        child: Container(
                          key: ValueKey('message-bubble-${message.id}'),
                          constraints: const BoxConstraints(maxWidth: 620),
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 9,
                          ),
                          decoration: BoxDecoration(
                            color: widget.isCurrentUser
                                ? scheme.primaryContainer
                                : scheme.surfaceContainerHigh,
                            borderRadius: BorderRadius.only(
                              topLeft: const Radius.circular(16),
                              topRight: const Radius.circular(16),
                              bottomLeft: Radius.circular(
                                widget.isCurrentUser ? 16 : 4,
                              ),
                              bottomRight: Radius.circular(
                                widget.isCurrentUser ? 4 : 16,
                              ),
                            ),
                          ),
                          child: Column(
                            crossAxisAlignment: widget.isCurrentUser
                                ? CrossAxisAlignment.end
                                : CrossAxisAlignment.start,
                            children: [
                              for (
                                var index = 0;
                                index < message.quotes.length;
                                index++
                              ) ...[
                                if (index > 0) const SizedBox(height: 6),
                                Container(
                                  key: ValueKey(
                                    'message-quote-${message.id}-$index',
                                  ),
                                  constraints: const BoxConstraints(
                                    minWidth: 180,
                                  ),
                                  padding: const EdgeInsets.symmetric(
                                    horizontal: 10,
                                    vertical: 8,
                                  ),
                                  decoration: BoxDecoration(
                                    color: scheme.surface.withValues(
                                      alpha: 0.55,
                                    ),
                                    borderRadius: BorderRadius.circular(8),
                                    border: Border(
                                      left: BorderSide(
                                        color: scheme.primary,
                                        width: 3,
                                      ),
                                    ),
                                  ),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      if (message
                                          .quotes[index]
                                          .sender
                                          .isNotEmpty)
                                        Text(
                                          message.quotes[index].sender,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: Theme.of(context)
                                              .textTheme
                                              .labelMedium
                                              ?.copyWith(
                                                color: scheme.primary,
                                                fontWeight: FontWeight.w600,
                                              ),
                                        ),
                                      if (message
                                              .quotes[index]
                                              .sender
                                              .isNotEmpty &&
                                          message
                                              .quotes[index]
                                              .content
                                              .isNotEmpty)
                                        const SizedBox(height: 2),
                                      if (message
                                          .quotes[index]
                                          .content
                                          .isNotEmpty)
                                        Text(
                                          message.quotes[index].content,
                                          maxLines: 4,
                                          overflow: TextOverflow.ellipsis,
                                          style: Theme.of(context)
                                              .textTheme
                                              .bodySmall
                                              ?.copyWith(
                                                color: scheme.onSurfaceVariant,
                                              ),
                                        ),
                                    ],
                                  ),
                                ),
                              ],
                              if (message.quotes.isNotEmpty &&
                                  message.content.isNotEmpty)
                                const SizedBox(height: 8),
                              if (message.content.isNotEmpty)
                                contentBlocks.length == 1 &&
                                        contentBlocks.first.$2 == null
                                    ? _messageText(
                                        context,
                                        message.content,
                                        key: ValueKey(
                                          'message-text-${message.id}',
                                        ),
                                        textAlign: widget.isCurrentUser
                                            ? TextAlign.end
                                            : TextAlign.start,
                                      )
                                    : Column(
                                        crossAxisAlignment:
                                            CrossAxisAlignment.stretch,
                                        children: [
                                          for (
                                            var index = 0;
                                            index < contentBlocks.length;
                                            index++
                                          ) ...[
                                            if (index > 0)
                                              const SizedBox(height: 8),
                                            if (contentBlocks[index].$2 == null)
                                              _messageText(
                                                context,
                                                contentBlocks[index].$1,
                                              )
                                            else
                                              Container(
                                                key: ValueKey(
                                                  'message-code-${message.id}-$index',
                                                ),
                                                padding:
                                                    const EdgeInsets.symmetric(
                                                      horizontal: 10,
                                                      vertical: 8,
                                                    ),
                                                decoration: BoxDecoration(
                                                  color: scheme.surface
                                                      .withValues(alpha: 0.72),
                                                  borderRadius:
                                                      BorderRadius.circular(8),
                                                  border: Border.all(
                                                    color:
                                                        scheme.outlineVariant,
                                                  ),
                                                ),
                                                child: Column(
                                                  crossAxisAlignment:
                                                      CrossAxisAlignment.start,
                                                  children: [
                                                    if (contentBlocks[index]
                                                        .$2!
                                                        .isNotEmpty) ...[
                                                      Text(
                                                        contentBlocks[index]
                                                            .$2!,
                                                        style: Theme.of(context)
                                                            .textTheme
                                                            .labelSmall
                                                            ?.copyWith(
                                                              color: scheme
                                                                  .onSurfaceVariant,
                                                            ),
                                                      ),
                                                      const SizedBox(height: 4),
                                                    ],
                                                    SingleChildScrollView(
                                                      scrollDirection:
                                                          Axis.horizontal,
                                                      child: widget.mobileLayout
                                                          ? Text(
                                                              contentBlocks[index]
                                                                  .$1,
                                                              style: const TextStyle(
                                                                fontFamily:
                                                                    'monospace',
                                                              ),
                                                            )
                                                          : SelectableText(
                                                              contentBlocks[index]
                                                                  .$1,
                                                              style: const TextStyle(
                                                                fontFamily:
                                                                    'monospace',
                                                              ),
                                                            ),
                                                    ),
                                                  ],
                                                ),
                                              ),
                                          ],
                                        ],
                                      ),
                              if (_firstLink(message.content) case final link?
                                  when widget.linkPreviewsEnabled) ...[
                                const SizedBox(height: 8),
                                _LinkPreviewCard(url: link),
                              ],
                              FutureBuilder<List<MessageImage>>(
                                future: cachedImageLoad,
                                initialData: message.images,
                                builder: (context, snapshot) {
                                  final images = message.images.isNotEmpty
                                      ? message.images
                                      : snapshot.data ?? const [];
                                  final slotCount = max(
                                    images.length,
                                    max(
                                      message.imageUrls.length,
                                      message.cachedImageCount,
                                    ),
                                  );
                                  if (slotCount == 0) {
                                    if (message.content.isEmpty &&
                                        message.quotes.isEmpty) {
                                      return Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          if (snapshot.connectionState ==
                                              ConnectionState.waiting)
                                            const SizedBox.square(
                                              dimension: 20,
                                              child: CircularProgressIndicator(
                                                strokeWidth: 2,
                                              ),
                                            )
                                          else
                                            const Icon(
                                              Icons.image_outlined,
                                              size: 20,
                                            ),
                                          const SizedBox(width: 8),
                                          Flexible(
                                            child: Text(
                                              snapshot.connectionState ==
                                                      ConnectionState.waiting
                                                  ? 'Loading attachment…'
                                                  : 'Attachment unavailable',
                                            ),
                                          ),
                                          if (snapshot.connectionState !=
                                                  ConnectionState.waiting &&
                                              widget.onRetry != null)
                                            IconButton(
                                              tooltip: 'Retry attachment',
                                              icon: const Icon(Icons.refresh),
                                              onPressed: () {
                                                setState(
                                                  () => _cachedImageLoad = null,
                                                );
                                                widget.onRetry!();
                                              },
                                            ),
                                        ],
                                      );
                                    }
                                    return const SizedBox.shrink();
                                  }
                                  return Column(
                                    children: [
                                      if (message.content.isNotEmpty ||
                                          message.quotes.isNotEmpty)
                                        const SizedBox(height: 8),
                                      for (
                                        var index = 0;
                                        index < slotCount;
                                        index++
                                      ) ...[
                                        if (index > 0)
                                          const SizedBox(height: 8),
                                        _messageImageSlot(
                                          context,
                                          index < images.length
                                              ? images[index]
                                              : null,
                                          index,
                                          loading:
                                              snapshot.connectionState ==
                                              ConnectionState.waiting,
                                        ),
                                      ],
                                    ],
                                  );
                                },
                              ),
                            ],
                          ),
                        ),
                      ),
                      if (widget.mobileLayout && _hovered && _canReact)
                        Positioned(
                          top: 0,
                          right: widget.isCurrentUser ? 4 : null,
                          left: widget.isCurrentUser ? null : 4,
                          child: _reactionBar(context),
                        ),
                      if (message.reactions.isNotEmpty)
                        Positioned(
                          bottom: 0,
                          right: widget.isCurrentUser ? 8 : null,
                          left: widget.isCurrentUser ? null : 8,
                          child: _ReactionTray(
                            key: ValueKey('message-reactions-${message.id}'),
                            reactions: message.reactions,
                            gateway: widget.gateway,
                            reactionLabels: widget.reactionLabels,
                            customReactions: widget.customReactions,
                            enabled: _canReact,
                            onReact: (reactionType) =>
                                widget.onReact(message, reactionType),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _messageText(
    BuildContext context,
    String text, {
    Key? key,
    TextAlign? textAlign,
  }) {
    if (!_containsLink(text)) {
      return widget.mobileLayout
          ? Text(text, key: key, textAlign: textAlign)
          : SelectableText(text, key: key, textAlign: textAlign);
    }
    final span = _messageTextSpan(context, text);
    return widget.mobileLayout
        ? Text.rich(span, key: key, textAlign: textAlign)
        : SelectableText.rich(span, key: key, textAlign: textAlign);
  }

  Widget _messageImageSlot(
    BuildContext context,
    MessageImage? image,
    int index, {
    required bool loading,
  }) {
    final content = image == null
        ? Center(
            child: loading
                ? const Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      SizedBox.square(
                        dimension: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                      SizedBox(height: 8),
                      Text('Loading attachment…'),
                    ],
                  )
                : const Icon(Icons.broken_image_outlined),
          )
        : _ContextMenu(
            actions: {
              'Open gallery': () => widget.onOpenImage(image, index),
              'Copy image': () => _imageAction(context, image, copy: true),
              'Save image': () => _imageAction(context, image),
            },
            child: InkWell(
              onTap: () => widget.onOpenImage(image, index),
              child: Image.memory(
                image.bytes,
                width: double.infinity,
                height: double.infinity,
                fit: BoxFit.contain,
                gaplessPlayback: true,
                errorBuilder: (_, _, _) =>
                    const Center(child: Icon(Icons.broken_image_outlined)),
              ),
            ),
          );
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 360),
      child: AspectRatio(
        aspectRatio: 3 / 2,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(10),
          child: content,
        ),
      ),
    );
  }

  Widget _reactionBar(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return TooltipVisibility(
      visible: widget.mobileLayout,
      child: MenuAnchor(
        controller: _reactionMenuController,
        onOpen: () => setState(() => _reactionMenuOpen = true),
        onClose: () {
          setState(() {
            _reactionMenuOpen = false;
            _hovered = false;
          });
        },
        style: MenuStyle(
          backgroundColor: WidgetStatePropertyAll(
            scheme.surfaceContainerHighest,
          ),
          elevation: const WidgetStatePropertyAll(8),
          padding: const WidgetStatePropertyAll(
            EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          ),
          shape: WidgetStatePropertyAll(
            RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(14),
              side: BorderSide(color: scheme.outlineVariant),
            ),
          ),
        ),
        builder: (context, controller, _) => !widget.mobileLayout
            ? Material(
                elevation: 3,
                borderRadius: BorderRadius.circular(18),
                color: scheme.surfaceContainerHighest,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 2),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      for (final entry in widget.reactionLabels.entries)
                        IconButton(
                          key: ValueKey('quick-reaction-${entry.key}'),
                          tooltip: entry.value,
                          visualDensity: VisualDensity.compact,
                          padding: const EdgeInsets.all(2),
                          constraints: const BoxConstraints.tightFor(
                            width: 30,
                            height: 28,
                          ),
                          style: const ButtonStyle(
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                            minimumSize: WidgetStatePropertyAll(Size(30, 28)),
                            maximumSize: WidgetStatePropertyAll(Size(30, 28)),
                          ),
                          onPressed: () => _selectReaction(entry.key),
                          icon: Text(
                            entry.value,
                            style: const TextStyle(fontSize: 18),
                          ),
                        ),
                    ],
                  ),
                ),
              )
            : IconButton(
                tooltip: 'Add reaction',
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.add_reaction_outlined, size: 20),
                onPressed: widget.reacting
                    ? null
                    : () => controller.isOpen
                          ? controller.close()
                          : controller.open(),
              ),
        menuChildren: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: widget.reactionLabels.entries
                .map(
                  (entry) => IconButton(
                    key: ValueKey('quick-reaction-${entry.key}'),
                    tooltip: entry.value,
                    onPressed: () => _selectReaction(entry.key),
                    padding: const EdgeInsets.all(7),
                    constraints: const BoxConstraints(
                      minWidth: 40,
                      minHeight: 40,
                    ),
                    icon: Text(
                      entry.value,
                      style: const TextStyle(fontSize: 22),
                    ),
                  ),
                )
                .toList(),
          ),
        ],
      ),
    );
  }
}

List<(String, String?)> _messageContentBlocks(String text) {
  if (!text.contains('```')) return [(text, null)];
  final lines = text
      .replaceAll('\r\n', '\n')
      .replaceAll('\r', '\n')
      .split('\n');
  final blocks = <(String, String?)>[];
  final current = <String>[];
  var inCode = false;
  String? language;

  void flush() {
    if (current.isEmpty) return;
    blocks.add((current.join('\n'), inCode ? language ?? '' : null));
    current.clear();
  }

  for (final line in lines) {
    final fence = line.trimLeft();
    if (fence.startsWith('```')) {
      flush();
      if (inCode) {
        inCode = false;
        language = null;
      } else {
        inCode = true;
        language = fence.substring(3).trim();
      }
      continue;
    }
    current.add(line);
  }
  flush();
  return blocks.isEmpty ? [(text, null)] : blocks;
}
