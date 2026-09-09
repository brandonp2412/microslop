part of '../widget_test.dart';

class FakeAuthGateway implements AuthGateway {
  int logoutCount = 0;

  @override
  Future<DeviceCodeDetails> beginWorkLogin() async => DeviceCodeDetails(
    verificationUri: 'https://microsoft.com/devicelogin',
    userCode: 'ABC-123',
    expiresInSeconds: BigInt.from(900),
  );

  @override
  Future<AuthSnapshot> restoreWorkSession() => status();

  @override
  Future<void> cancelWorkLogin() async {}

  @override
  Future<AuthSnapshot> logout() async {
    logoutCount++;
    return const AuthSnapshot(signedIn: false, loginInProgress: false);
  }

  @override
  Future<AuthSnapshot> completeWorkLogin() async =>
      const AuthSnapshot(signedIn: true, loginInProgress: false);

  @override
  Future<AuthSnapshot> status() async =>
      const AuthSnapshot(signedIn: false, loginInProgress: false);
}

class PendingLoginAuthGateway extends FakeAuthGateway {
  @override
  Future<AuthSnapshot> completeWorkLogin() => Completer<AuthSnapshot>().future;
}

class FailingAuthGateway extends FakeAuthGateway {
  @override
  Future<DeviceCodeDetails> beginWorkLogin() async {
    throw Exception('Unable to store the secure sign-in credential.');
  }
}

class FailingRestoreAuthGateway extends FakeAuthGateway {
  @override
  Future<AuthSnapshot> restoreWorkSession() async {
    throw Exception('The session could not be refreshed.');
  }
}

class RestoringAuthGateway extends FakeAuthGateway {
  @override
  Future<AuthSnapshot> restoreWorkSession() async =>
      const AuthSnapshot(signedIn: true, loginInProgress: false);
}

class FailingMessageStopGateway extends FakeTeamsGateway {
  int messageEventStarts = 0;
  int stopCount = 0;

  @override
  Stream<MessageEvent> messageEvents() {
    messageEventStarts++;
    return const Stream<MessageEvent>.empty();
  }

  @override
  Future<void> stopMessageEvents() async {
    stopCount++;
    throw StateError('message stop failed');
  }
}

class DelayedMessageStopGateway extends FakeTeamsGateway {
  final stopStarted = Completer<void>();
  final releaseStop = Completer<void>();
  int messageEventStarts = 0;
  int stopCount = 0;

  @override
  Stream<MessageEvent> messageEvents() {
    messageEventStarts++;
    return const Stream<MessageEvent>.empty();
  }

  @override
  Future<void> stopMessageEvents() async {
    stopCount++;
    if (!stopStarted.isCompleted) stopStarted.complete();
    await releaseStop.future;
  }
}

class MultiAccountPendingLoginAuthGateway
    extends MultiAccountRestoringAuthGateway {
  @override
  Future<AuthSnapshot> completeWorkLogin() => Completer<AuthSnapshot>().future;
}

class FailingPostSwitchAccountLookupAuthGateway
    extends MultiAccountRestoringAuthGateway {
  int activeAccountIdReads = 0;

  @override
  Future<String?> activeAccountId() async {
    activeAccountIdReads++;
    if (activeAccountIdReads > 1) {
      throw StateError('active account lookup failed');
    }
    return super.activeAccountId();
  }
}

class FailingActiveAccountLookupAuthGateway
    extends MultiAccountRestoringAuthGateway {
  @override
  Future<String?> activeAccountId() async {
    throw StateError('active account lookup failed');
  }
}

class FailingActiveAccountLookupLoginAuthGateway
    extends FailingActiveAccountLookupAuthGateway {
  int completionReads = 0;

  @override
  Future<AuthSnapshot> restoreWorkSession() async =>
      const AuthSnapshot(signedIn: false, loginInProgress: false);

  @override
  Future<AuthSnapshot> completeWorkLogin() async {
    completionReads++;
    return completionReads == 1
        ? const AuthSnapshot(signedIn: false, loginInProgress: true)
        : const AuthSnapshot(signedIn: true, loginInProgress: false);
  }
}

class MultiAccountRestoringAuthGateway extends RestoringAuthGateway
    implements MultiAccountAuthGateway {
  String activeId = 'account-primary';
  final switchedAccounts = <String>[];

  @override
  Future<String?> activeAccountId() async => activeId;

  @override
  Future<List<SavedAccountSummary>> savedAccounts() async => const [
    SavedAccountSummary(
      id: 'account-primary',
      displayName: 'Test User',
      username: 'test@example.com',
    ),
    SavedAccountSummary(
      id: 'account-secondary',
      displayName: 'Other Account',
      username: 'other@example.com',
    ),
  ];

  @override
  Future<AuthSnapshot> switchAccount(String accountId) async {
    activeId = accountId;
    switchedAccounts.add(accountId);
    return const AuthSnapshot(signedIn: true, loginInProgress: false);
  }
}

final testPng = Uint8List.fromList(
  base64Decode(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ'
    'AAAADUlEQVQIHWP4z8DwHwAFgAI/ScL8SAAAAABJRU5ErkJggg==',
  ),
);

class CallIdentityTeamsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-call-identity',
      name: 'Chat with Ada',
      isGroup: false,
      profilePhotoUserId: '22222222-2222-2222-2222-222222222222',
    ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          sender: 'Ada',
          senderId: '33333333-3333-3333-3333-333333333333',
          timestamp: 'Now',
          content: 'Hello',
        ),
      ];
}

class MemberIdentityTeamsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-member-identity',
      name: 'Chat with Test Peer',
      isGroup: false,
      avatarUserIds: ['test-user', '44444444-4444-4444-4444-444444444444'],
    ),
  ];
}

class FakeTeamsGateway implements TeamsGateway {
  @override
  Future<UserSummary> getUser() async =>
      const UserSummary(id: 'test-user', displayName: 'Test User');

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => null;

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async => null;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-1',
      name: 'Chat with Ada',
      isGroup: false,
      profilePhotoUserId: '22222222-2222-2222-2222-222222222222',
    ),
  ];

  @override
  Future<List<TeamSummary>> listTeams() async => const [];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {}

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {}

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) async => null;

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {}

  @override
  Stream<MessageEvent> messageEvents() => const Stream.empty();

  @override
  Future<void> stopMessageEvents() async {}
}

class UserHoverGateway extends FakeTeamsGateway
    implements UserDetailsTeamsGateway {
  final requestedUserIds = <String>[];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'message-user-hover',
          sender: 'Ada Lovelace',
          senderId: 'ada-user',
          timestamp: '2026-09-08T00:00:00Z',
          content: 'Hover me',
        ),
      ];

  @override
  Future<UserDetailsSummary?> getUserDetails(String userId) async {
    requestedUserIds.add(userId);
    return const UserDetailsSummary(
      userId: 'ada-user',
      displayName: 'Ada Lovelace',
      email: 'ada@example.com',
      jobTitle: 'Principal Engineer',
      availability: 'Busy',
      activity: 'InACall',
      statusMessage: 'Heads down',
    );
  }
}

class DynamicPresenceGateway extends FakeTeamsGateway
    implements PresenceTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final requestedUserIds = <List<String>>[];
  final presenceStarted = Completer<void>();
  final releaseFirstPresence = Completer<void>();
  bool revealGrace = false;
  bool delayFirstPresence = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => [
    const Conversation.chat(
      id: 'chat-ada-presence',
      name: 'Ada Lovelace',
      isGroup: false,
      profilePhotoUserId: 'ada-user',
    ),
    if (revealGrace)
      const Conversation.chat(
        id: 'chat-grace-presence',
        name: 'Grace Hopper',
        isGroup: false,
        profilePhotoUserId: 'grace-user',
      ),
  ];

  @override
  Future<List<PresenceSummary>> getPresences(List<String> userIds) async {
    requestedUserIds.add(List.unmodifiable(userIds));
    if (delayFirstPresence && requestedUserIds.length == 1) {
      if (!presenceStarted.isCompleted) presenceStarted.complete();
      await releaseFirstPresence.future;
    }
    return [
      if (userIds.contains('ada-user'))
        const PresenceSummary(
          userId: 'ada-user',
          availability: 'Busy',
          activity: 'InACall',
        ),
      if (userIds.contains('grace-user'))
        const PresenceSummary(
          userId: 'grace-user',
          availability: 'DoNotDisturb',
          activity: 'Presenting',
        ),
    ];
  }

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class PresenceGateway extends FakeTeamsGateway implements PresenceTeamsGateway {
  final requestedUserIds = <String>[];

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-ada-presence',
      name: 'Ada Lovelace',
      isGroup: false,
      profilePhotoUserId: 'ada-user',
    ),
    Conversation.chat(
      id: 'chat-self-presence',
      name: 'Test User',
      isGroup: false,
      profilePhotoUserId: '{TEST-USER}',
    ),
  ];

  @override
  Future<List<PresenceSummary>> getPresences(List<String> userIds) async {
    requestedUserIds
      ..clear()
      ..addAll(userIds);
    return [
      if (userIds.contains('ada-user'))
        const PresenceSummary(
          userId: 'ADA-USER',
          availability: 'Busy',
          activity: 'InACall',
        ),
      if (userIds.contains('test-user'))
        const PresenceSummary(
          userId: 'test-user',
          availability: 'Available',
          activity: 'Available',
        ),
    ];
  }
}

class DelayedChatsGateway extends FakeTeamsGateway {
  final _chatsCompleter = Completer<List<Conversation>>();

  @override
  Future<List<Conversation>> listChats({int limit = 50}) =>
      _chatsCompleter.future;

  void complete() => _chatsCompleter.complete(const []);
}

class LinkMessageGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'message-link',
          sender: 'Ada',
          timestamp: '2026-08-30T12:00:00Z',
          content: 'Read https://example.com/docs today',
        ),
      ];
}

class EmojiLeadingNameGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'chat-emoji', name: '😀 Ada', isGroup: false),
  ];
}

class FlakyProfilePhotoGateway extends FakeTeamsGateway {
  final profilePhoto = Uint8List.fromList(testPng);
  int profilePhotoRequests = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-ada-flaky-photo',
      name: 'Chat with Ada',
      isGroup: false,
      profilePhotoUserId: 'ada',
    ),
  ];

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async {
    profilePhotoRequests++;
    return profilePhotoRequests <= 2 ? null : profilePhoto;
  }
}

class AvatarChatsGateway extends FakeTeamsGateway {
  final gracePhoto = Uint8List.fromList(
    base64Decode(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ'
      'AAAADUlEQVQIHWP4z8DwHwAFgAI/ScL8SAAAAABJRU5ErkJggg==',
    ),
  );
  final _adaPhoto = Uint8List.fromList(
    base64Decode(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ'
      'AAAADUlEQVQIHWP4z8DwHwAFgAI/ScL8SAAAAABJRU5ErkJggg==',
    ),
  );

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'chat-ada',
      name: 'Chat with Ada',
      isGroup: false,
      profilePhotoUserId: 'ada',
    ),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
      profilePhotoUserId: 'grace',
    ),
  ];

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => switch (userId) {
    'ada' => _adaPhoto,
    'grace' => gracePhoto,
    _ => null,
  };
}

class CodeBlockGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(
    Conversation conversation,
  ) async => const [
    MessageSummary(
      id: 'code-1',
      sender: 'Ada Lovelace',
      timestamp: '2026-09-05T10:00:00Z',
      content:
          'Before\n```dart\nfinal answer = 42;\nhttps://example.com/code\n```\nAfter',
    ),
  ];
}

class ImageMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          sender: 'Ada Lovelace',
          timestamp: 'Now',
          content: '',
          images: [MessageImage(contentType: 'image/png', bytes: testPng)],
        ),
      ];
}

class GalleryMessagesGateway extends FakeTeamsGateway
    implements ImageHistoryTeamsGateway {
  final images = [
    MessageImage(
      contentType: 'image/png',
      bytes: base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAKUlEQVR4nO3NsQ0AAAzCMP5/mj5RNkuZ4zSZtr0DAAAAAAAAAACA/gEHMsP8LofXz44AAAAASUVORK5CYII=',
      ),
    ),
    MessageImage(
      contentType: 'image/png',
      bytes: base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAJ0lEQVR4nO3NMQkAAAwDsPo33anoMQjkT9JsCQQCgUAgEAgEgj4JDjaH/C7PsHgbAAAAAElFTkSuQmCC',
      ),
    ),
    MessageImage(
      contentType: 'image/png',
      bytes: base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAJ0lEQVR4nO3NMQkAAAwDsPo33anoMQjkT5KOCQQCgUAgEAgEgv4IDjpL/C4b7k3TAAAAAElFTkSuQmCC',
      ),
    ),
  ];

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'self-chat', name: 'brandonp2412', isGroup: false),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        for (var index = 0; index < 2; index++)
          MessageSummary(
            id: 'image-$index',
            sender: 'brandonp2412',
            timestamp: '2026-09-05T10:0$index:00Z',
            content: '',
            images: [images[index]],
          ),
      ];

  @override
  Future<List<MessageSummary>> readImageHistory(
    Conversation conversation,
  ) async => [
    ...await readMessages(conversation),
    MessageSummary(
      id: 'older-image',
      sender: 'brandonp2412',
      timestamp: '2026-09-04T10:00:00Z',
      content: '',
      images: [images[2]],
    ),
  ];
}

class MissingImageGateway extends GalleryMessagesGateway {
  bool available = false;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'missing-image',
          sender: 'brandonp2412',
          timestamp: '2026-09-05T10:24:00Z',
          content: '',
          images: available ? [images.first] : const [],
        ),
      ];
}

class GroupAvatarGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'group-1',
      name: 'Project group',
      isGroup: true,
      avatarUserIds: ['ada', 'grace'],
    ),
  ];

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => testPng;
}

class FlakyGroupPhotoGateway extends GroupAvatarGateway {
  final chatPhoto = Uint8List.fromList(testPng);
  int chatPhotoRequests = 0;

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async {
    chatPhotoRequests++;
    return chatPhotoRequests == 1 ? null : chatPhoto;
  }
}

class MeetingAvatarGateway extends GroupAvatarGateway {
  int chatPhotoRequests = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: '19:meeting_MDAwMDAwMDAtMDAwMC00MDAwLTgwMDAtMDAwMDAwMDAwMDAw@thread.v2',
      name: 'Absolute Zero: Stand-Up',
      isGroup: true,
    ),
  ];

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async {
    chatPhotoRequests++;
    return null;
  }
}

class CountingGroupAvatarGateway extends GroupAvatarGateway {
  int chatPhotoRequests = 0;
  int profilePhotoRequests = 0;

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async {
    chatPhotoRequests++;
    return null;
  }

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async {
    profilePhotoRequests++;
    return testPng;
  }
}

class NamedGroupAvatarGateway extends GroupAvatarGateway {
  final chatPhoto = testPng;
  String? requestedChatId;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'bingo-bus',
      name: 'Bingo Bus',
      isGroup: true,
      avatarUserIds: ['ada', 'grace'],
    ),
  ];

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async {
    requestedChatId = chatId;
    return chatPhoto;
  }
}

class GroupWithProfileAvatarGateway extends GroupAvatarGateway {
  final chatPhoto = Uint8List.fromList(testPng);
  String? requestedChatId;
  final requestedProfileUserIds = <String>[];

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'consumer-stc-chat',
      name: 'Consumer STC',
      isGroup: true,
      profilePhotoUserId: 'consumer-stc',
      avatarUserIds: ['ada', 'grace'],
    ),
  ];

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async {
    requestedChatId = chatId;
    return chatPhoto;
  }

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async {
    requestedProfileUserIds.add(userId);
    return Uint8List.fromList(testPng);
  }
}

class MessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          sender: 'Ada Lovelace',
          timestamp: 'Now',
          content: 'Newest message',
        ),
      ];
}

class QuotedMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'quoted-1',
          sender: 'Ada Lovelace',
          timestamp: 'Now',
          content: 'Reply body only',
          quotes: [
            MessageQuote(
              messageId: 'original-1',
              sender: 'Grace Hopper',
              content: 'Original quoted message',
            ),
          ],
        ),
      ];
}

class GroupedIncomingMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'grouped-1',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-31T00:00:00Z',
          content: 'First grouped message',
        ),
        MessageSummary(
          id: 'grouped-2',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-31T00:01:00Z',
          content: 'Second grouped message',
        ),
      ];
}

class PersistentSyncingGateway extends FakeTeamsGateway
    implements MessageSyncTeamsGateway {
  final started = Completer<void>();
  final finish = Completer<void>();

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    onProgress(0, 2);
    onProgress(1, 2);
    started.complete();
    await finish.future;
    onProgress(2, 2);
  }
}

class FullyCachedNavigationGateway extends FakeTeamsGateway
    implements MessageSyncTeamsGateway {
  final requestedLimits = <int>[];
  final allChats = List.generate(
    80,
    (index) => Conversation.chat(
      id: 'chat-${index + 1}',
      name: 'Chat ${index + 1}',
      isGroup: false,
    ),
  );

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    requestedLimits.add(limit);
    return allChats.take(limit).toList();
  }

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    onProgress(0, allChats.length);
    for (var index = 0; index < allChats.length; index++) {
      await onMessages(allChats[index], const []);
      onProgress(index + 1, allChats.length);
    }
  }
}

class BurstyWorkspaceLoadGateway extends FakeTeamsGateway {
  final resumeLoadStarted = Completer<void>();
  final releaseResumeLoads = Completer<void>();
  int listCalls = 0;
  int activeResumeLoads = 0;
  int maxConcurrentResumeLoads = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    listCalls++;
    if (listCalls == 1) return super.listChats(limit: limit);
    activeResumeLoads++;
    if (activeResumeLoads > maxConcurrentResumeLoads) {
      maxConcurrentResumeLoads = activeResumeLoads;
    }
    if (!resumeLoadStarted.isCompleted) resumeLoadStarted.complete();
    await releaseResumeLoads.future;
    activeResumeLoads--;
    return super.listChats(limit: limit);
  }
}

class SyncChatDiscoveryRaceGateway extends FakeTeamsGateway
    implements MessageSyncTeamsGateway {
  final syncStarted = Completer<void>();
  final releaseSync = Completer<void>();
  bool revealGrace = false;
  int listCalls = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    listCalls++;
    return [
      const Conversation.chat(
        id: 'chat-ada-sync-race',
        name: 'Chat with Ada',
        isGroup: false,
      ),
      if (revealGrace)
        const Conversation.chat(
          id: 'chat-grace-sync-race',
          name: 'Chat with Grace',
          isGroup: false,
        ),
    ];
  }

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    const conversation = Conversation.chat(
      id: 'chat-ada-sync-race',
      name: 'Chat with Ada',
      isGroup: false,
    );
    onProgress(0, 1);
    await onMessages(conversation, const []);
    onProgress(1, 1);
    if (!syncStarted.isCompleted) syncStarted.complete();
    await releaseSync.future;
  }
}

class PendingSendSyncRaceGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway, MessageSyncTeamsGateway {
  final syncStarted = Completer<void>();
  final releaseSync = Completer<void>();
  final sendStarted = Completer<void>();
  final releaseSend = Completer<void>();
  bool sendAccepted = false;

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async => [
    const MessageSummary(
      id: 'baseline-message',
      sender: 'Ada Lovelace',
      timestamp: '2026-09-04T14:00:00Z',
      content: 'Baseline history',
    ),
    if (sendAccepted)
      const MessageSummary(
        id: 'server-send',
        sender: 'Test User',
        senderId: 'test-user',
        isFromCurrentUser: true,
        timestamp: '2026-09-04T14:03:00Z',
        content: 'Pending local send',
      ),
  ];

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    if (!sendStarted.isCompleted) sendStarted.complete();
    await releaseSend.future;
    sendAccepted = true;
  }

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    final conversation = (await listChats()).single;
    onProgress(0, 1);
    if (!syncStarted.isCompleted) syncStarted.complete();
    await releaseSync.future;
    await onMessages(conversation, const [
      MessageSummary(
        id: 'baseline-message',
        sender: 'Ada Lovelace',
        timestamp: '2026-09-04T14:00:00Z',
        content: 'Baseline history',
      ),
    ]);
    onProgress(1, 1);
  }
}

class DelayedSyncRefreshRaceGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway, MessageSyncTeamsGateway {
  final syncStarted = Completer<void>();
  final releaseSync = Completer<void>();

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async => const [
    MessageSummary(
      id: 'fresh-live-message',
      sender: 'Ada Lovelace',
      timestamp: '2026-09-04T14:02:00Z',
      content: 'Fresh live history',
    ),
  ];

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    final conversation = (await listChats()).single;
    onProgress(0, 1);
    if (!syncStarted.isCompleted) syncStarted.complete();
    await releaseSync.future;
    await onMessages(conversation, const [
      MessageSummary(
        id: 'stale-sync-message',
        sender: 'Ada Lovelace',
        timestamp: '2026-09-04T14:01:00Z',
        content: 'Stale synced history',
      ),
    ]);
    onProgress(1, 1);
  }
}

class CachedSyncResponsiveGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway, MessageSyncTeamsGateway {
  final _refresh = Completer<List<MessageSummary>>();

  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    final conversation = (await listChats()).single;
    onProgress(0, 1);
    await onMessages(conversation, const [
      MessageSummary(
        id: 'synced-message',
        sender: 'Ada Lovelace',
        timestamp: '2026-08-31T02:00:00Z',
        content: 'Synced history',
      ),
    ]);
    onProgress(1, 1);
  }

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) =>
      _refresh.future;

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  void completeRefresh() {
    if (!_refresh.isCompleted) {
      _refresh.complete(const [
        MessageSummary(
          id: 'fresh-message',
          sender: 'Ada Lovelace',
          timestamp: '2026-08-31T02:01:00Z',
          content: 'Fresh history',
        ),
      ]);
    }
  }
}

class SyncingGateway extends FakeTeamsGateway
    implements MessageSyncTeamsGateway {
  @override
  Future<void> syncRecentMessages(
    int perConversation,
    void Function(int completed, int total) onProgress,
    Future<void> Function(
      Conversation conversation,
      List<MessageSummary> messages,
    )
    onMessages,
  ) async {
    onProgress(0, 2);
    await Future<void>.delayed(Duration.zero);
    onProgress(1, 2);
    await Future<void>.delayed(Duration.zero);
    onProgress(2, 2);
  }
}

class MentionSuggestionsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(
      id: 'group-mentions',
      name: 'Project group',
      isGroup: true,
    ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(sender: 'Ada Lovelace', timestamp: 'Now', content: 'A'),
        MessageSummary(sender: 'Grace Hopper', timestamp: 'Now', content: 'G'),
      ];
}

class CrossChatMentionSuggestionsGateway extends FakeTeamsGateway {
  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'group-alpha', name: 'Alpha group', isGroup: true),
    Conversation.chat(id: 'group-beta', name: 'Beta group', isGroup: true),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.id == 'group-alpha'
      ? const [
          MessageSummary(sender: 'Ada Alpha', timestamp: 'Now', content: 'A'),
          MessageSummary(sender: 'Grace Alpha', timestamp: 'Now', content: 'G'),
        ]
      : const [
          MessageSummary(
            sender: 'Barbara Beta',
            timestamp: 'Now',
            content: 'B',
          ),
          MessageSummary(
            sender: 'Dorothy Beta',
            timestamp: 'Now',
            content: 'D',
          ),
        ];
}

class OwnMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          sender: 'Test User',
          timestamp: 'Now',
          content: 'My message',
        ),
      ];
}

class MismatchedOutgoingMessageGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          sender: 'Ada Lovelace',
          senderId: '{TEST-USER}',
          timestamp: 'Now',
          content: 'Message from me',
        ),
      ];
}

class MixedMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          sender: 'Ada Lovelace',
          timestamp: '2026-08-28T09:00:00Z',
          content: 'Incoming message',
        ),
        MessageSummary(
          sender: 'Test User',
          timestamp: '2026-08-28T09:01:00Z',
          content: 'Outgoing message',
        ),
      ];
}

class FailingMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async {
    throw Exception('Unable to load messages');
  }
}

class FailingRejectedMessagesGateway extends FakeTeamsGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async {
    throw Exception('Teams rejected the request');
  }
}

class HistoricalSavedAccountReactionGateway extends WatchingMessagesGateway
    implements MultiAccountTeamsGateway {
  bool reacted = false;
  final requestedLimits = <int>[];

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    requestedLimits.add(recentMessageLimit);
    const latest = MessageSummary(
      id: 'latest-other-account-own-message',
      sender: 'Other Account',
      isFromCurrentUser: true,
      timestamp: 'Later',
      content: 'Latest message from me',
    );
    final older = MessageSummary(
      id: 'older-other-account-own-message',
      sender: 'Other Account',
      isFromCurrentUser: true,
      timestamp: 'Earlier',
      content: 'Earlier message from me',
      reactions: reacted
          ? const [MessageReaction(type: 'heart', count: 1)]
          : const [],
    );
    return SavedAccountNotificationSummary(
      accountId: accountId,
      accountName: 'Other Account',
      conversationId: conversationId,
      conversationName: 'Chat with Grace',
      isGroup: false,
      isChannel: false,
      message: latest,
      messages: [older, latest],
    );
  }
}

class SavedAccountReactionWatchingGateway extends WatchingMessagesGateway
    implements MultiAccountTeamsGateway {
  int reactionCount = 1;

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async => SavedAccountNotificationSummary(
    accountId: accountId,
    accountName: 'Other Account',
    conversationId: conversationId,
    conversationName: 'Chat with Grace',
    isGroup: false,
    isChannel: false,
    message: MessageSummary(
      id: 'other-account-own-message',
      sender: 'Other Account',
      isFromCurrentUser: true,
      timestamp: 'Now',
      content: 'My message',
      reactions: [MessageReaction(type: 'heart', count: reactionCount)],
    ),
  );
}

class OutOfOrderSavedAccountGateway extends WatchingMessagesGateway
    implements MultiAccountTeamsGateway {
  final firstReadStarted = Completer<void>();
  final releaseFirstRead = Completer<void>();
  int reads = 0;

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    reads++;
    final first = reads == 1;
    if (first) {
      if (!firstReadStarted.isCompleted) firstReadStarted.complete();
      await releaseFirstRead.future;
    }
    return SavedAccountNotificationSummary(
      accountId: accountId,
      accountName: 'Other Account',
      conversationId: conversationId,
      conversationName: 'Chat with Grace',
      isGroup: false,
      isChannel: false,
      message: MessageSummary(
        id: first ? 'older-saved-message' : 'newer-saved-message',
        sender: 'Grace Hopper',
        timestamp: first ? 'Earlier' : 'Later',
        content: first ? 'Older saved activity' : 'Newer saved activity',
      ),
    );
  }
}

class DelayedSavedAccountGateway extends WatchingMessagesGateway
    implements MultiAccountTeamsGateway {
  final fetchStarted = Completer<void>();
  final releaseFetch = Completer<void>();

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    if (!fetchStarted.isCompleted) fetchStarted.complete();
    await releaseFetch.future;
    return SavedAccountNotificationSummary(
      accountId: accountId,
      accountName: 'Other Account',
      conversationId: conversationId,
      conversationName: 'Chat with Grace',
      isGroup: false,
      isChannel: false,
      message: const MessageSummary(
        id: 'delayed-other-account-message',
        sender: 'Grace Hopper',
        timestamp: 'Now',
        content: 'Delayed message from the other account',
      ),
    );
  }
}

class MultiAccountWatchingGateway extends WatchingMessagesGateway
    implements MultiAccountTeamsGateway {
  final requestedLimits = <int>[];

  @override
  Future<SavedAccountNotificationSummary?> savedAccountNotification(
    String accountId,
    String conversationId, {
    int recentMessageLimit = 1,
  }) async {
    requestedLimits.add(recentMessageLimit);
    return SavedAccountNotificationSummary(
      accountId: accountId,
      accountName: 'Other Account',
      conversationId: conversationId,
      conversationName: 'Chat with Grace',
      isGroup: false,
      isChannel: false,
      message: const MessageSummary(
        id: 'other-account-message',
        sender: 'Grace Hopper',
        timestamp: 'Now',
        content: 'Message from the other account',
      ),
    );
  }
}

class StartupConversationWatchingGateway extends FakeTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  final chatsRequested = Completer<void>();
  final releaseChats = Completer<void>();

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    if (!chatsRequested.isCompleted) chatsRequested.complete();
    await releaseChats.future;
    return const [
      Conversation.chat(
        id: 'chat-startup',
        name: 'Chat with Startup Grace',
        isGroup: false,
      ),
    ];
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'startup-message',
          sender: 'Grace Hopper',
          senderId: 'grace-user',
          timestamp: 'Now',
          content: 'Arrived during startup',
        ),
      ];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class NewConversationWatchingGateway extends FakeTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  bool revealNewConversation = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => [
    const Conversation.chat(
      id: 'chat-ada',
      name: 'Chat with Ada',
      isGroup: false,
    ),
    if (revealNewConversation)
      const Conversation.chat(
        id: 'chat-new',
        name: 'Chat with Grace',
        isGroup: false,
      ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.id == 'chat-new'
      ? const [
          MessageSummary(
            id: 'new-chat-message',
            sender: 'Grace Hopper',
            senderId: 'grace-user',
            timestamp: 'Now',
            content: 'Hello from a new chat',
          ),
        ]
      : const [];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class OwnIdVariationWatchingGateway extends WatchingMessagesGateway {
  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'own-message',
          sender: 'Different display name',
          senderId: '{TEST-USER}',
          timestamp: 'Now',
          content: 'Own message',
        ),
      ];
}

class OutOfOrderActivityGateway extends WatchingMessagesGateway {
  final firstReadStarted = Completer<void>();
  final releaseFirstRead = Completer<void>();
  int reads = 0;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async {
    reads++;
    if (reads == 1) {
      if (!firstReadStarted.isCompleted) firstReadStarted.complete();
      await releaseFirstRead.future;
      return const [
        MessageSummary(
          id: 'older-message',
          sender: 'Ada Lovelace',
          senderId: 'ada-user',
          timestamp: '2026-09-04T09:00:00Z',
          content: 'Older activity',
        ),
      ];
    }
    return const [
      MessageSummary(
        id: 'newer-message',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: '2026-09-04T09:01:00Z',
        content: 'Newer activity',
      ),
    ];
  }
}

class HistoricalReactionNotificationGateway extends WatchingMessagesGateway {
  bool reacted = false;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'older-own-message',
          sender: 'Test User',
          senderId: 'test-user',
          isFromCurrentUser: true,
          timestamp: '2026-09-04T09:00:00Z',
          content: 'Earlier message from me',
          reactions: reacted
              ? const [MessageReaction(type: 'heart', count: 1)]
              : const [],
        ),
        const MessageSummary(
          id: 'latest-own-message',
          sender: 'Test User',
          senderId: 'test-user',
          isFromCurrentUser: true,
          timestamp: '2026-09-04T09:01:00Z',
          content: 'Latest message from me',
        ),
      ];
}

class FailingQueuedRefreshGateway extends WatchingMessagesGateway {
  final refreshStarted = Completer<void>();
  final releaseFailure = Completer<void>();
  int listCalls = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    listCalls++;
    if (listCalls == 2) {
      if (!refreshStarted.isCompleted) refreshStarted.complete();
      await releaseFailure.future;
      throw StateError('refresh failed');
    }
    return super.listChats(limit: limit);
  }
}

class StaleQueuedReconcileGateway extends WatchingMessagesGateway {
  final refreshStarted = Completer<void>();
  final releaseStaleRefresh = Completer<void>();
  final newerLoadApplied = Completer<void>();
  int listCalls = 0;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async {
    listCalls++;
    if (listCalls == 2) {
      if (!refreshStarted.isCompleted) refreshStarted.complete();
      await releaseStaleRefresh.future;
    }
    if (listCalls == 3 && !newerLoadApplied.isCompleted) {
      newerLoadApplied.complete();
    }
    final changed = listCalls >= 4;
    return [
      const Conversation.chat(
        id: 'chat-ada',
        name: 'Chat with Ada',
        isGroup: false,
        preview: 'Ada preview',
      ),
      Conversation.chat(
        id: 'chat-grace',
        name: 'Chat with Grace',
        isGroup: false,
        lastMessageId: changed ? 'grace-new' : 'grace-old',
        preview: changed ? 'New Grace preview' : 'Old Grace preview',
      ),
    ];
  }

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      conversation.id == 'chat-grace'
      ? const [
          MessageSummary(
            id: 'grace-new',
            sender: 'Grace Hopper',
            senderId: 'grace-user',
            timestamp: 'Now',
            content: 'Queued reconcile message',
          ),
        ]
      : super.readMessages(conversation);
}

class NotificationRetryGateway extends WatchingMessagesGateway {
  bool latest = false;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async {
    readCount++;
    readCounts.update(conversation.id, (count) => count + 1, ifAbsent: () => 1);
    if (conversation.id != 'chat-grace') {
      return super.readMessages(conversation);
    }
    return [
      MessageSummary(
        id: latest ? 'grace-latest' : 'grace-baseline',
        sender: 'Grace Hopper',
        senderId: 'grace-user',
        timestamp: latest ? '2026-09-04T17:20:00Z' : '2026-09-04T17:19:00Z',
        content: latest ? 'Retry this notification' : 'Grace baseline',
      ),
    ];
  }
}

class WatchingMessagesGateway extends FakeTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  bool showLatestMessage = false;
  int readCount = 0;
  int stopCount = 0;
  final readCounts = <String, int>{};

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
  Future<List<MessageSummary>> readMessages(Conversation conversation) async {
    readCount++;
    readCounts.update(conversation.id, (count) => count + 1, ifAbsent: () => 1);
    return [
      MessageSummary(
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: 'Now',
        content: showLatestMessage ? 'Latest message' : 'First message',
      ),
    ];
  }

  @override
  Stream<MessageEvent> messageEvents() => events.stream;

  @override
  Future<void> stopMessageEvents() async {
    stopCount++;
  }
}

class LazyGalleryGateway extends GalleryMessagesGateway
    implements LazyImageTeamsGateway {
  final history = Completer<List<MessageSummary>>();
  final download = Completer<MessageImage?>();
  final requests = <String>[];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'image-0',
          sender: 'brandonp2412',
          timestamp: '2026-09-05T10:00:00Z',
          content: '',
          images: [images.first],
        ),
      ];

  @override
  Future<List<MessageSummary>> readImageHistory(Conversation conversation) =>
      history.future;

  @override
  Future<MessageImage?> loadMessageImage(String url) {
    requests.add(url);
    return download.future;
  }
}

class LoadingImageGateway extends MissingImageGateway
    implements LazyImageTeamsGateway {
  final download = Completer<MessageImage?>();

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'loading-image',
          sender: 'brandonp2412',
          timestamp: '2026-09-05T10:00:00Z',
          content: '',
          imageUrls: ['image-source'],
        ),
      ];

  @override
  Future<MessageImage?> loadMessageImage(String url) => download.future;
}

class CachedImageCountGateway extends FakeTeamsGateway
    implements ResponsiveTeamsGateway {
  final events = StreamController<MessageEvent>.broadcast();
  bool imageLoaded = false;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => const [
    Conversation.chat(id: 'self-chat', name: 'brandonp2412', isGroup: false),
  ];

  List<MessageSummary> get messages => [
    MessageSummary(
      id: 'cached-image',
      sender: 'brandonp2412',
      timestamp: '2026-09-05T10:00:00Z',
      content: '',
      cachedImageCount: 1,
      images: imageLoaded
          ? [MessageImage(contentType: 'image/png', bytes: testPng)]
          : const [],
    ),
  ];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      messages;

  @override
  Future<List<MessageSummary>> refreshMessages(
    Conversation conversation,
  ) async => messages;

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Stream<MessageEvent> messageEvents() => events.stream;
}

class ReactionPeopleGateway extends GalleryMessagesGateway
    implements ReactionUsersTeamsGateway {
  final name = Completer<String>();
  final requests = <String>[];

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      const [
        MessageSummary(
          id: 'people',
          sender: 'brandonp2412',
          timestamp: '2026-09-05T10:00:00Z',
          content: 'Self-chat reaction',
          reactions: [
            MessageReaction(
              type: 'like',
              count: 2,
              users: [
                ReactionUser(id: 'ada', name: 'Ada Lovelace'),
                ReactionUser(id: 'grace', name: ''),
              ],
            ),
          ],
        ),
      ];

  @override
  Future<String> reactionUserName(String userId) {
    requests.add(userId);
    return name.future;
  }
}
