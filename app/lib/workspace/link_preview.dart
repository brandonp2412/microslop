part of '../workspace.dart';

final _linkPattern = RegExp(r'https?://[^\s<>()]+');

class _LinkPreviewCard extends StatelessWidget {
  const _LinkPreviewCard({required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final uri = Uri.tryParse(url);
    final host = uri?.host.isNotEmpty == true ? uri!.host : url;
    final scheme = Theme.of(context).colorScheme;
    return Material(
      color: scheme.surfaceContainerHighest,
      borderRadius: BorderRadius.circular(10),
      clipBehavior: Clip.antiAlias,
      child: _ContextMenu(
        actions: {
          if (uri != null)
            'Open link': () =>
                unawaited(launchUrl(uri, mode: LaunchMode.externalApplication)),
          'Copy link': () => Clipboard.setData(ClipboardData(text: url)),
        },
        child: InkWell(
          onTap: uri == null
              ? null
              : () => unawaited(
                  launchUrl(uri, mode: LaunchMode.externalApplication),
                ),
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Row(
              children: [
                const Icon(Icons.link_rounded, size: 20),
                const SizedBox(width: 8),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        host,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.labelLarge,
                      ),
                      Text(
                        url,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                const Icon(Icons.open_in_new_rounded, size: 16),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

String? _firstLink(String text) {
  for (final block in _messageContentBlocks(text)) {
    if (block.$2 != null) continue;
    final link = _linkPattern.firstMatch(block.$1)?.group(0);
    if (link != null) return link;
  }
  return null;
}

TextSpan _messageTextSpan(BuildContext context, String text) {
  final style = DefaultTextStyle.of(context).style;
  final linkStyle = style.copyWith(
    color: Theme.of(context).colorScheme.primary,
    decoration: TextDecoration.underline,
    decorationColor: Theme.of(context).colorScheme.primary,
  );
  final children = <InlineSpan>[];
  var offset = 0;
  for (final match in _linkPattern.allMatches(text)) {
    if (match.start > offset) {
      children.add(TextSpan(text: text.substring(offset, match.start)));
    }
    final value = match.group(0)!;
    children.add(
      TextSpan(
        text: value,
        style: linkStyle,
        recognizer: TapGestureRecognizer()
          ..onTap = () => unawaited(
            launchUrl(Uri.parse(value), mode: LaunchMode.externalApplication),
          ),
      ),
    );
    offset = match.end;
  }
  if (offset < text.length) {
    children.add(TextSpan(text: text.substring(offset)));
  }
  return TextSpan(style: style, children: children);
}

bool _containsLink(String text) => _firstLink(text) != null;

bool _sameMessageGroup(MessageSummary previous, MessageSummary current) {
  if (previous.sender != current.sender) return false;
  final previousTime = DateTime.tryParse(previous.timestamp);
  final currentTime = DateTime.tryParse(current.timestamp);
  if (previousTime == null || currentTime == null) return false;
  final gap = currentTime.difference(previousTime).abs();
  return gap <= const Duration(minutes: 5);
}

String _imageContentType(String? extension) =>
    switch (extension?.toLowerCase()) {
      'jpg' || 'jpeg' => 'image/jpeg',
      'gif' => 'image/gif',
      'webp' => 'image/webp',
      'bmp' => 'image/bmp',
      _ => 'image/png',
    };

String _fileContentType(String? extension) => switch (extension
    ?.toLowerCase()) {
  'pdf' => 'application/pdf',
  'txt' => 'text/plain',
  'csv' => 'text/csv',
  'json' => 'application/json',
  'zip' => 'application/zip',
  'doc' => 'application/msword',
  'docx' =>
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'xls' => 'application/vnd.ms-excel',
  'xlsx' => 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'ppt' => 'application/vnd.ms-powerpoint',
  'pptx' =>
    'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  'mp3' => 'audio/mpeg',
  'm4a' || 'aac' => 'audio/mp4',
  'wav' => 'audio/wav',
  'mp4' => 'video/mp4',
  'mov' => 'video/quicktime',
  'jpg' || 'jpeg' => 'image/jpeg',
  'png' => 'image/png',
  'gif' => 'image/gif',
  'webp' => 'image/webp',
  _ => 'application/octet-stream',
};

String _messageTimestamp(BuildContext context, String value) {
  final parsed = DateTime.tryParse(value)?.toLocal();
  if (parsed == null) return value;

  final localizations = MaterialLocalizations.of(context);
  final time = localizations.formatTimeOfDay(
    TimeOfDay.fromDateTime(parsed),
    alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
  );
  final now = DateTime.now();
  if (now.year == parsed.year &&
      now.month == parsed.month &&
      now.day == parsed.day) {
    return time;
  }

  final month = parsed.month.toString().padLeft(2, '0');
  final day = parsed.day.toString().padLeft(2, '0');
  return '${parsed.year}-$month-$day $time';
}
