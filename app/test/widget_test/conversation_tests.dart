part of '../widget_test.dart';

void registerConversationTests() {
  testWidgets('autocompletes message sender mentions', (tester) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField).last, '@Ad');
    await tester.pump();

    expect(find.text('Ada'), findsOneWidget);
    await tester.tap(find.text('Ada'));
    await tester.pump();
    expect(find.byType(TextField).last, findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      '@Ada ',
    );
  });

  testWidgets('shows and cycles desktop mention suggestions', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: MentionSuggestionsGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Project group'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.enterText(composer, '@');
    await tester.pump();

    expect(find.text('People in this conversation'), findsOneWidget);
    expect(find.text('Ada Lovelace'), findsWidgets);
    expect(find.text('Grace Hopper'), findsWidgets);
    expect(find.text('Tab to select'), findsOneWidget);
    expect(find.text('↑↓ to navigate'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();

    expect(
      tester.widget<TextField>(composer).controller!.text,
      '@Grace Hopper ',
    );
  });

  testWidgets('wraps desktop mention selection with arrow keys', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: MentionSuggestionsGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Project group'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.enterText(composer, '@');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();

    expect(
      tester.widget<TextField>(composer).controller!.text,
      '@Grace Hopper ',
    );
  });

  testWidgets('mention selection resets when switching conversations', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: CrossChatMentionSuggestionsGateway(),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Beta group'));
    await tester.pumpAndSettle();
    var composer = find.byType(TextField).last;
    await tester.enterText(composer, '@');
    await tester.pump();

    await tester.tap(find.text('Alpha group'));
    await tester.pumpAndSettle();
    composer = find.byType(TextField).last;
    await tester.enterText(composer, '@');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();

    await tester.tap(find.text('Beta group'));
    await tester.pumpAndSettle();
    composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();

    expect(
      tester.widget<TextField>(composer).controller!.text,
      '@Barbara Beta ',
    );
  });

  testWidgets('highlights links in message text', (tester) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: LinkMessageGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final text = tester.widget<SelectableText>(
      find.byKey(const ValueKey('message-text-message-link')),
    );
    final span = text.textSpan!;
    expect(span.toPlainText(), 'Read https://example.com/docs today');
    expect(
      span.children!.whereType<TextSpan>().any(
        (child) =>
            child.text == 'https://example.com/docs' &&
            child.recognizer != null,
      ),
      isTrue,
    );
    expect(find.byIcon(Icons.open_in_new_rounded), findsOneWidget);
  });

  testWidgets('link previews can be disabled', (tester) async {
    SharedPreferences.setMockInitialValues({'messages.linkPreviews': false});
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: LinkMessageGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.open_in_new_rounded), findsNothing);
    expect(
      find.byKey(const ValueKey('message-text-message-link')),
      findsOneWidget,
    );
  });

  testWidgets('shows a spinner in the navigation while chats are loading', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = DelayedChatsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pump();
    await tester.tap(find.byIcon(Icons.menu));
    await tester.pump();

    expect(find.byType(CircularProgressIndicator), findsNWidgets(2));
    expect(find.text('No chats available.'), findsNothing);

    gateway.complete();
    await tester.pumpAndSettle();
  });

  testWidgets('sign out clears account-scoped workspace data', (tester) async {
    SharedPreferences.setMockInitialValues({
      'workspace.chats': '{"stale":true}',
      'workspace.allChatsCached': true,
      'navigation.favoriteConversations': ['chat-old'],
      'notifications.mutedConversations': ['chat-old'],
      'messages.syncedConversations': ['chat-old'],
      'messages.linkPreviews': false,
    });
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sign out'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pumpAndSettle();

    expect(find.text('Start Microsoft sign-in'), findsOneWidget);
    final prefs = await SharedPreferences.getInstance();
    expect(prefs.getString('workspace.chats'), isNull);
    expect(prefs.getBool('workspace.allChatsCached'), isNull);
    expect(prefs.getStringList('navigation.favoriteConversations'), isNull);
    expect(prefs.getStringList('notifications.mutedConversations'), isNull);
    expect(prefs.getStringList('messages.syncedConversations'), isNull);
    expect(prefs.getBool('messages.linkPreviews'), isFalse);
  });

  testWidgets('renders initials for an emoji-leading chat name', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: EmojiLeadingNameGateway(),
      ),
    );
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
  });

  testWidgets('retries a profile photo after a transient miss on selection', (
    tester,
  ) async {
    final gateway = FlakyProfilePhotoGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.profilePhotoRequests, 2);

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(gateway.profilePhotoRequests, greaterThan(2));
    final sidebarAvatars = tester.widgetList<CircleAvatar>(
      find.descendant(
        of: find.byKey(
          const ValueKey('conversation-tile-chat-chat-ada-flaky-photo'),
        ),
        matching: find.byType(CircleAvatar),
      ),
    );
    expect(
      sidebarAvatars.any(
        (avatar) =>
            avatar.backgroundImage is MemoryImage &&
            identical(
              (avatar.backgroundImage! as MemoryImage).bytes,
              gateway.profilePhoto,
            ),
      ),
      isTrue,
    );
  });

  testWidgets('reuses the cached leading avatar image when switching chats', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = AvatarChatsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();

    final avatarImages = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .map((avatar) => avatar.backgroundImage)
        .whereType<MemoryImage>()
        .toList();
    expect(
      avatarImages
          .where((image) => identical(image.bytes, gateway.gracePhoto))
          .length,
      2,
    );
  });

  testWidgets('retries a team photo after a transient miss on selection', (
    tester,
  ) async {
    final gateway = FlakyTeamPhotoGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();

    expect(gateway.teamPhotoRequests, 1);

    await tester.tap(find.text('General'));
    await tester.pumpAndSettle();

    expect(gateway.teamPhotoRequests, 3);
    final sidebarAvatar = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .singleWhere((avatar) => avatar.radius == 14);
    expect(sidebarAvatar.backgroundImage, isA<MemoryImage>());
    expect(
      identical(
        (sidebarAvatar.backgroundImage! as MemoryImage).bytes,
        gateway.teamPhoto,
      ),
      isTrue,
    );
  });

  testWidgets('renders teammate presence without querying current user', (
    tester,
  ) async {
    final gateway = PresenceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedUserIds, ['ada-user']);
    expect(find.byTooltip('Busy · In a call'), findsOneWidget);
    expect(find.byTooltip('Available'), findsNothing);
  });

  testWidgets('refreshes presence when a new direct chat is discovered', (
    tester,
  ) async {
    final gateway = DynamicPresenceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedUserIds, [
      ['ada-user'],
    ]);
    gateway.revealGrace = true;
    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-grace-presence',
        resourceType: 'NewMessage',
      ),
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedUserIds.last, contains('grace-user'));
    expect(find.byTooltip('Do not disturb · Presenting'), findsOneWidget);
  });

  testWidgets('retries presence refresh while the same users are loading', (
    tester,
  ) async {
    final gateway = DynamicPresenceGateway()..delayFirstPresence = true;
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    for (
      var attempt = 0;
      attempt < 10 && !gateway.presenceStarted.isCompleted;
      attempt++
    ) {
      await tester.pump();
    }
    expect(gateway.presenceStarted.isCompleted, isTrue);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    await tester.pump();
    gateway.releaseFirstPresence.complete();
    await tester.pumpAndSettle();

    expect(gateway.requestedUserIds.length, greaterThanOrEqualTo(2));
    expect(gateway.requestedUserIds.last, ['ada-user']);
  });

  testWidgets('queues presence refresh while an earlier request is running', (
    tester,
  ) async {
    final gateway = DynamicPresenceGateway()..delayFirstPresence = true;
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    for (
      var attempt = 0;
      attempt < 10 && !gateway.presenceStarted.isCompleted;
      attempt++
    ) {
      await tester.pump();
    }
    expect(gateway.presenceStarted.isCompleted, isTrue);

    gateway.revealGrace = true;
    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-grace-presence',
        resourceType: 'NewMessage',
      ),
    );
    await tester.pump();
    await tester.pump();
    gateway.releaseFirstPresence.complete();
    await tester.pumpAndSettle();

    expect(gateway.requestedUserIds.length, greaterThanOrEqualTo(2));
    expect(gateway.requestedUserIds.last, contains('grace-user'));
  });

  testWidgets('delayed clipboard text cannot move to another chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final readStarted = Completer<void>();
    final releaseRead = Completer<void>();
    MessageClipboard.debugRead = () async {
      if (!readStarted.isCompleted) readStarted.complete();
      await releaseRead.future;
      return const MessageClipboardContent(text: 'Clipboard text');
    };
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: WatchingMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await readStarted.future;

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
    );
    await tester.pumpAndSettle();
    releaseRead.complete();
    await tester.pumpAndSettle();

    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      isEmpty,
    );
  });

  testWidgets('delayed image picker cannot send to another chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    const pickerChannel = MethodChannel(
      'miguelruivo.flutter.plugins.filepicker',
      StandardMethodCodec(),
    );
    final pickerStarted = Completer<void>();
    final releasePicker = Completer<void>();
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      pickerChannel,
      (call) async {
        if (!pickerStarted.isCompleted) pickerStarted.complete();
        await releasePicker.future;
        return [
          {
            'name': 'picked.png',
            'size': 3,
            'bytes': Uint8List.fromList([1, 2, 3]),
          },
        ];
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        pickerChannel,
        null,
      ),
    );
    final gateway = RecordingImageSendGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Upload image'));
    await pickerStarted.future;
    notifications.selectConversation('chat-grace');
    await tester.pumpAndSettle();
    releasePicker.complete();
    await tester.pumpAndSettle();

    expect(gateway.imageConversationIds, isEmpty);
  });

  testWidgets('delayed file picker cannot send to another chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    const pickerChannel = MethodChannel(
      'miguelruivo.flutter.plugins.filepicker',
      StandardMethodCodec(),
    );
    final pickerStarted = Completer<void>();
    final releasePicker = Completer<void>();
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      pickerChannel,
      (call) async {
        if (!pickerStarted.isCompleted) pickerStarted.complete();
        await releasePicker.future;
        return [
          {
            'name': 'picked.txt',
            'size': 3,
            'bytes': Uint8List.fromList([1, 2, 3]),
          },
        ];
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        pickerChannel,
        null,
      ),
    );
    final gateway = RecordingFileSendGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Upload file'));
    await pickerStarted.future;

    notifications.selectConversation('chat-grace');
    await tester.pumpAndSettle();
    releasePicker.complete();
    await tester.pumpAndSettle();

    expect(gateway.fileConversationIds, isEmpty);
  });

  testWidgets('delayed clipboard image cannot send to another chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final readStarted = Completer<void>();
    final releaseRead = Completer<void>();
    MessageClipboard.debugRead = () async {
      if (!readStarted.isCompleted) readStarted.complete();
      await releaseRead.future;
      return MessageClipboardContent(
        imageBytes: Uint8List.fromList([1, 2, 3]),
        imageContentType: 'image/png',
      );
    };
    final gateway = RecordingImageSendGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await readStarted.future;

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
    );
    await tester.pumpAndSettle();
    releaseRead.complete();
    await tester.pumpAndSettle();

    expect(gateway.imageConversationIds, isEmpty);
  });

  testWidgets(
    'shared text stays with its target while an image upload is pending',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = DelayedSharedImageGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();

      final payload = const StandardMethodCodec().encodeMethodCall(
        MethodCall('sharedContent', {
          'text': 'Shared caption text',
          'data': <int>[1, 2, 3],
          'contentType': 'image/png',
        }),
      );
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'microslop/share_target',
        payload,
        (_) {},
      );
      await tester.pumpAndSettle();
      expect(find.text('Share to'), findsOneWidget);

      await tester.tap(find.text('Chat with Ada').last);
      await tester.pump();
      for (
        var attempt = 0;
        attempt < 10 && !gateway.imageSendStarted.isCompleted;
        attempt++
      ) {
        await tester.pump();
      }
      expect(gateway.imageSendStarted.isCompleted, isTrue);

      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
      );
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(find.byType(TextField).last).controller!.text,
        isEmpty,
      );

      gateway.releaseImageSend.complete();
      await tester.pumpAndSettle();

      expect(
        tester.widget<TextField>(find.byType(TextField).last).controller!.text,
        isEmpty,
      );
      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
      );
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(find.byType(TextField).last).controller!.text,
        'Shared caption text',
      );
    },
  );

  testWidgets('shared image preserves an existing target draft', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = WatchingMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'Existing draft');

    final payload = const StandardMethodCodec().encodeMethodCall(
      MethodCall('sharedContent', {
        'data': <int>[1, 2, 3],
        'contentType': 'image/png',
      }),
    );
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'microslop/share_target',
      payload,
      (_) {},
    );
    await tester.pumpAndSettle();
    expect(find.text('Share to'), findsOneWidget);
    await tester.tap(find.text('Chat with Ada').last);
    await tester.pumpAndSettle();

    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      'Existing draft',
    );
  });

  testWidgets('shared text preserves an existing target draft', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = WatchingMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'Existing draft');

    final payload = const StandardMethodCodec().encodeMethodCall(
      MethodCall('sharedContent', {'text': 'Shared text'}),
    );
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'microslop/share_target',
      payload,
      (_) {},
    );
    await tester.pumpAndSettle();
    expect(find.text('Share to'), findsOneWidget);
    await tester.tap(find.text('Chat with Ada').last);
    await tester.pumpAndSettle();

    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      'Existing draft\nShared text',
    );
  });

  testWidgets('shows a message-loading error without poisoning the workspace', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FailingMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.text('Unable to load messages'), findsOneWidget);
    expect(find.textContaining('Unable to load messages'), findsWidgets);
    expect(find.text('Unable to load Teams'), findsNothing);
    expect(find.text('Chat with Ada'), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  testWidgets('restored chat rejection does not replace the workspace', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({
      'workspace.chats': jsonEncode(
        const CachedWorkspace(
          chats: [
            CachedConversation(
              id: 'chat-1',
              name: 'Chat with Ada',
              isGroup: false,
            ),
          ],
          teams: [],
          selectedChatId: 'chat-1',
          messages: {},
        ).toJson(),
      ),
    });

    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FailingRejectedMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Unable to load messages'), findsOneWidget);
    expect(find.textContaining('Teams rejected the request'), findsOneWidget);
    expect(find.text('Unable to load Teams'), findsNothing);
    expect(find.text('Chat with Ada'), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  testWidgets('opens the mobile drawer from the wider swipe area', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.byType(Drawer), findsNothing);
    expect(find.text('Conversation'), findsNothing);

    await tester.dragFrom(const Offset(100, 400), const Offset(250, 0));
    await tester.pumpAndSettle();
    expect(find.byType(Drawer), findsOneWidget);

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.text('Newest message'), findsOneWidget);
  });

  testWidgets('late message camera start is stopped after disposal', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    PlatformCallVideo.debugSupportedOverride = true;
    addTearDown(() => PlatformCallVideo.debugSupportedOverride = null);
    const callVideoChannel = MethodChannel('microslop/call_video');
    final startEntered = Completer<void>();
    final releaseStart = Completer<bool>();
    var stopCalls = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      callVideoChannel,
      (call) async {
        if (call.method == 'startCamera') {
          if (!startEntered.isCompleted) startEntered.complete();
          return releaseStart.future;
        }
        if (call.method == 'stopCamera') stopCalls++;
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        callVideoChannel,
        null,
      ),
    );
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open camera'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(startEntered.isCompleted, isTrue);

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(stopCalls, 1);

    releaseStart.complete(true);
    await tester.pump();

    expect(stopCalls, 2);
    PlatformCallVideo.debugSupportedOverride = null;
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('recorded audio cannot send to a newly selected chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    const messageMediaChannel = MethodChannel('microslop/message_media');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      messageMediaChannel,
      (call) async => switch (call.method) {
        'stopAudioRecording' => {
          'data': Uint8List.fromList([1, 2, 3]),
          'name': 'voice.m4a',
          'contentType': 'audio/mp4',
        },
        _ => null,
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        messageMediaChannel,
        null,
      ),
    );
    final gateway = RecordingFileSendGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Record audio'));
    await tester.pumpAndSettle();
    expect(find.textContaining('Recording ·'), findsOneWidget);

    notifications.selectConversation('chat-grace');
    await tester.pump();
    await tester.tap(find.text('Stop & send'));
    await tester.pumpAndSettle();

    expect(gateway.fileConversationIds, isEmpty);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('camera result cannot send to a newly selected chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    PlatformCallVideo.debugSupportedOverride = true;
    addTearDown(() => PlatformCallVideo.debugSupportedOverride = null);
    const callVideoChannel = MethodChannel('microslop/call_video');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      callVideoChannel,
      (call) async => call.method == 'startCamera' ? false : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        callVideoChannel,
        null,
      ),
    );
    final gateway = RecordingImageSendGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open camera'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Camera'), findsOneWidget);

    notifications.selectConversation('chat-grace');
    await tester.pump();
    Navigator.of(
      tester.element(find.text('Camera')),
    ).pop(Uint8List.fromList([1, 2, 3]));
    await tester.pumpAndSettle();

    expect(gateway.imageConversationIds, isEmpty);
    PlatformCallVideo.debugSupportedOverride = null;
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('late audio recording start is cancelled after disposal', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    const messageMediaChannel = MethodChannel('microslop/message_media');
    final startEntered = Completer<void>();
    final releaseStart = Completer<void>();
    var cancelCalls = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      messageMediaChannel,
      (call) async {
        if (call.method == 'startAudioRecording') {
          if (!startEntered.isCompleted) startEntered.complete();
          await releaseStart.future;
        } else if (call.method == 'cancelAudioRecording') {
          cancelCalls++;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        messageMediaChannel,
        null,
      ),
    );
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Record audio'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(startEntered.isCompleted, isTrue);

    await tester.pumpWidget(const SizedBox.shrink());
    releaseStart.complete();
    await tester.pump();

    expect(cancelCalls, 1);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('mobile composer opens attachment actions from a small plus', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.byTooltip('Add attachment'), findsOneWidget);
    await tester.tap(find.byTooltip('Add attachment'));
    await tester.pumpAndSettle();

    expect(find.text('Upload file'), findsOneWidget);
    expect(find.text('Image'), findsOneWidget);
    expect(find.text('Open camera'), findsOneWidget);
    expect(find.text('Record audio'), findsOneWidget);
  });

  testWidgets('groups consecutive incoming messages under one sender header', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: GroupedIncomingMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.text('Ada Lovelace'), findsOneWidget);
    expect(selectableMessage('First grouped message'), findsOneWidget);
    expect(selectableMessage('Second grouped message'), findsOneWidget);
  });

  testWidgets('shows sender avatars only for messages from other people', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final incomingAvatarCount = find.byType(CircleAvatar).evaluate().length;

    SharedPreferences.setMockInitialValues({});
    await tester.pumpWidget(
      OstApp(
        key: const ValueKey('current-user-message'),
        gateway: RestoringAuthGateway(),
        teamsGateway: OwnMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final outgoingAvatarCount = find.byType(CircleAvatar).evaluate().length;
    expect(incomingAvatarCount, outgoingAvatarCount + 1);
  });

  testWidgets('renders quoted replies separately from the reply body', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: QuotedMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(
      find.byKey(const ValueKey('message-quote-quoted-1-0')),
      findsOneWidget,
    );
    expect(find.text('Grace Hopper'), findsOneWidget);
    expect(find.text('Original quoted message'), findsOneWidget);
    expect(selectableMessage('Reply body only'), findsOneWidget);
  });

  testWidgets('generic direct chat name ignores formatted current-user id', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: GenericNameOwnIdGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat'));
    await tester.pumpAndSettle();

    final navigationTile = find.byKey(
      const ValueKey('conversation-tile-chat-generic-chat'),
    );
    expect(
      find.descendant(of: navigationTile, matching: find.text('Grace Hopper')),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: navigationTile,
        matching: find.text('Different display name'),
      ),
      findsNothing,
    );
  });

  testWidgets('renders fenced code blocks with monospace content', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: CodeBlockGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final block = find.byKey(const ValueKey('message-code-code-1-1'));
    expect(block, findsOneWidget);
    expect(find.text('Before'), findsOneWidget);
    expect(find.text('dart'), findsOneWidget);
    expect(find.text('After'), findsOneWidget);
    final code = tester.widget<SelectableText>(
      find.descendant(
        of: block,
        matching: find.byWidgetPredicate(
          (widget) =>
              widget is SelectableText &&
              widget.data == 'final answer = 42;\nhttps://example.com/code',
        ),
      ),
    );
    expect(code.style?.fontFamily, 'monospace');
    expect(find.byIcon(Icons.open_in_new_rounded), findsNothing);
  });

  testWidgets('right aligns messages sent by the current user', (tester) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: OwnMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final message = tester.widget<SelectableText>(
      find.byWidgetPredicate(
        (widget) => widget is SelectableText && widget.data == 'My message',
      ),
    );

    expect(message.textAlign, TextAlign.end);
  });

  testWidgets(
    'uses the sender identity when an outgoing message has another display name',
    (tester) async {
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: MismatchedOutgoingMessageGateway(),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();

      final message = tester.widget<SelectableText>(
        find.byWidgetPredicate(
          (widget) =>
              widget is SelectableText && widget.data == 'Message from me',
        ),
      );

      expect(message.textAlign, TextAlign.end);
      expect(find.byType(CircleAvatar), findsNWidgets(3));
    },
  );

  testWidgets('keeps incoming and outgoing messages close on wide screens', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1600, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: MixedMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final incoming = tester.getCenter(selectableMessage('Incoming message'));
    final outgoing = tester.getCenter(selectableMessage('Outgoing message'));

    expect((incoming.dx - outgoing.dx).abs(), lessThanOrEqualTo(760));
  });

  testWidgets('live refresh preserves the visible history anchor', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = GrowingHistoryGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    final latest = find.byKey(const ValueKey('message-bubble-history-59'));
    final messageList = find.ancestor(
      of: latest,
      matching: find.byType(ListView),
    );
    await tester.drag(messageList, const Offset(0, 700));
    await tester.pumpAndSettle();

    Map<String, double> visibleBubbleTops() {
      final screen = tester.getRect(find.byType(Scaffold).first);
      final bubbles = find.byWidgetPredicate(
        (widget) =>
            widget.key is ValueKey<String> &&
            (widget.key! as ValueKey<String>).value.startsWith(
              'message-bubble-history-',
            ),
      );
      return {
        for (final element in bubbles.evaluate())
          if (tester.getRect(find.byWidget(element.widget)).bottom >
                  screen.top &&
              tester.getRect(find.byWidget(element.widget)).top < screen.bottom)
            (element.widget.key! as ValueKey<String>).value: tester
                .getTopLeft(find.byWidget(element.widget))
                .dy,
      };
    }

    final before = visibleBubbleTops();
    expect(before.length, greaterThan(2));
    gateway.newerMessage = true;
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();
    final after = visibleBubbleTops();

    var compared = 0;
    for (final entry in before.entries) {
      final next = after[entry.key];
      if (next == null) continue;
      expect((next - entry.value).abs(), lessThanOrEqualTo(0.5));
      compared++;
    }
    expect(compared, greaterThan(2));

    final beforeReaction = visibleBubbleTops();
    gateway.newestReaction = true;
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();
    final afterReaction = visibleBubbleTops();
    compared = 0;
    for (final entry in beforeReaction.entries) {
      final next = afterReaction[entry.key];
      if (next == null) continue;
      expect((next - entry.value).abs(), lessThanOrEqualTo(0.5));
      compared++;
    }
    expect(compared, greaterThan(2));
  });

  testWidgets('cached image resolution preserves the visible history anchor', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = GrowingHistoryGateway()..cachedImageSlot = true;
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    final latest = find.byKey(const ValueKey('message-bubble-history-59'));
    final messageList = find.ancestor(
      of: latest,
      matching: find.byType(ListView),
    );
    await tester.drag(messageList, const Offset(0, 700));
    await tester.pumpAndSettle();

    Map<String, double> visibleBubbleTops() {
      final screen = tester.getRect(find.byType(Scaffold).first);
      final bubbles = find.byWidgetPredicate(
        (widget) =>
            widget.key is ValueKey<String> &&
            (widget.key! as ValueKey<String>).value.startsWith(
              'message-bubble-history-',
            ),
      );
      return {
        for (final element in bubbles.evaluate())
          if (tester.getRect(find.byWidget(element.widget)).bottom >
                  screen.top &&
              tester.getRect(find.byWidget(element.widget)).top < screen.bottom)
            (element.widget.key! as ValueKey<String>).value: tester
                .getTopLeft(find.byWidget(element.widget))
                .dy,
      };
    }

    final before = visibleBubbleTops();
    expect(before.length, greaterThan(2));
    gateway.cachedImageLoaded = true;
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();
    final after = visibleBubbleTops();

    var compared = 0;
    for (final entry in before.entries) {
      final next = after[entry.key];
      if (next == null) continue;
      expect((next - entry.value).abs(), lessThanOrEqualTo(0.5));
      compared++;
    }
    expect(compared, greaterThan(2));
  });

  testWidgets('updates the open conversation without a manual refresh', (
    tester,
  ) async {
    final gateway = WatchingMessagesGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(selectableMessage('First message'), findsOneWidget);

    gateway.showLatestMessage = true;
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();

    expect(selectableMessage('Latest message'), findsOneWidget);
    expect(gateway.readCount, 2);
    expect(notifications.titles, isEmpty);
  });

  testWidgets(
    'reconciles the open conversation for an empty message event id',
    (tester) async {
      final gateway = WatchingMessagesGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();
      expect(selectableMessage('First message'), findsOneWidget);

      gateway.showLatestMessage = true;
      gateway.events.add(const MessageEvent(conversationId: ''));
      await tester.pumpAndSettle();

      expect(selectableMessage('Latest message'), findsOneWidget);
      expect(gateway.readCount, 2);
    },
  );

  testWidgets('clears a deleted message from a closed conversation cache', (
    tester,
  ) async {
    final gateway = DeletedMessageActivityGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(selectableMessage('Deleted remotely'), findsOneWidget);

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-grace',
        resourceType: 'MessageUpdate',
      ),
    );
    await tester.pumpAndSettle();
    expect(gateway.graceRefreshes, 2);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pump();
    await gateway.reopenRefreshStarted.future;
    await tester.pump();

    expect(selectableMessage('Deleted remotely'), findsNothing);
  });

  testWidgets('coalesces reconcile events during the initial chat load', (
    tester,
  ) async {
    final gateway = DelayedInitialChatsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pump();

    gateway.events.add(const MessageEvent(conversationId: ''));
    gateway.events.add(const MessageEvent(conversationId: ''));
    await tester.pump();
    expect(gateway.listCount, 1);

    gateway.completeInitialLoad();
    await tester.pumpAndSettle();
    expect(gateway.listCount, 1);
  });

  testWidgets('does not prefetch chat history before a chat is opened', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = WatchingMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.readCount, 0);

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(selectableMessage('First message'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);
    expect(gateway.readCount, 1);
  });

  testWidgets('keeps message history out of shared preferences', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString('workspace.chats');
    expect(raw, isNotNull);
    final workspace = jsonDecode(raw!) as Map<String, dynamic>;
    expect(workspace['messages'], isEmpty);
    expect(raw.length, lessThan(10000));
  });

  testWidgets('reconcile events notify when a closed chat preview changed', (
    tester,
  ) async {
    final gateway = ReconcileNotificationGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.changedPreview = true;
    gateway.events.add(const MessageEvent(conversationId: ''));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Grace']);
  });

  testWidgets('reconcile detects a newly discovered chat', (tester) async {
    final gateway = NewChatReconcileGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.revealGrace = true;
    gateway.events.add(const MessageEvent(conversationId: ''));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with New Grace']);
    expect(notifications.bodies, ['Hello from the new chat']);
  });

  testWidgets(
    'reconcile detects the first message when the old chat had no message id',
    (tester) async {
      final gateway = FirstMessageReconcileGateway();
      final notifications = RecordingDesktopNotifications();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: gateway,
          notifications: notifications,
        ),
      );
      await tester.pumpAndSettle();

      gateway.hasFirstMessage = true;
      gateway.events.add(const MessageEvent(conversationId: ''));
      await tester.pumpAndSettle();

      expect(notifications.titles, ['Chat with Grace']);
      expect(notifications.bodies, ['First message in this chat']);
    },
  );

  testWidgets(
    'reconcile detects a new message even when preview text repeats',
    (tester) async {
      final gateway = ReconcileNotificationGateway();
      final notifications = RecordingDesktopNotifications();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: gateway,
          notifications: notifications,
        ),
      );
      await tester.pumpAndSettle();

      gateway.samePreviewNewMessage = true;
      gateway.events.add(const MessageEvent(conversationId: ''));
      await tester.pumpAndSettle();

      expect(notifications.titles, ['Chat with Grace']);
    },
  );

  testWidgets(
    'notification selection opens its conversation and closes drawer',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(400, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final notifications = RecordingDesktopNotifications();
      final gateway = WatchingMessagesGateway();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: gateway,
          notifications: notifications,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Open navigation'));
      await tester.pumpAndSettle();
      expect(
        tester.state<ScaffoldState>(find.byType(Scaffold).first).isDrawerOpen,
        isTrue,
      );

      notifications.selectConversation('chat-grace');
      await tester.pumpAndSettle();

      expect(gateway.readCounts['chat-grace'], 1);
      expect(notifications.dismissedConversationIds, contains('chat-grace'));
      expect(
        tester.state<ScaffoldState>(find.byType(Scaffold).first).isDrawerOpen,
        isFalse,
      );
    },
  );

  testWidgets('notifies for the selected DM when the app is backgrounded', (
    tester,
  ) async {
    final gateway = WatchingMessagesGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();
    expect(notifications.titles, isEmpty);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Ada']);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
  });

  testWidgets('background notification fallback checks within 15 seconds', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    final gateway = ReconcileNotificationGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    gateway.samePreviewNewMessage = true;
    await tester.pump(const Duration(seconds: 14));
    expect(notifications.titles, isEmpty);
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Grace']);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('reconcile notifies for the selected DM while backgrounded', (
    tester,
  ) async {
    final gateway = ReconcileNotificationGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    gateway.changedSelectedPreview = true;
    gateway.events.add(const MessageEvent(conversationId: ''));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Ada']);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
  });

  testWidgets('marks background activity unread until the chat is opened', (
    tester,
  ) async {
    AppLog.entries.value = [];
    addTearDown(() => AppLog.entries.value = []);
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = WatchingMessagesGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.circle), findsOneWidget);
    expect(notifications.titles, ['Chat with Grace']);
    expect(AppLog.entries.value, isEmpty);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.circle), findsNothing);
  });

  testWidgets('does not notify for own chat activity', (tester) async {
    final gateway = OwnActivityWatchingGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });

  testWidgets('muted conversations do not notify', (tester) async {
    SharedPreferences.setMockInitialValues({
      'notifications.mutedConversations': ['chat-grace'],
    });
    final gateway = WatchingMessagesGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });
}
