part of '../workspace.dart';

class _HiddenItemsScreen extends StatefulWidget {
  const _HiddenItemsScreen({
    required this.hiddenConversations,
    required this.hiddenSectionIds,
    required this.onRestoreConversation,
    required this.onRestoreSection,
    required this.sectionTitle,
  });

  final List<Conversation> hiddenConversations;
  final Set<String> hiddenSectionIds;
  final ValueChanged<Conversation> onRestoreConversation;
  final ValueChanged<String> onRestoreSection;
  final String Function(String) sectionTitle;

  @override
  State<_HiddenItemsScreen> createState() => _HiddenItemsScreenState();
}

class _HiddenItemsScreenState extends State<_HiddenItemsScreen> {
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Hidden items')),
    body: SafeArea(
      top: false,
      child:
          widget.hiddenConversations.isEmpty && widget.hiddenSectionIds.isEmpty
          ? const Center(child: Text('Nothing is hidden.'))
          : ListView(
              padding: const EdgeInsets.all(16),
              children: [
                if (widget.hiddenConversations.isNotEmpty) ...[
                  Text(
                    'Conversations',
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                  for (final conversation in List.of(
                    widget.hiddenConversations,
                  ))
                    ListTile(
                      title: Text(conversation.name),
                      subtitle: Text(
                        conversation.kind == ConversationKind.channel
                            ? 'Channel'
                            : conversation.isGroup
                            ? 'Group chat'
                            : 'Direct message',
                      ),
                      trailing: TextButton(
                        onPressed: () {
                          widget.onRestoreConversation(conversation);
                          setState(() {});
                        },
                        child: const Text('Restore'),
                      ),
                    ),
                ],
                if (widget.hiddenSectionIds.isNotEmpty) ...[
                  if (widget.hiddenConversations.isNotEmpty)
                    const Divider(height: 32),
                  Text(
                    'Navigation sections',
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                  for (final id in List.of(widget.hiddenSectionIds))
                    ListTile(
                      title: Text(widget.sectionTitle(id)),
                      trailing: TextButton(
                        onPressed: () {
                          widget.onRestoreSection(id);
                          setState(() {});
                        },
                        child: const Text('Restore'),
                      ),
                    ),
                ],
              ],
            ),
    ),
  );
}
