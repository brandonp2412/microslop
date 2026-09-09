part of '../widget_test.dart';

class OwnActivityWatchingGateway extends WatchingMessagesGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'own-message',
          sender: 'Test User',
          senderId: 'test-user',
          isFromCurrentUser: true,
          timestamp: '2026-08-31T01:00:00Z',
          content: 'My own activity',
        ),
      ];
}

class DeletedMessageActivityGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final reopenRefreshStarted = Completer<void>();
  final _reopenRefresh = Completer<List<MessageSummary>>();
  int graceRefreshes = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada') {
      return Future.value(const [
        MessageSummary(
          id: 'ada-message',
          sender: 'Ada Lovelace',
          timestamp: '2026-09-04T11:00:00Z',
          content: 'Ada message',
        ),
      ]);
    }
    graceRefreshes++;
    if (graceRefreshes == 1) {
      return Future.value(const [
        MessageSummary(
          id: 'deleted-message',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T11:01:00Z',
          content: 'Deleted remotely',
        ),
      ]);
    }
    if (graceRefreshes == 2) return Future.value(const []);
    if (!reopenRefreshStarted.isCompleted) reopenRefreshStarted.complete();
    return _reopenRefresh.future;
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class ActivitySelectionRaceGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final activityStarted = Completer<void>();
  final releaseActivity = Completer<List<MessageSummary>>();
  final reopenRefreshStarted = Completer<void>();
  final reopenRefresh = Completer<List<MessageSummary>>();
  int graceRefreshes = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada') {
      return Future.value(const [
        MessageSummary(
          id: 'ada-message',
          sender: 'Ada Lovelace',
          timestamp: '2026-09-04T12:00:00Z',
          content: 'Ada current',
        ),
      ]);
    }
    graceRefreshes++;
    return switch (graceRefreshes) {
      1 => Future.value(const [
        MessageSummary(
          id: 'grace-baseline',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T12:00:00Z',
          content: 'Grace baseline',
        ),
      ]),
      2 => () {
        if (!activityStarted.isCompleted) activityStarted.complete();
        return releaseActivity.future;
      }(),
      3 => Future.value(const [
        MessageSummary(
          id: 'grace-fresh',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T12:02:00Z',
          content: 'Grace fresh selected',
        ),
      ]),
      _ => () {
        if (!reopenRefreshStarted.isCompleted) reopenRefreshStarted.complete();
        return reopenRefresh.future;
      }(),
    };
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class ActivityOpenRaceGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final activityStarted = Completer<void>();
  final releaseActivity = Completer<List<MessageSummary>>();
  final selectedRefreshStarted = Completer<void>();
  final selectedRefresh = Completer<List<MessageSummary>>();
  int graceRefreshes = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada') {
      return Future.value(const [
        MessageSummary(
          id: 'ada-message',
          sender: 'Ada Lovelace',
          timestamp: '2026-09-04T13:00:00Z',
          content: 'Ada current',
        ),
      ]);
    }
    graceRefreshes++;
    return switch (graceRefreshes) {
      1 => Future.value(const [
        MessageSummary(
          id: 'grace-baseline',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T13:00:00Z',
          content: 'Grace baseline',
        ),
      ]),
      2 => () {
        if (!activityStarted.isCompleted) activityStarted.complete();
        return releaseActivity.future;
      }(),
      _ => () {
        if (!selectedRefreshStarted.isCompleted) {
          selectedRefreshStarted.complete();
        }
        return selectedRefresh.future;
      }(),
    };
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class DelayedResponsiveHistoryGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  Completer<List<MessageSummary>>? _pendingAda;
  bool _delayAda = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  void delayNextAdaRefresh() => _delayAda = true;

  void completeAdaRefresh() {
    _pendingAda?.complete(const [
      MessageSummary(
        id: 'ada-new',
        sender: 'Ada Lovelace',
        timestamp: '2026-08-31T01:01:00Z',
        content: 'Current Ada history',
      ),
    ]);
    _pendingAda = null;
  }

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada' && _delayAda) {
      _delayAda = false;
      return (_pendingAda = Completer<List<MessageSummary>>()).future;
    }
    return Future.value([
      MessageSummary(
        id: '${conversation.id}-old',
        sender: 'Ada Lovelace',
        timestamp: '2026-08-31T01:00:00Z',
        content: conversation.id == 'chat-ada'
            ? 'Old Ada history'
            : 'Grace history',
      ),
    ]);
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];
}

class EquivalentDelayedResponsiveHistoryGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  Completer<List<MessageSummary>>? _pendingAda;
  bool _delayAda = false;
  int hydrationCalls = 0;
  int profilePhotoRequests = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  List<MessageSummary> _messages(Conversation conversation) => [
    for (var index = 0; index < 80; index++)
      MessageSummary(
        id: '${conversation.id}-$index',
        sender: conversation.id == 'chat-ada' ? 'Ada Lovelace' : 'Grace Hopper',
        senderId: conversation.id == 'chat-ada' ? 'ada-user' : 'grace-user',
        timestamp: DateTime.utc(
          2026,
          9,
          5,
        ).add(Duration(minutes: index * 10)).toIso8601String(),
        content: '${conversation.id} message $index',
      ),
  ];

  void delayNextAdaRefresh() => _delayAda = true;

  void completeAdaRefresh() {
    _pendingAda?.complete(
      _messages(
        const Conversation.chat(
          id: 'chat-ada',
          name: 'Chat with Ada',
          isGroup: false,
        ),
      ),
    );
    _pendingAda = null;
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      _messages(conversation);

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada' && _delayAda) {
      _delayAda = false;
      return (_pendingAda = Completer<List<MessageSummary>>()).future;
    }
    return Future.value(_messages(conversation));
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async {
    hydrationCalls++;
    return const [];
  }

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async {
    profilePhotoRequests++;
    return null;
  }
}

class NotificationAvatarGateway extends ReconcileNotificationGateway {
  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => testPng;
}

class DelayedNotificationAvatarGateway extends ReconcileNotificationGateway {
  final avatarStarted = Completer<void>();
  final releaseAvatar = Completer<Uint8List?>();

  @override
  Future<Uint8List?> getProfilePhoto(String userId) {
    if (!avatarStarted.isCompleted) avatarStarted.complete();
    return releaseAvatar.future;
  }
}

class NewChatReconcileGateway extends WatchingMessagesGateway {
  bool revealGrace = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => [
    const Conversation.chat(
      id: 'chat-ada',
      name: 'Chat with Ada',
      isGroup: false,
    ),
    if (revealGrace)
      const Conversation.chat(
        id: 'chat-grace-new',
        name: 'Chat with New Grace',
        isGroup: false,
        lastMessageId: 'grace-new-first',
        preview: 'Hello from the new chat',
      ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.id == 'chat-grace-new'
      ? const [
          MessageSummary(
            id: 'grace-new-first',
            sender: 'Grace Hopper',
            senderId: 'grace-user',
            timestamp: '2026-09-04T14:22:00Z',
            content: 'Hello from the new chat',
          ),
        ]
      : super.readMessages(conversation);
}

class FirstMessageReconcileGateway extends WatchingMessagesGateway {
  bool hasFirstMessage = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => [
    const Conversation.chat(
      id: 'chat-ada',
      name: 'Chat with Ada',
      isGroup: false,
    ),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
      lastMessageId: hasFirstMessage ? 'grace-first' : null,
      preview: '',
    ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.id == 'chat-grace' && hasFirstMessage
      ? const [
          MessageSummary(
            id: 'grace-first',
            sender: 'Grace Hopper',
            senderId: 'grace-user',
            timestamp: '2026-09-04T14:20:00Z',
            content: 'First message in this chat',
          ),
        ]
      : super.readMessages(conversation);
}

class ReconcileNotificationGateway extends WatchingMessagesGateway {
  bool changedPreview = false;
  bool changedSelectedPreview = false;
  bool samePreviewNewMessage = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => [
    Conversation.chat(
      id: 'chat-ada',
      name: 'Chat with Ada',
      isGroup: false,
      preview: changedSelectedPreview ? 'New Ada preview' : 'Unchanged preview',
    ),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
      lastMessageId: samePreviewNewMessage ? 'grace-new' : 'grace-old',
      preview: changedPreview ? 'New preview' : 'Old preview',
    ),
  ];
}

class GrowingHistoryGateway extends WatchingMessagesGateway {
  bool newerMessage = false;
  bool newestReaction = false;
  bool cachedImageSlot = false;
  bool cachedImageLoaded = false;

  @override
  Future<List<MessageSummary>> readMessages(
    Conversation conversation,
  ) async => [
    for (var index = 0; index < 60; index++)
      MessageSummary(
        id: 'history-$index',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp:
            '2026-09-05T${(index ~/ 60).toString().padLeft(2, '0')}:${(index % 60).toString().padLeft(2, '0')}:00Z',
        content: 'History $index',
        images: cachedImageLoaded && index == 59
            ? [MessageImage(contentType: 'image/png', bytes: testPng)]
            : const [],
        cachedImageCount: cachedImageSlot && index == 59 ? 1 : 0,
        reactions: newestReaction && index == 59
            ? const [MessageReaction(type: 'like', count: 1)]
            : const [],
      ),
    if (newerMessage)
      const MessageSummary(
        id: 'history-new',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: '2026-09-05T01:00:00Z',
        content: 'Newest history',
      ),
  ];
}

class PendingImageHydrationGateway extends WatchingMessagesGateway
    implements ResponsiveTeamsGateway {
  final hydrationStarted = Completer<void>();
  final releaseHydration = Completer<void>();
  int hydrationCalls = 0;
  bool newerMessage = false;

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async => [
    MessageSummary(
      id: newerMessage ? 'new-image-message' : 'baseline-message',
      sender: 'Ada Lovelace',
      timestamp: '2026-09-04T17:00:00Z',
      content: newerMessage ? 'New image message' : 'Baseline message',
    ),
  ];

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async {
    hydrationCalls++;
    if (hydrationCalls == 1) {
      if (!hydrationStarted.isCompleted) hydrationStarted.complete();
      await releaseHydration.future;
    }
    return const [];
  }
}

class DelayedRefreshSwitchGateway extends WatchingMessagesGateway {
  Completer<List<MessageSummary>>? _pendingAdaRefresh;
  bool _delayNextAdaRefresh = false;

  bool get hasPendingAdaRefresh => _pendingAdaRefresh != null;

  void delayNextAdaRefresh() => _delayNextAdaRefresh = true;

  void completeAdaRefresh() {
    _pendingAdaRefresh?.complete(const [
      MessageSummary(
        sender: 'Ada Lovelace',
        timestamp: 'Now',
        content: 'First message',
      ),
    ]);
    _pendingAdaRefresh = null;
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) {
    if (_delayNextAdaRefresh && conversation.id == 'chat-ada') {
      _delayNextAdaRefresh = false;
      readCount++;
      readCounts.update(
        conversation.id,
        (count) => count + 1,
        ifAbsent: () => 1,
      );
      return (_pendingAdaRefresh = Completer<List<MessageSummary>>()).future;
    }
    return super.readMessages(conversation);
  }
}

class DelayedInitialChatsGateway extends WatchingMessagesGateway {
  final _initialChats = Completer<List<Conversation>>();
  int listCount = 0;

  void completeInitialLoad() => _initialChats.complete(const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
  ]);

  @override
  Future<List<Conversation>> listChats({int limit = 50}) {
    listCount++;
    if (listCount == 1) return _initialChats.future;
    return super.listChats(limit: limit);
  }
}

class SplitChatsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'group-1', name: 'Project group', isGroup: true),
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
  ];
}

class PaginatedGroupsGateway extends FakeTeamsGateway {
  final requestedLimits = <int>[];

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    requestedLimits.add(limit);
    return List.generate(
      70.clamp(0, limit),
      (index) => Conversation.chat(
        id: 'group-$index',
        name: 'Group ${index + 1}',
        isGroup: true,
      ),
    );
  }
}

class PaginationRefreshRaceGateway extends PaginatedGroupsGateway {
  final paginationStarted = Completer<void>();
  final releasePagination = Completer<void>();
  var hundredLoads = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    requestedLimits.add(limit);
    if (limit == 100) {
      hundredLoads++;
      if (hundredLoads == 1) {
        paginationStarted.complete();
        await releasePagination.future;
      }
    }
    return List.generate(
      70.clamp(0, limit),
      (index) => Conversation.chat(
        id: 'group-$index',
        name: 'Group ${index + 1}',
        isGroup: true,
      ),
    );
  }
}

class ChannelsGateway extends MessagesGateway {
  @override
  Future<List<TeamSummary>> listTeams() async => const [
    TeamSummary(
      id: 'team-1',
      name: 'Engineering',
      channels: [
        Conversation.channel(
          id: 'channel-general',
          name: 'General',
          teamId: 'team-1',
        ),
      ],
    ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.kind == ConversationKind.channel
      ? const [
          MessageSummary(
            sender: 'Ada Lovelace',
            timestamp: 'Now',
            content: 'Channel message',
          ),
        ]
      : super.readMessages(conversation);
}

class FlakyTeamPhotoGateway extends ChannelsGateway {
  final teamPhoto = Uint8List.fromList(testPng);
  int teamPhotoRequests = 0;

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) async {
    teamPhotoRequests++;
    return teamPhotoRequests == 1 ? null : teamPhoto;
  }
}

class AvatarChannelsGateway extends ChannelsGateway {
  final teamPhoto = Uint8List.fromList(testPng);
  final senderPhoto = Uint8List.fromList(testPng);

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) async => teamPhoto;

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => senderPhoto;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.kind == ConversationKind.channel
      ? const [
          MessageSummary(
            sender: 'Ada Lovelace',
            senderId: 'ada',
            timestamp: 'Now',
            content: 'Channel message',
          ),
        ]
      : super.readMessages(conversation);
}

class FavoriteChannelsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [];

  @override
  Future<List<TeamSummary>> listTeams() async => const [
    TeamSummary(
      id: 'team-engineering',
      name: 'Engineering',
      channels: [
        Conversation.channel(
          id: 'channel-general',
          name: 'General',
          teamId: 'team-engineering',
        ),
      ],
    ),
    TeamSummary(
      id: 'team-operations',
      name: 'Operations',
      channels: [
        Conversation.channel(
          id: 'channel-deployments',
          name: 'Deployments',
          teamId: 'team-operations',
        ),
      ],
    ),
  ];
}

class WatchingChannelsGateway extends ChannelsGateway {
  final events = StreamController<MessageEvent>.broadcast();

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class NewlyDiscoveredChannelGateway extends ChannelsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  int teamLoads = 0;

  @override
  Stream<MessageEvent> messageEvents() => events.stream;

  @override
  Future<List<TeamSummary>> listTeams() async {
    teamLoads++;
    if (teamLoads == 1) return const [];
    return super.listTeams();
  }
}

class StartupChannelWatchingGateway extends ChannelsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final teamsRequested = Completer<void>();
  final releaseTeams = Completer<void>();

  @override
  Stream<MessageEvent> messageEvents() => events.stream;

  @override
  Future<List<TeamSummary>> listTeams() async {
    if (!teamsRequested.isCompleted) teamsRequested.complete();
    await releaseTeams.future;
    return super.listTeams();
  }
}

class BurstyUnknownChannelGateway extends ChannelsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final releaseTeamLoads = Completer<void>();
  int teamLoads = 0;
  int activeTeamLoads = 0;
  int maxConcurrentTeamLoads = 0;

  @override
  Stream<MessageEvent> messageEvents() => events.stream;

  @override
  Future<List<TeamSummary>> listTeams() async {
    teamLoads++;
    if (teamLoads == 1) return const [];
    activeTeamLoads++;
    if (activeTeamLoads > maxConcurrentTeamLoads) {
      maxConcurrentTeamLoads = activeTeamLoads;
    }
    await releaseTeamLoads.future;
    activeTeamLoads--;
    return super.listTeams();
  }
}

class CrossChatDelayedReactionGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final reactionStarted = Completer<void>();
  final releaseReaction = Completer<void>();
  final reactionConversationIds = <String>[];
  bool graceFresh = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async {
    if (conversation.id == 'chat-grace') {
      return [
        MessageSummary(
          id: 'grace-message',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T16:00:00Z',
          content: graceFresh ? 'Grace fresh' : 'Grace stale',
        ),
      ];
    }
    return const [
      MessageSummary(
        id: 'ada-reaction-message',
        sender: 'Ada Lovelace',
        timestamp: '2026-09-04T16:00:00Z',
        content: 'React while switching chats',
      ),
    ];
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    reactionConversationIds.add(conversation.id);
    if (conversation.id != 'chat-ada') return;
    if (!reactionStarted.isCompleted) reactionStarted.complete();
    await releaseReaction.future;
  }
}

class SameMessageIdCrossChatReactionGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final reactionStarted = Completer<void>();
  final releaseReaction = Completer<void>();

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async => [
    MessageSummary(
      id: 'shared-message-id',
      sender: conversation.id == 'chat-ada' ? 'Ada Lovelace' : 'Grace Hopper',
      timestamp: '2026-09-04T18:30:00Z',
      content: conversation.id == 'chat-ada'
          ? 'Ada shared id'
          : 'Grace shared id',
    ),
  ];

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    if (!reactionStarted.isCompleted) reactionStarted.complete();
    await releaseReaction.future;
  }
}

class DelayedReactionGateway extends MessagesGateway {
  final reactionCompleter = Completer<void>();
  String? reactionType;
  bool reactionAccepted = false;
  bool serveStaleReactionAfterAcceptance = false;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'message-1',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-29T06:00:00Z',
          content: 'React to this message',
          reactions: reactionAccepted && !serveStaleReactionAfterAcceptance
              ? const [MessageReaction(type: 'like', count: 1)]
              : const [],
        ),
        const MessageSummary(
          id: 'message-2',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-29T06:06:00Z',
          content: 'Following message',
        ),
      ];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    this.reactionType = reactionType;
    await reactionCompleter.future;
    reactionAccepted = true;
  }
}

class HistoricalReactionGateway extends MessagesGateway {
  bool selected = true;
  int count = 2;
  final removingActions = <bool>[];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'message-history',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-29T06:00:00Z',
          content: 'Historical reaction',
          reactions: [
            MessageReaction(type: 'like', count: count, selected: selected),
          ],
        ),
      ];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    final removing = message.reactions.any(
      (reaction) => reaction.type == reactionType && reaction.selected,
    );
    removingActions.add(removing);
    selected = !removing;
    count += removing ? -1 : 1;
  }
}

class RecordingImageSendGateway extends WatchingMessagesGateway {
  final imageConversationIds = <String>[];

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {
    imageConversationIds.add(conversation.id);
  }
}

class RecordingFileSendGateway extends WatchingMessagesGateway
    implements FileTeamsGateway {
  final fileConversationIds = <String>[];

  @override
  Future<void> sendFileMessage(
    Conversation conversation,
    Uint8List bytes,
    String fileName,
    String contentType,
  ) async {
    fileConversationIds.add(conversation.id);
  }
}

class DelayedSharedImageGateway extends WatchingMessagesGateway {
  final imageSendStarted = Completer<void>();
  final releaseImageSend = Completer<void>();

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {
    if (!imageSendStarted.isCompleted) imageSendStarted.complete();
    await releaseImageSend.future;
  }
}

class CrossChatSendGateway extends WatchingMessagesGateway {
  final adaSendStarted = Completer<void>();
  final releaseAdaSend = Completer<void>();
  final sentConversationIds = <String>[];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    sentConversationIds.add(conversation.id);
    if (conversation.id != 'chat-ada') return;
    if (!adaSendStarted.isCompleted) adaSendStarted.complete();
    await releaseAdaSend.future;
  }
}

class DelayedFailingSendGateway extends WatchingMessagesGateway {
  final sendStarted = Completer<void>();
  final releaseSend = Completer<void>();

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    if (!sendStarted.isCompleted) sendStarted.complete();
    await releaseSend.future;
    throw StateError('send failed');
  }
}

class DelayedSendGateway extends MessagesGateway {
  final sendCompleter = Completer<void>();
  String? sentContent;
  bool accepted = false;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        const MessageSummary(
          id: 'existing-message',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-29T06:00:00Z',
          content: 'Existing message',
        ),
        if (accepted)
          MessageSummary(
            id: 'server-message',
            sender: 'Test User',
            senderId: 'test-user',
            isFromCurrentUser: true,
            timestamp: DateTime.now().toUtc().toIso8601String(),
            content: sentContent!,
          ),
      ];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    sentContent = content;
    await sendCompleter.future;
    accepted = true;
  }
}

class FormattedOwnIdGroupOutgoingGateway extends FakeTeamsGateway {
  bool accepted = false;
  String? sentContent;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'group-own-id', name: 'Project group', isGroup: true),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      accepted
      ? [
          MessageSummary(
            id: 'server-message',
            sender: 'Different display name',
            senderId: '{TEST-USER}',
            timestamp: DateTime.now().toUtc().toIso8601String(),
            content: sentContent!,
          ),
        ]
      : const [];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    sentContent = content;
    accepted = true;
  }
}

class GenericNameOwnIdGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'generic-chat', name: 'Chat', isGroup: false),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'mine',
          sender: 'Different display name',
          senderId: '{TEST-USER}',
          timestamp: '2026-09-04T00:00:00Z',
          content: 'My message',
        ),
        MessageSummary(
          id: 'theirs',
          sender: 'Grace Hopper',
          senderId: 'grace-user',
          timestamp: '2026-09-04T00:01:00Z',
          content: 'Their message',
        ),
      ];
}

class AmbiguousOutgoingGateway extends FakeTeamsGateway {
  bool accepted = false;
  String? sentContent;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      accepted
      ? [
          MessageSummary(
            id: 'server-message',
            sender: 'Test User',
            timestamp: DateTime.now().toUtc().toIso8601String(),
            content: sentContent!,
          ),
        ]
      : const [];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    sentContent = content;
    accepted = true;
  }
}

class CustomReactionIconGateway extends MessagesGateway
    implements CustomReactionTeamsGateway {
  final requestedReactionTypes = <String>[];
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'message-custom-reaction',
          sender: 'Ada Lovelace',
          timestamp: 'Now',
          content: 'Custom reaction',
          reactions: [
            MessageReaction(type: 'party-parrot;0-sau-d4-asset', count: 2),
          ],
        ),
      ];

  @override
  Future<TeamCustomReaction?> getCustomReaction(String reactionType) async {
    requestedReactionTypes.add(reactionType);
    return TeamCustomReaction(
      reactionType: reactionType,
      shortcut: 'party-parrot',
      documentId: '0-sau-d4-asset',
      contentType: 'image/png',
      icon: testPng,
    );
  }
}

class ReactionsGateway extends MessagesGateway {
  String? reactionType;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'message-1',
          sender: 'Ada Lovelace',
          timestamp: 'Now',
          content: 'React to this message',
        ),
      ];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    this.reactionType = reactionType;
  }
}
