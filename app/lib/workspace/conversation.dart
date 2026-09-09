part of '../workspace.dart';

class _ChatScrollPhysics extends ScrollPhysics {
  const _ChatScrollPhysics({required this.preserveAnchor, super.parent});

  final bool Function() preserveAnchor;

  @override
  _ChatScrollPhysics applyTo(ScrollPhysics? ancestor) => _ChatScrollPhysics(
    preserveAnchor: preserveAnchor,
    parent: buildParent(ancestor),
  );

  @override
  double adjustPositionForNewDimensions({
    required ScrollMetrics oldPosition,
    required ScrollMetrics newPosition,
    required bool isScrolling,
    required double velocity,
  }) {
    if (preserveAnchor() && oldPosition.pixels > oldPosition.minScrollExtent) {
      final extentDelta =
          newPosition.maxScrollExtent - oldPosition.maxScrollExtent;
      if (extentDelta != 0) {
        return (newPosition.pixels + extentDelta)
            .clamp(newPosition.minScrollExtent, newPosition.maxScrollExtent)
            .toDouble();
      }
    }
    return super.adjustPositionForNewDimensions(
      oldPosition: oldPosition,
      newPosition: newPosition,
      isScrolling: isScrolling,
      velocity: velocity,
    );
  }
}

class _ConversationPane extends StatefulWidget {
  const _ConversationPane({
    required this.mobileLayout,
    required this.conversation,
    required this.gateway,
    required this.messages,
    required this.loading,
    required this.error,
    required this.onRetry,
    required this.sending,
    required this.composer,
    required this.onSend,
    required this.reacting,
    required this.onReact,
    required this.currentUser,
    required this.loadCachedImages,
    required this.onPickImage,
    required this.onPickFile,
    required this.onOpenCamera,
    required this.onRecordAudio,
    required this.onPaste,
    required this.onInsertContent,
    required this.linkPreviewsEnabled,
  });
  final bool mobileLayout;
  final Conversation? conversation;
  final TeamsGateway gateway;
  final List<MessageSummary> messages;
  final bool loading;
  final Object? error;
  final VoidCallback? onRetry;
  final bool sending;
  final TextEditingController composer;
  final VoidCallback onSend;
  final bool reacting;
  final void Function(MessageSummary message, String reactionType) onReact;
  final UserSummary? currentUser;
  final Future<List<MessageImage>> Function(MessageSummary message)
  loadCachedImages;
  final VoidCallback onPickImage;
  final VoidCallback onPickFile;
  final VoidCallback onOpenCamera;
  final VoidCallback onRecordAudio;
  final VoidCallback onPaste;
  final Future<void> Function(Uint8List bytes, String contentType)
  onInsertContent;
  final bool linkPreviewsEnabled;

  @override
  State<_ConversationPane> createState() => _ConversationPaneState();
}

class _ConversationPaneState extends State<_ConversationPane> {
  Key _messageRowKey(MessageSummary message, int messageIndex) =>
      ValueKey<Object>(
        message.id.isEmpty
            ? (message.timestamp, message.sender, messageIndex)
            : message.id,
      );

  late Map<Key, int> _messageChildIndices;

  Map<Key, int> _buildMessageChildIndices() => {
    for (var index = 0; index < widget.messages.length; index++)
      _messageRowKey(widget.messages[index], index):
          widget.messages.length - 1 - index,
  };

  int? _findMessageChildIndex(Key key) => _messageChildIndices[key];

  void _openImage(MessageSummary owner, MessageImage clicked, int imageIndex) {
    final messages = List<MessageSummary>.of(widget.messages);
    final conversation = widget.conversation;
    final gateway = widget.gateway;
    final clickedKey = '${owner.id}:${clicked.sourceUrl ?? imageIndex}';
    List<_GalleryEntry> entries(List<MessageSummary> messages) => [
      for (final message in messages) ..._galleryEntries(message),
    ];
    final initial = entries(messages);
    if (!initial.any((entry) => entry.id == clickedKey)) {
      initial.add(_GalleryEntry(id: clickedKey, image: clicked));
    }
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        fullscreenDialog: true,
        builder: (_) => _ImageGallery(
          clickedKey: clickedKey,
          clicked: clicked,
          initial: initial,
          loadImage: gateway is LazyImageTeamsGateway
              ? (gateway as LazyImageTeamsGateway).loadMessageImage
              : (_) async => null,
          loadImages: () async {
            final refreshed = conversation == null
                ? <MessageSummary>[]
                : gateway is ImageHistoryTeamsGateway
                ? await (gateway as ImageHistoryTeamsGateway).readImageHistory(
                    conversation,
                  )
                : await gateway.readMessages(conversation);
            final byId = {for (final message in messages) message.id: message};
            for (final message in refreshed) {
              if (message.images.isNotEmpty ||
                  message.imageUrls.isNotEmpty ||
                  !byId.containsKey(message.id)) {
                byId[message.id] = message;
              }
            }
            final ordered = byId.values.toList()
              ..sort((a, b) => a.timestamp.compareTo(b.timestamp));
            return entries(ordered);
          },
        ),
      ),
    );
  }

  List<_GalleryEntry> _galleryEntries(MessageSummary message) {
    final imageUrls = {
      for (final image in message.images)
        if (image.sourceUrl != null) image.sourceUrl,
    };
    return [
      for (var index = 0; index < message.images.length; index++)
        _GalleryEntry(
          id: '${message.id}:${message.images[index].sourceUrl ?? index}',
          image: message.images[index],
          url: message.images[index].sourceUrl,
        ),
      for (var index = 0; index < message.imageUrls.length; index++)
        if (!imageUrls.contains(message.imageUrls[index]) &&
            !(index < message.images.length &&
                message.images[index].sourceUrl == null))
          _GalleryEntry(
            id: '${message.id}:${message.imageUrls[index]}',
            url: message.imageUrls[index],
          ),
    ];
  }

  int _mentionIndex = 0;
  String? _mentionQuery;
  late List<(String, String)> _mentionNamesCache;
  Map<String, TeamCustomReaction> _customReactions = const {};
  bool _preserveHistoryAnchor = false;

  @override
  void initState() {
    super.initState();
    _messageChildIndices = _buildMessageChildIndices();
    _mentionNamesCache = _mentionNames(widget.conversation, widget.messages);
    unawaited(_loadCustomReactions(widget.messages));
  }

  @override
  void didUpdateWidget(covariant _ConversationPane oldWidget) {
    super.didUpdateWidget(oldWidget);
    final messagesChanged = !identical(oldWidget.messages, widget.messages);
    _preserveHistoryAnchor =
        oldWidget.conversation == widget.conversation && messagesChanged;
    if (messagesChanged) _messageChildIndices = _buildMessageChildIndices();
    if (_preserveHistoryAnchor) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _preserveHistoryAnchor = false;
      });
    }
    if (oldWidget.conversation != widget.conversation) {
      _mentionIndex = 0;
      _mentionQuery = null;
    }
    if (oldWidget.conversation != widget.conversation || messagesChanged) {
      _mentionNamesCache = _mentionNames(widget.conversation, widget.messages);
    }
    if (!identical(oldWidget.gateway, widget.gateway)) {
      _customReactions = const {};
    }
    if (!identical(oldWidget.gateway, widget.gateway) || messagesChanged) {
      unawaited(_loadCustomReactions(widget.messages));
    }
  }

  Future<void> _loadCustomReactions(List<MessageSummary> messages) async {
    final gateway = widget.gateway;
    if (gateway is! CustomReactionTeamsGateway) return;
    final types = messages
        .expand((message) => message.reactions)
        .map((reaction) => reaction.type.trim())
        .where(
          (type) => type.contains(';') && !_customReactions.containsKey(type),
        )
        .toSet();
    if (types.isEmpty) return;
    final customGateway = gateway as CustomReactionTeamsGateway;
    final reactions = await Future.wait(
      types.map(customGateway.getCustomReaction),
    );
    if (!mounted || !identical(gateway, widget.gateway)) return;
    final loaded = reactions.whereType<TeamCustomReaction>();
    if (loaded.isEmpty) return;
    setState(() {
      _customReactions = {
        ..._customReactions,
        for (final reaction in loaded) reaction.reactionType: reaction,
      };
    });
  }

  void _moveMention(int change, int count) {
    setState(() => _mentionIndex = (_mentionIndex + change) % count);
  }

  Future<void> _showAttachmentSheet() async {
    final action = await _showActions(context, {
      'file': 'Upload file',
      'image': 'Image',
      'camera': 'Open camera',
      'audio': 'Record audio',
    });
    switch (action) {
      case 'file':
        widget.onPickFile();
      case 'image':
        widget.onPickImage();
      case 'camera':
        widget.onOpenCamera();
      case 'audio':
        widget.onRecordAudio();
    }
  }

  void _completeMention(String name) {
    final match = _mentionPattern.firstMatch(widget.composer.text);
    if (match == null) return;
    final text = widget.composer.text.replaceRange(
      match.start,
      match.end,
      '@$name ',
    );
    widget.composer.value = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: text.length),
    );
  }

  @override
  Widget build(BuildContext context) {
    final conversation = widget.conversation;
    if (conversation == null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.forum_outlined,
              size: 56,
              color: Theme.of(context).colorScheme.primary,
            ),
            const SizedBox(height: 16),
            Text(
              'Choose a chat to start reading.',
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ],
        ),
      );
    }
    const reactionLabels = {
      'like': '👍',
      'heart': '❤️',
      'laugh': '😂',
      'surprised': '😮',
      'sad': '😢',
      'angry': '😠',
    };
    return Column(
      children: [
        Expanded(
          child: widget.loading
              ? const Center(
                  child: CircularProgressIndicator(
                    key: ValueKey('message-loading-indicator'),
                  ),
                )
              : widget.error != null && widget.messages.isEmpty
              ? Center(
                  child: Padding(
                    padding: const EdgeInsets.all(24),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Text('Unable to load messages'),
                        const SizedBox(height: 8),
                        SelectableText(widget.error.toString()),
                        const SizedBox(height: 12),
                        FilledButton(
                          onPressed: widget.onRetry,
                          child: const Text('Retry'),
                        ),
                      ],
                    ),
                  ),
                )
              : widget.messages.isEmpty
              ? const Center(child: Text('No messages yet.'))
              : ListView.builder(
                  key: ValueKey(conversation),
                  reverse: true,
                  padding: const EdgeInsets.fromLTRB(24, 18, 24, 10),
                  physics: _ChatScrollPhysics(
                    preserveAnchor: () => _preserveHistoryAnchor,
                  ),
                  itemCount: widget.messages.length,
                  findChildIndexCallback: _findMessageChildIndex,
                  itemBuilder: (context, index) {
                    final messageIndex = widget.messages.length - 1 - index;
                    final message = widget.messages[messageIndex];
                    final previous = messageIndex == 0
                        ? null
                        : widget.messages[messageIndex - 1];
                    final showHeader =
                        previous == null ||
                        !_sameMessageGroup(previous, message);
                    final messageSenderId = message.senderId?.trim();
                    final currentUserId = widget.currentUser?.id?.trim();
                    final isCurrentUser =
                        message.isFromCurrentUser ||
                        _sameUserId(messageSenderId, currentUserId) ||
                        ((messageSenderId == null ||
                                messageSenderId.isEmpty ||
                                currentUserId == null ||
                                currentUserId.isEmpty) &&
                            message.sender == widget.currentUser?.displayName);
                    return Align(
                      key: _messageRowKey(message, messageIndex),
                      alignment: Alignment.topCenter,
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 760),
                        child: Padding(
                          padding: EdgeInsets.only(
                            top: showHeader && index > 0 ? 8 : 0,
                            bottom: message.reactions.isNotEmpty
                                ? 2
                                : showHeader
                                ? 10
                                : 4,
                          ),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              if (!isCurrentUser) ...[
                                SizedBox.square(
                                  dimension: 32,
                                  child: showHeader
                                      ? _ProfileAvatar(
                                          key: ValueKey(
                                            '${message.id}:${message.senderId}',
                                          ),
                                          name: message.sender,
                                          userId: message.senderId,
                                          gateway: widget.gateway,
                                          radius: 16,
                                        )
                                      : null,
                                ),
                                const SizedBox(width: 10),
                              ],
                              Expanded(
                                child: _MessageContent(
                                  key: ValueKey<Object>((
                                    widget.conversation?.kind.name,
                                    widget.conversation?.teamId,
                                    widget.conversation?.id,
                                    message.id.isEmpty ? '@$index' : message.id,
                                  )),
                                  mobileLayout: widget.mobileLayout,
                                  message: message,
                                  onOpenImage: (image, index) =>
                                      _openImage(message, image, index),
                                  gateway: widget.gateway,
                                  onRetry: widget.onRetry,
                                  showHeader: showHeader,
                                  isCurrentUser: isCurrentUser,
                                  reacting: widget.reacting,
                                  reactionLabels: reactionLabels,
                                  customReactions: _customReactions,
                                  loadCachedImages: widget.loadCachedImages,
                                  onReact: widget.onReact,
                                  linkPreviewsEnabled:
                                      widget.linkPreviewsEnabled,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                ),
        ),
        Container(
          padding: EdgeInsets.fromLTRB(
            widget.mobileLayout ? 4 : 8,
            10,
            widget.mobileLayout ? 4 : 8,
            12,
          ),
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerLow,
            border: Border(
              top: BorderSide(
                color: Theme.of(context).colorScheme.outlineVariant,
              ),
            ),
          ),
          child: ValueListenableBuilder<TextEditingValue>(
            valueListenable: widget.composer,
            builder: (context, value, _) {
              final mentions = _mentionSuggestions(
                value.text,
                _mentionNamesCache,
              );
              final mentionQuery = _mentionPattern.stringMatch(value.text);
              if (_mentionQuery != mentionQuery) {
                _mentionQuery = mentionQuery;
                _mentionIndex = 0;
              }
              if (_mentionIndex >= mentions.length) _mentionIndex = 0;
              return Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (mentions.isNotEmpty)
                    _MentionMenu(
                      names: mentions,
                      selectedIndex: _mentionIndex,
                      onSelected: _completeMention,
                    ),
                  Row(
                    children: [
                      IconButton(
                        tooltip: widget.mobileLayout
                            ? 'Add attachment'
                            : 'Upload image',
                        visualDensity: VisualDensity.compact,
                        constraints: const BoxConstraints.tightFor(
                          width: 40,
                          height: 40,
                        ),
                        padding: EdgeInsets.zero,
                        onPressed: widget.sending
                            ? null
                            : widget.mobileLayout
                            ? () => unawaited(_showAttachmentSheet())
                            : widget.onPickImage,
                        icon: Icon(
                          widget.mobileLayout
                              ? Icons.add_rounded
                              : Icons.add_photo_alternate_outlined,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: CallbackShortcuts(
                          bindings: {
                            const SingleActivator(LogicalKeyboardKey.enter):
                                widget.onSend,
                            if (mentions.isNotEmpty) ...{
                              const SingleActivator(
                                LogicalKeyboardKey.tab,
                              ): () =>
                                  _completeMention(mentions[_mentionIndex]),
                              const SingleActivator(
                                LogicalKeyboardKey.arrowDown,
                              ): () =>
                                  _moveMention(1, mentions.length),
                              const SingleActivator(
                                LogicalKeyboardKey.arrowUp,
                              ): () =>
                                  _moveMention(-1, mentions.length),
                            },
                            const SingleActivator(
                              LogicalKeyboardKey.keyV,
                              control: true,
                            ): widget.onPaste,
                          },
                          child: TextField(
                            controller: widget.composer,
                            onTapOutside: (_) =>
                                FocusScope.of(context).unfocus(),
                            minLines: 1,
                            maxLines: 5,
                            keyboardType: TextInputType.multiline,
                            textInputAction: TextInputAction.newline,
                            contentInsertionConfiguration:
                                ContentInsertionConfiguration(
                                  allowedMimeTypes: const [
                                    'image/gif',
                                    'image/png',
                                    'image/jpeg',
                                    'image/webp',
                                  ],
                                  onContentInserted: (content) {
                                    final bytes = content.data;
                                    if (bytes == null || bytes.isEmpty) return;
                                    unawaited(
                                      widget.onInsertContent(
                                        bytes,
                                        content.mimeType,
                                      ),
                                    );
                                  },
                                ),
                            decoration: const InputDecoration(
                              filled: true,
                              hintText: 'Write a message',
                            ),
                          ),
                        ),
                      ),
                      const SizedBox(width: 6),
                      IconButton.filled(
                        tooltip: 'Send message',
                        visualDensity: VisualDensity.compact,
                        constraints: const BoxConstraints.tightFor(
                          width: 40,
                          height: 40,
                        ),
                        padding: EdgeInsets.zero,
                        onPressed: widget.sending ? null : widget.onSend,
                        icon: const Icon(Icons.send_rounded, size: 20),
                      ),
                    ],
                  ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}

class _MentionMenu extends StatelessWidget {
  const _MentionMenu({
    required this.names,
    required this.selectedIndex,
    required this.onSelected,
  });

  final List<String> names;
  final int selectedIndex;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Align(
      alignment: Alignment.centerLeft,
      child: Container(
        width: 360,
        margin: const EdgeInsets.only(bottom: 8, left: 52),
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(
          color: scheme.surfaceContainerHigh,
          border: Border.all(color: scheme.outlineVariant),
          borderRadius: BorderRadius.circular(14),
          boxShadow: const [
            BoxShadow(
              color: Color(0x26000000),
              blurRadius: 18,
              offset: Offset(0, 8),
            ),
          ],
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 10, 14, 6),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'People in this conversation',
                      style: Theme.of(context).textTheme.labelMedium?.copyWith(
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                  ),
                  Text(
                    '↑↓ to navigate',
                    style: Theme.of(context).textTheme.labelSmall?.copyWith(
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
            for (var index = 0; index < names.length; index++)
              InkWell(
                onTap: () => onSelected(names[index]),
                child: Container(
                  color: index == selectedIndex
                      ? scheme.primaryContainer
                      : null,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 9,
                  ),
                  child: Row(
                    children: [
                      CircleAvatar(
                        radius: 16,
                        backgroundColor: scheme.secondaryContainer,
                        child: Text(
                          names[index].characters.first.toUpperCase(),
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                      ),
                      const SizedBox(width: 10),
                      Expanded(child: Text(names[index])),
                      if (index == selectedIndex)
                        Text(
                          'Tab to select',
                          style: Theme.of(context).textTheme.labelSmall
                              ?.copyWith(color: scheme.onPrimaryContainer),
                        ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
