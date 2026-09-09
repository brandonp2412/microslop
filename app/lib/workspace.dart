import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:ui' as ui;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:url_launcher/url_launcher.dart';

import 'app_log.dart';
import 'auth_gateway.dart';
import 'call_gateway.dart';
import 'desktop_notifications.dart';
import 'message_clipboard.dart';
import 'message_media.dart';
import 'share_target.dart';
import 'teams_gateway.dart';
import 'workspace_cache.dart';
import 'workspace_database.dart';

part 'workspace/calls.dart';
part 'workspace/test_call.dart';
part 'workspace/settings.dart';
part 'workspace/navigation.dart';
part 'workspace/conversation.dart';
part 'workspace/message_content.dart';
part 'workspace/image_gallery.dart';
part 'workspace/message_reactions.dart';
part 'workspace/link_preview.dart';
part 'workspace/settings_hidden.dart';
part 'workspace/message_camera.dart';
part 'workspace/audio_recording.dart';
part 'workspace/presentation.dart';
part 'workspace/state_messages.dart';
part 'workspace/state_setup.dart';
part 'workspace/state_workspace.dart';
part 'workspace/state_calls.dart';
part 'workspace/state_conversations.dart';

String _userIdKey(String id) {
  var value = id.trim();
  if (value.length > 1 && value.startsWith('{') && value.endsWith('}')) {
    value = value.substring(1, value.length - 1);
  }
  return value.toLowerCase();
}

bool _sameUserId(String? left, String? right) {
  final leftValue = left?.trim();
  final rightValue = right?.trim();
  return leftValue != null &&
      leftValue.isNotEmpty &&
      rightValue != null &&
      rightValue.isNotEmpty &&
      _userIdKey(leftValue) == _userIdKey(rightValue);
}

void _showSnackBar(BuildContext context, String message) =>
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));

class TeamsWorkspace extends StatefulWidget {
  const TeamsWorkspace({
    super.key,
    required this.gateway,
    required this.callGateway,
    required this.notifications,
    this.activeAccountId,
    this.onListAccounts,
    this.onSwitchAccount,
    this.onAddAccount,
    required this.onSignOut,
  });

  final TeamsGateway gateway;
  final CallGateway callGateway;
  final DesktopNotifications notifications;
  final String? activeAccountId;
  final Future<List<SavedAccountSummary>> Function()? onListAccounts;
  final Future<void> Function(String accountId)? onSwitchAccount;
  final Future<void> Function()? onAddAccount;
  final Future<void> Function() onSignOut;

  @override
  State<TeamsWorkspace> createState() => _TeamsWorkspaceState();
}

class _TeamsWorkspaceState extends State<TeamsWorkspace>
    with WidgetsBindingObserver {
  final _composer = TextEditingController();
  final _workspaceFocus = FocusNode(debugLabel: 'workspace');
  final _searchFocus = FocusNode(debugLabel: 'workspace search');
  final _scaffoldKey = GlobalKey<ScaffoldState>();
  StreamSubscription<MessageEvent>? _messageEvents;
  StreamSubscription<CallUpdate>? _callEvents;
  StreamSubscription<String>? _notificationSelections;
  StreamSubscription<SharedContent>? _sharedContentEvents;
  Timer? _backgroundNotificationPoll;
  Timer? _presenceRefreshTimer;
  final Map<Conversation, List<MessageSummary>> _messageCache = {};
  final Map<Conversation, String> _drafts = {};
  final Set<Conversation> _hydratingImages = {};
  final Set<Conversation> _pendingImageHydrations = {};
  final Set<String> _unreadConversationIds = {};
  final Set<String> _hiddenConversationIds = {};
  final Set<String> _hiddenSectionIds = {};
  final Set<String> _mutedConversationIds = {};
  final Set<String> _favoriteConversationIds = {};
  final Set<String> _syncedConversationIds = {};
  final Set<(Conversation, String, String)> _myReactionKeys = {};
  final Map<(Conversation, String, String), DateTime> _myRemovedReactionKeys =
      {};
  final Map<(Conversation, String), DateTime> _ownReactionSuppressions = {};
  final Map<String, String> _notifiedMessageIds = {};
  final Map<String, (String, MessageSummary)> _pendingMessageNotifications = {};
  final Map<String, (String, String)> _pendingReactionNotifications = {};
  final Map<String, (String, String)>
  _pendingSavedAccountReactionNotifications = {};
  final Map<String, List<MessageSummary>> _savedAccountNotificationMessages =
      {};
  final Map<String, int> _savedAccountActivityAppliedGenerations = {};
  final Map<String, RegExp> _mentionPatterns = {};
  int _savedAccountActivityRequestGeneration = 0;
  final Map<String, int> _messageAppliedGenerations = {};
  int _messageRequestGeneration = 0;
  final WorkspaceDatabase _database = WorkspaceDatabase();
  Future<void>? _databaseReady;
  List<Conversation> _chats = const [];
  List<TeamSummary> _teams = const [];
  List<MessageSummary> _messages = const [];
  List<SavedAccountSummary> _savedAccounts = const [];
  Map<String, PresenceSummary> _presenceByUserId = const {};
  Conversation? _selected;
  UserSummary? _user;
  Object? _error;
  Object? _messageError;
  bool _loading = true;
  bool _loadingMessages = false;
  final Set<Conversation> _sendingConversations = {};
  final Set<Conversation> _reactingConversations = {};
  bool _switchingAccount = false;
  bool _callActionBusy = false;
  bool _videoCallActive = false;
  bool _ringbackActive = false;
  CallUpdate? _callUpdate;
  final Set<Conversation> _refreshingMessageConversations = {};
  final Set<Conversation> _pendingMessageRefreshes = {};
  bool _loadingWorkspace = false;
  bool _loadWorkspaceAgain = false;
  bool _refreshingChats = false;
  bool _refreshChatsAgain = false;
  bool _refreshChatsAgainNotifyPreviewChanges = false;
  final Set<String> _pendingActivityConversationIds = {};
  bool _refreshingPresence = false;
  bool _presenceRefreshPending = false;
  bool _initialWorkspaceLoadComplete = false;
  bool _loadingMoreChats = false;
  int? _pendingChatLimit;
  bool _notificationsEnabled = true;
  bool _mentionNotificationsEnabled = true;
  bool _directMessageNotificationsEnabled = true;
  bool _groupNotificationsEnabled = false;
  bool _channelNotificationsEnabled = false;
  bool _reactionNotificationsEnabled = false;
  bool _linkPreviewsEnabled = true;
  AppLifecycleState _appLifecycleState = AppLifecycleState.resumed;
  bool _hasMoreChats = true;
  int _chatLimit = 50;
  bool _groupsExpanded = true;
  bool _mobileGroupsExpanded = false;
  bool _directMessagesExpanded = true;
  double _navigationWidth = 320;
  double _groupsHeightFraction = 0.5;
  String _searchQuery = '';
  String? _pendingSelectedChatId;
  String? _pendingNotificationChatId;
  SharedContent? _pendingSharedContent;
  bool _presentingShareTarget = false;
  final _messageSync = ValueNotifier(const _MessageSyncState());
  bool _persistingWorkspaceCache = false;
  bool _persistWorkspaceCacheAgain = false;
  int _chatRequestGeneration = 0;
  int _chatAppliedGeneration = 0;
  int _teamRequestGeneration = 0;
  int _teamAppliedGeneration = 0;
  bool _loadingTeams = false;
  bool _loadTeamsAgain = false;
  int _userRequestGeneration = 0;
  int _userAppliedGeneration = 0;

  static const _channelsSectionId = 'channels';
  static const _groupsSectionId = 'groups';
  static const _directMessagesSectionId = 'direct-messages';
  static const _accountPreferenceKeys = [
    'workspace.chats',
    'workspace.selectedChatId',
    'workspace.allChatsCached',
    'navigation.favoriteConversations',
    'notifications.mutedConversations',
    'messages.syncedConversations',
  ];

  void _mutate(VoidCallback change) => setState(change);

  void _showConversationError(Conversation conversation, Object error) {
    if (!mounted || _selected != conversation) return;
    _mutate(() => _error = error);
  }

  int _beginMessageRequest() => ++_messageRequestGeneration;

  bool _acceptMessageResponse(
    Conversation conversation,
    int requestGeneration,
  ) {
    final appliedGeneration = _messageAppliedGenerations[conversation.id] ?? 0;
    if (requestGeneration < appliedGeneration) return false;
    _messageAppliedGenerations[conversation.id] = requestGeneration;
    return true;
  }

  Future<void> _loadSavedAccounts() async {
    final loader = widget.onListAccounts;
    if (loader == null) return;
    try {
      final accounts = await loader();
      if (mounted) setState(() => _savedAccounts = accounts);
    } catch (error) {
      AppLog.record('Load saved Microsoft accounts', error);
    }
  }

  Future<void> _selectAccountAction(String value) async {
    if (_switchingAccount) return;
    setState(() => _switchingAccount = true);
    try {
      if (!await _endCallForAccountTransition()) return;
      if (value == 'add-account') {
        await widget.onAddAccount?.call();
        return;
      }
      if (value == 'sign-out') {
        await _signOut();
        return;
      }
      if (!value.startsWith('account:')) return;
      final accountId = value.substring('account:'.length);
      if (accountId == widget.activeAccountId) return;
      await widget.onSwitchAccount?.call(accountId);
    } finally {
      if (mounted) setState(() => _switchingAccount = false);
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unawaited(_loadSavedAccounts());
    AppLog.info('Teams message events', 'Listening for Teams message activity');
    _messageEvents = widget.gateway.messageEvents().listen(
      _onMessageEvent,
      onError: (Object error) {
        AppLog.record('Watch Teams messages', error);
      },
      onDone: () => AppLog.info(
        'Teams message events',
        'The Teams message activity stream closed',
      ),
    );
    _callEvents = widget.callGateway.events().listen(
      _onCallUpdate,
      onError: (Object error) => AppLog.record('Watch Teams calls', error),
      onDone: () {
        final call = _callUpdate;
        if (call != null) {
          AppLog.record(
            'Watch Teams calls',
            'Call event stream closed while ${call.kind.name} ${call.callId}',
          );
        }
      },
    );
    _notificationSelections = widget.notifications.conversationSelections
        .listen(
          _onNotificationSelection,
          onError: (Object error) => AppLog.record('Open notification', error),
        );
    _sharedContentEvents = PlatformShareTarget.events.listen(
      _receiveSharedContent,
    );
    unawaited(_takePendingSharedContent());
    if (BackgroundMessageWatcher.supported) {
      _backgroundNotificationPoll = Timer.periodic(
        const Duration(seconds: 15),
        (_) {
          if (_notificationsEnabled &&
              _initialWorkspaceLoadComplete &&
              _appLifecycleState != AppLifecycleState.resumed) {
            unawaited(_refreshChats(notifyPreviewChanges: true));
          }
        },
      );
    }
    _presenceRefreshTimer = Timer.periodic(const Duration(minutes: 1), (_) {
      if (_initialWorkspaceLoadComplete &&
          _appLifecycleState == AppLifecycleState.resumed) {
        unawaited(_refreshPresence());
      }
    });
    unawaited(_initializeWorkspace());
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _messageEvents?.cancel();
    _callEvents?.cancel();
    _notificationSelections?.cancel();
    _sharedContentEvents?.cancel();
    _backgroundNotificationPoll?.cancel();
    _presenceRefreshTimer?.cancel();
    unawaited(_disposeCallSession(widget.callGateway, _callUpdate));
    unawaited(PlatformCallAudio.reset());
    if (_videoCallActive) unawaited(PlatformCallVideo.stopCamera());
    unawaited(_database.close());
    _messageSync.dispose();
    _composer.dispose();
    _workspaceFocus.dispose();
    _searchFocus.dispose();
    super.dispose();
  }

  Future<void> _disposeCallSession(
    CallGateway callGateway,
    CallUpdate? call,
  ) async {
    try {
      if (call != null) {
        if (call.kind == CallUpdateKind.incoming) {
          await callGateway.declineCall(call.callId);
        } else {
          await callGateway.hangUp();
        }
      }
    } catch (error, stackTrace) {
      AppLog.record('End Teams call during shutdown', error, stackTrace);
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _appLifecycleState = state;
    if (state != AppLifecycleState.resumed) return;
    unawaited(_loadWorkspace());
    final selected = _selected;
    if (selected != null) unawaited(_refreshSelectedMessages(selected));
  }

  List<Conversation> _conversations({required bool hidden}) => [
    ..._chats.where(
      (chat) => _hiddenConversationIds.contains(chat.id) == hidden,
    ),
    for (final team in _teams)
      ...team.channels.where(
        (channel) => _hiddenConversationIds.contains(channel.id) == hidden,
      ),
  ];

  List<Conversation> _orderedConversations(
    Iterable<Conversation> conversations,
  ) {
    final ordered = <Conversation>[];
    final others = <Conversation>[];
    for (final conversation in conversations) {
      (_favoriteConversationIds.contains(conversation.id) ? ordered : others)
          .add(conversation);
    }
    return ordered..addAll(others);
  }

  Widget _buildNavigation({
    required bool splitGroups,
    bool closeDrawerOnSelect = false,
    bool? groupsExpanded,
    ValueChanged<bool>? onGroupsExpandedChanged,
  }) {
    final favoriteTeams = <TeamSummary>[];
    final otherTeams = <TeamSummary>[];
    for (final team in _teams) {
      final summary = TeamSummary(
        id: team.id,
        name: team.name,
        channels: _orderedConversations(
          team.channels.where(
            (channel) => !_hiddenConversationIds.contains(channel.id),
          ),
        ),
      );
      (team.channels.any(
                (channel) => _favoriteConversationIds.contains(channel.id),
              )
              ? favoriteTeams
              : otherTeams)
          .add(summary);
    }
    return _Navigation(
      chats: _orderedConversations(
        _chats
            .where((chat) => !_hiddenConversationIds.contains(chat.id))
            .map(_withCachedMetadata),
      ),
      messagePreviews: {
        for (final entry in _messageCache.entries)
          if (entry.value.isNotEmpty &&
              entry.value.last.content.trim().isNotEmpty)
            entry.key.id: entry.value.last.content.trim(),
      },
      teams: favoriteTeams..addAll(otherTeams),
      unreadConversationIds: _unreadConversationIds,
      favoriteConversationIds: _favoriteConversationIds,
      mutedConversationIds: _mutedConversationIds,
      presenceByUserId: _presenceByUserId,
      selected: _selected,
      onSelect: (conversation) {
        unawaited(_select(conversation));
        if (closeDrawerOnSelect) _scaffoldKey.currentState?.closeDrawer();
      },
      searchQuery: _searchQuery,
      searchFocus: _searchFocus,
      onSearchChanged: (query) => setState(() => _searchQuery = query),
      splitGroups: splitGroups,
      groupsExpanded: groupsExpanded ?? _groupsExpanded,
      directMessagesExpanded: _directMessagesExpanded,
      onGroupsExpandedChanged:
          onGroupsExpandedChanged ??
          (expanded) => setState(() => _groupsExpanded = expanded),
      onDirectMessagesExpandedChanged: (expanded) =>
          setState(() => _directMessagesExpanded = expanded),
      groupsHeightFraction: _groupsHeightFraction,
      onGroupsHeightChanged: (fraction) =>
          setState(() => _groupsHeightFraction = fraction),
      gateway: widget.gateway,
      loading: _loading,
      onLoadMoreChats: _loadMoreChats,
      canLoadMoreChats: _hasMoreChats,
      loadingMoreChats: _loadingMoreChats,
      onHide: _hideConversation,
      hiddenSectionIds: _hiddenSectionIds,
      onHideSection: _hideSection,
      onSettings: () {
        _unfocusTextInput();
        if (closeDrawerOnSelect) _scaffoldKey.currentState?.closeDrawer();
        unawaited(_openSettings());
      },
    );
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, rootConstraints) {
      final call = _callUpdate;
      if (call != null) {
        return _ActiveCallScreen(
          call: call,
          displayName: _callDisplayName(call),
          busy: _callActionBusy,
          video: _videoCallActive,
          callGateway: widget.callGateway,
          onAccept: _acceptIncomingCall,
          onDecline: _declineIncomingCall,
          onHangUp: _hangUp,
        );
      }
      final narrow = rootConstraints.maxWidth < 720;
      return CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.keyK, control: true): () =>
              unawaited(_openCommandPalette()),
        },
        child: Focus(
          focusNode: _workspaceFocus,
          autofocus: true,
          child: Scaffold(
            key: _scaffoldKey,
            drawerEdgeDragWidth: 120,
            onDrawerChanged: (open) {
              if (open) _unfocusTextInput();
            },
            drawer: narrow
                ? Drawer(
                    child: SafeArea(
                      child: _buildNavigation(
                        splitGroups: true,
                        closeDrawerOnSelect: true,
                        groupsExpanded: _mobileGroupsExpanded,
                        onGroupsExpandedChanged: (expanded) =>
                            setState(() => _mobileGroupsExpanded = expanded),
                      ),
                    ),
                  )
                : null,
            appBar: AppBar(
              leading: narrow
                  ? IconButton(
                      tooltip: 'Open navigation',
                      onPressed: () => _scaffoldKey.currentState?.openDrawer(),
                      icon: const Icon(Icons.menu),
                    )
                  : null,
              title: _selected == null
                  ? const Text('Chats')
                  : Row(
                      children: [
                        _ConversationAvatar(
                          conversation: _selected!,
                          gateway: widget.gateway,
                          presence: _selected!.profilePhotoUserId == null
                              ? null
                              : _presenceByUserId[_userIdKey(
                                  _selected!.profilePhotoUserId!,
                                )],
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Text(
                            _selected!.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ),
              actions: [
                if (_selected case final conversation?
                    when conversation.kind == ConversationKind.chat &&
                        !conversation.isGroup) ...[
                  if (narrow)
                    IconButton(
                      tooltip: _callUpdate != null
                          ? 'Call already active'
                          : 'Start call',
                      onPressed: _callUpdate != null || _callActionBusy
                          ? null
                          : () async {
                              final video = await _showActions<bool>(
                                context,
                                const {false: 'Audio call', true: 'Video call'},
                              );
                              if (video != null && mounted) {
                                await _startCall(conversation, video: video);
                              }
                            },
                      icon: const Icon(Icons.call_outlined),
                    )
                  else ...[
                    IconButton(
                      tooltip: _callUpdate != null
                          ? 'Call already active'
                          : 'Start audio call',
                      onPressed: _callUpdate != null || _callActionBusy
                          ? null
                          : () => _startCall(conversation),
                      icon: const Icon(Icons.call_outlined),
                    ),
                    IconButton(
                      tooltip: _callUpdate != null
                          ? 'Call already active'
                          : 'Start video call',
                      onPressed: _callUpdate != null || _callActionBusy
                          ? null
                          : () => _startCall(conversation, video: true),
                      icon: const Icon(Icons.videocam_outlined),
                    ),
                  ],
                ],
                if (!narrow || _selected == null)
                  PopupMenuButton<String>(
                    tooltip: _user == null
                        ? 'Account'
                        : 'Account: ${_user!.displayName}',
                    enabled: !_switchingAccount,
                    icon: _switchingAccount
                        ? const SizedBox.square(
                            dimension: 24,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : _user == null
                        ? const Icon(Icons.account_circle_outlined)
                        : _ProfileAvatar(
                            name: _user!.displayName,
                            userId: _user!.id,
                            gateway: widget.gateway,
                            radius: 16,
                          ),
                    onSelected: (value) =>
                        unawaited(_selectAccountAction(value)),
                    itemBuilder: (context) => [
                      for (final account in _savedAccounts)
                        PopupMenuItem(
                          value: 'account:${account.id}',
                          enabled: account.id != widget.activeAccountId,
                          child: Row(
                            children: [
                              Icon(
                                account.id == widget.activeAccountId
                                    ? Icons.check_circle
                                    : Icons.account_circle_outlined,
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(account.displayName),
                                    if (account.username.trim().isNotEmpty)
                                      Text(
                                        account.username,
                                        style: Theme.of(
                                          context,
                                        ).textTheme.bodySmall,
                                      ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                      if (widget.onAddAccount != null)
                        const PopupMenuItem(
                          value: 'add-account',
                          child: ListTile(
                            contentPadding: EdgeInsets.zero,
                            leading: Icon(Icons.person_add_alt_1_outlined),
                            title: Text('Add account'),
                          ),
                        ),
                      const PopupMenuDivider(),
                      const PopupMenuItem(
                        value: 'sign-out',
                        child: ListTile(
                          contentPadding: EdgeInsets.zero,
                          leading: Icon(Icons.logout),
                          title: Text('Sign out'),
                        ),
                      ),
                    ],
                  ),
              ],
            ),
            body: SafeArea(
              top: false,
              child: Column(
                children: [
                  if (_callUpdate != null)
                    _CallBanner(
                      call: _callUpdate!,
                      displayName: _callDisplayName(_callUpdate!),
                      busy: _callActionBusy,
                      onAccept: _acceptIncomingCall,
                      onDecline: _declineIncomingCall,
                      onHangUp: _hangUp,
                    ),
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        final isDesktop = constraints.maxWidth >= 720;
                        final navigation = _buildNavigation(
                          splitGroups:
                              isDesktop && constraints.maxHeight >= 480,
                        );
                        final content = _ConversationPane(
                          mobileLayout: !isDesktop,
                          conversation: _selected,
                          gateway: widget.gateway,
                          messages: _messages,
                          loading: _loadingMessages,
                          error: _messageError,
                          onRetry: _selected == null
                              ? null
                              : () => unawaited(
                                  _refreshSelectedMessages(_selected!),
                                ),
                          sending:
                              _selected != null &&
                              _sendingConversations.contains(_selected),
                          composer: _composer,
                          onSend: _send,
                          reacting:
                              _selected != null &&
                              _reactingConversations.contains(_selected),
                          onReact: _react,
                          currentUser: _user,
                          loadCachedImages: _loadCachedImages,
                          onPickImage: _pickImage,
                          onPickFile: _pickFile,
                          onOpenCamera: _openMessageCamera,
                          onRecordAudio: _recordAudioMessage,
                          onPaste: _pasteImageOrText,
                          onInsertContent: _sendImage,
                          linkPreviewsEnabled: _linkPreviewsEnabled,
                        );
                        if (_loading) {
                          return const Center(
                            child: CircularProgressIndicator(),
                          );
                        }
                        if (_error != null && _chats.isEmpty) {
                          return _ErrorPanel(
                            error: _error!,
                            onRetry: _loadWorkspace,
                          );
                        }
                        if (!isDesktop) {
                          return Column(
                            children: [
                              Expanded(child: content),
                              if (_error != null)
                                _ErrorBanner(
                                  error: _error!,
                                  onDismiss: () =>
                                      setState(() => _error = null),
                                ),
                            ],
                          );
                        }
                        return Padding(
                          padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
                          child: Row(
                            children: [
                              ClipRRect(
                                borderRadius: BorderRadius.circular(28),
                                child: SizedBox(
                                  width: _navigationWidth.clamp(
                                    260.0,
                                    constraints.maxWidth * .55,
                                  ),
                                  child: Material(
                                    color: Theme.of(
                                      context,
                                    ).colorScheme.surfaceContainerLow,
                                    child: navigation,
                                  ),
                                ),
                              ),
                              _ResizeHandle(
                                axis: Axis.horizontal,
                                onDrag: (delta) => setState(() {
                                  _navigationWidth = (_navigationWidth + delta)
                                      .clamp(260.0, constraints.maxWidth * .55);
                                }),
                              ),
                              Expanded(
                                child: ClipRRect(
                                  borderRadius: BorderRadius.circular(28),
                                  child: Column(
                                    children: [
                                      Expanded(child: content),
                                      if (_error != null)
                                        _ErrorBanner(
                                          error: _error!,
                                          onDismiss: () =>
                                              setState(() => _error = null),
                                        ),
                                    ],
                                  ),
                                ),
                              ),
                            ],
                          ),
                        );
                      },
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
    },
  );
}
