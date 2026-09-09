part of '../workspace.dart';

final _mentionPattern = RegExp(r'@([\p{L}\p{N}_.-]*)$', unicode: true);

class _ReactionTray extends StatelessWidget {
  const _ReactionTray({
    super.key,
    required this.reactions,
    required this.gateway,
    required this.reactionLabels,
    required this.customReactions,
    required this.enabled,
    required this.onReact,
  });

  final List<MessageReaction> reactions;
  final TeamsGateway gateway;
  final Map<String, String> reactionLabels;
  final Map<String, TeamCustomReaction> customReactions;
  final bool enabled;
  final ValueChanged<String> onReact;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Material(
      elevation: 2,
      color: scheme.surfaceContainerHighest,
      shadowColor: scheme.shadow.withValues(alpha: 0.22),
      shape: StadiumBorder(
        side: BorderSide(
          color: scheme.outlineVariant.withValues(alpha: 0.72),
          width: 0.8,
        ),
      ),
      clipBehavior: Clip.antiAlias,
      child: SizedBox(
        height: 26,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var index = 0; index < reactions.length; index++) ...[
              if (index > 0)
                SizedBox(
                  width: 1,
                  height: 14,
                  child: ColoredBox(
                    color: scheme.outlineVariant.withValues(alpha: 0.6),
                  ),
                ),
              _ReactionTrayItem(
                reaction: reactions[index],
                gateway: gateway,
                label:
                    reactionLabels[reactions[index].type] ??
                    reactionEmoji(reactions[index].type),
                customReaction: customReactions[reactions[index].type],
                enabled: enabled,
                onPressed: () => onReact(reactions[index].type),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _ReactionTrayItem extends StatefulWidget {
  const _ReactionTrayItem({
    required this.reaction,
    required this.gateway,
    required this.label,
    required this.customReaction,
    required this.enabled,
    required this.onPressed,
  });

  final MessageReaction reaction;
  final TeamsGateway gateway;
  final String label;
  final TeamCustomReaction? customReaction;
  final bool enabled;
  final VoidCallback onPressed;

  @override
  State<_ReactionTrayItem> createState() => _ReactionTrayItemState();
}

class _ReactionTrayItemState extends State<_ReactionTrayItem> {
  Future<String>? _names;

  @override
  void didUpdateWidget(covariant _ReactionTrayItem oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.reaction != widget.reaction ||
        oldWidget.gateway != widget.gateway) {
      _names = _names == null ? null : _loadNames();
    }
  }

  Future<String> _loadNames() async {
    final gateway = widget.gateway;
    final names = await Future.wait(
      widget.reaction.users.map((user) async {
        if (user.name.isNotEmpty) return user.name;
        if (user.id.isNotEmpty && gateway is ReactionUsersTeamsGateway) {
          try {
            return await (gateway as ReactionUsersTeamsGateway)
                .reactionUserName(user.id);
          } catch (_) {}
        }
        return user.id.isEmpty ? 'Unknown person' : user.id;
      }),
    );
    return names.isEmpty ? 'Reaction details unavailable' : names.join('\n');
  }

  @override
  Widget build(BuildContext context) {
    final reaction = widget.reaction;
    final customReaction = widget.customReaction;
    final label = widget.label;
    final enabled = widget.enabled;
    final onPressed = widget.onPressed;
    final scheme = Theme.of(context).colorScheme;
    return MouseRegion(
      onEnter: _desktopPlatform
          ? (_) => setState(() {
              _names ??= _loadNames();
            })
          : null,
      child: FutureBuilder<String>(
        future: _names,
        builder: (context, snapshot) => Tooltip(
          message: _desktopPlatform
              ? snapshot.data ?? 'Loading reactions…'
              : '',
          child: Semantics(
            key: ValueKey(
              'reaction-${reaction.type}-${reaction.selected ? 'selected' : 'unselected'}',
            ),
            button: true,
            selected: reaction.selected,
            enabled: enabled,
            label:
                '${customReaction?.shortcut ?? label} ${reaction.count}${reaction.selected ? ', selected' : ''}',
            child: SizedBox(
              height: 26,
              child: Ink(
                decoration: reaction.selected
                    ? ShapeDecoration(
                        color: scheme.primaryContainer,
                        shape: StadiumBorder(
                          side: BorderSide(color: scheme.primary, width: 1.5),
                        ),
                      )
                    : const BoxDecoration(),
                child: InkWell(
                  onTap: enabled ? onPressed : null,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 7),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (customReaction case final custom?)
                          Image.memory(
                            custom.icon,
                            width: 16,
                            height: 16,
                            fit: BoxFit.contain,
                            gaplessPlayback: true,
                            errorBuilder: (_, _, _) => Text(
                              label,
                              style: const TextStyle(fontSize: 15, height: 1),
                            ),
                          )
                        else
                          Text(
                            label,
                            style: const TextStyle(fontSize: 15, height: 1),
                          ),
                        const SizedBox(width: 3),
                        Text(
                          reaction.count.toString(),
                          style: Theme.of(context).textTheme.labelSmall
                              ?.copyWith(
                                color: reaction.selected
                                    ? scheme.onPrimaryContainer
                                    : scheme.onSurfaceVariant,
                                fontWeight: FontWeight.w700,
                                height: 1,
                              ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

List<(String, String)> _mentionNames(
  Conversation? conversation,
  List<MessageSummary> messages,
) => [
  for (final name in <String>{
    for (final message in messages)
      if (message.sender.trim().isNotEmpty) message.sender.trim(),
    if (conversation != null &&
        !conversation.isGroup &&
        conversation.name.startsWith('Chat with '))
      conversation.name.substring('Chat with '.length).trim(),
  })
    (name, name.toLowerCase()),
];

List<String> _mentionSuggestions(String text, List<(String, String)> names) {
  final match = _mentionPattern.firstMatch(text);
  if (match == null) return const [];
  final query = match.group(1)!.toLowerCase();
  final suggestions = <String>[];
  for (final entry in names) {
    if (!entry.$2.startsWith(query)) continue;
    suggestions.add(entry.$1);
    if (suggestions.length == 6) break;
  }
  return suggestions;
}
