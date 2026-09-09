part of '../widget_test.dart';

void registerAvatarChannelTests() {
  testWidgets(
    'gallery discovers history before downloading and dismisses outside the image',
    (tester) async {
      final gateway = LazyGalleryGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
      );
      await tester.pumpAndSettle();
      final clicked = find.byWidgetPredicate(
        (widget) =>
            widget is Image &&
            widget.image is MemoryImage &&
            identical(
              (widget.image as MemoryImage).bytes,
              gateway.images.first.bytes,
            ),
      );
      await tester.runAsync(
        () => precacheImage(
          MemoryImage(gateway.images.first.bytes),
          tester.element(clicked),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(clicked);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('1 / 1+'), findsOneWidget);
      gateway.history.complete([
        ...await gateway.readMessages(
          const Conversation.chat(
            id: 'self-chat',
            name: 'Self',
            isGroup: false,
          ),
        ),
        for (var index = 1; index < 3; index++)
          MessageSummary(
            id: 'image-$index',
            sender: 'brandonp2412',
            timestamp: '2026-09-05T10:0$index:00Z',
            content: '',
            imageUrls: ['image-$index'],
          ),
      ]);
      await tester.pumpAndSettle();
      expect(find.text('1 / 3'), findsOneWidget);
      expect(gateway.requests, isEmpty);
      await tester.tap(find.byTooltip('Next image'));
      await tester.pump();
      await tester.pump();
      expect(find.text('2 / 3'), findsOneWidget);
      expect(gateway.requests, ['image-1']);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);
      expect(
        tester
            .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.download))
            .onPressed,
        isNull,
      );
      gateway.download.complete(gateway.images[1]);
      await tester.pumpAndSettle();
      await tester.runAsync(
        () => precacheImage(
          MemoryImage(gateway.images[1].bytes),
          tester.element(find.byType(InteractiveViewer)),
        ),
      );
      await tester.pumpAndSettle();
      final image = find.descendant(
        of: find.byType(InteractiveViewer),
        matching: find.byType(Image),
      );
      await tester.tap(image);
      await tester.pumpAndSettle();
      expect(find.text('2 / 3'), findsOneWidget);
      final page = tester.getRect(find.byType(PageView));
      await tester.tapAt(page.topLeft + const Offset(12, 12));
      await tester.pumpAndSettle();
      expect(find.byType(PageView), findsNothing);
      expect(gateway.requests, ['image-1']);
    },
  );

  testWidgets('pending attachment download keeps the message height stable', (
    tester,
  ) async {
    final gateway = LoadingImageGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('Attachment unavailable'), findsNothing);
    expect(find.text('Loading attachment…'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    final bubble = find.byKey(const ValueKey('message-bubble-loading-image'));
    final loadingSize = tester.getSize(bubble);
    gateway.download.complete(gateway.images.first);
    await tester.pumpAndSettle();
    expect(find.text('Loading attachment…'), findsNothing);
    expect(tester.getSize(bubble), loadingSize);
  });

  testWidgets('cached attachment count reserves the migrated message height', (
    tester,
  ) async {
    final gateway = CachedImageCountGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));

    final bubble = find.byKey(const ValueKey('message-bubble-cached-image'));
    final before = tester.getSize(bubble);
    gateway.imageLoaded = true;
    gateway.events.add(const MessageEvent(conversationId: 'self-chat'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));

    expect(
      find.descendant(of: bubble, matching: find.byType(Image)),
      findsOneWidget,
    );
    expect(tester.getSize(bubble), before);
  });

  testWidgets(
    'desktop user hover shows profile and Teams status details',
    (tester) async {
      final gateway = UserHoverGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(tester.getCenter(find.text('Ada Lovelace').last));
      await tester.pump();
      await tester.pumpAndSettle();
      await tester.pump(const Duration(milliseconds: 700));
      await tester.pumpAndSettle();

      expect(gateway.requestedUserIds, ['ada-user']);
      expect(
        find.byTooltip(
          'Ada Lovelace\nPrincipal Engineer\nHeads down\nBusy · In a call\nada@example.com',
        ),
        findsWidgets,
      );
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets(
    'desktop reaction hover resolves and lists the people',
    (tester) async {
      final gateway = ReactionPeopleGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
      );
      await tester.pumpAndSettle();
      expect(gateway.requests, isEmpty);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(
        tester.getCenter(
          find.byKey(const ValueKey('reaction-like-unselected')),
        ),
      );
      await tester.pump();
      expect(gateway.requests, ['grace']);
      gateway.name.complete('Grace Hopper');
      await tester.pumpAndSettle();
      await tester.pump(const Duration(seconds: 1));
      expect(find.text('Ada Lovelace\nGrace Hopper'), findsOneWidget);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets(
    'desktop chat options use immediate menus',
    (tester) async {
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: GalleryMessagesGateway(),
        ),
      );
      await tester.pumpAndSettle();
      final tile = find.byKey(
        const ValueKey('conversation-tile-chat-self-chat'),
      );
      await tester.longPress(tile);
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet), findsNothing);
      await tester.tap(tile, buttons: kSecondaryMouseButton);
      await tester.pump();
      expect(find.text('Chat options'), findsOneWidget);
      expect(
        ModalRoute.of(
          tester.element(find.text('Chat options')),
        )!.transitionDuration,
        Duration.zero,
      );
      await tester.tap(find.text('Chat options'));
      await tester.pumpAndSettle();
      expect(find.text('Add to favourites'), findsOneWidget);
      expect(find.byType(BottomSheet), findsNothing);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets(
    'missing image messages show an attachment state and can be retried',
    (tester) async {
      final gateway = MissingImageGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
      );
      await tester.pumpAndSettle();
      expect(find.text('Attachment unavailable'), findsOneWidget);
      gateway.available = true;
      await tester.tap(find.byTooltip('Retry attachment'));
      await tester.pumpAndSettle();
      expect(find.text('Attachment unavailable'), findsNothing);
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is Image &&
              widget.image is MemoryImage &&
              identical(
                (widget.image as MemoryImage).bytes,
                gateway.images.first.bytes,
              ),
        ),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'image gallery opens clicked image first and loads chat history',
    (tester) async {
      final gateway = GalleryMessagesGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
      );
      await tester.pumpAndSettle();
      final clicked = find.byWidgetPredicate(
        (widget) =>
            widget is Image &&
            widget.image is MemoryImage &&
            identical(
              (widget.image as MemoryImage).bytes,
              gateway.images[1].bytes,
            ),
      );
      await tester.runAsync(
        () => precacheImage(
          MemoryImage(gateway.images[1].bytes),
          tester.element(find.byType(Scaffold).first),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(clicked);
      await tester.pumpAndSettle();
      expect(find.text('3 / 3'), findsOneWidget);
      final visibleImage = tester.widget<Image>(
        find.descendant(
          of: find.byType(InteractiveViewer).first,
          matching: find.byType(Image),
        ),
      );
      expect(
        (visibleImage.image as MemoryImage).bytes,
        gateway.images[1].bytes,
      );
      await tester.tap(find.byTooltip('Previous image'));
      await tester.pumpAndSettle();
      expect(find.text('2 / 3'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byType(InteractiveViewer), findsNothing);
    },
  );

  testWidgets('right clicking a chat image offers gallery copy and save', (
    tester,
  ) async {
    final gateway = GalleryMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    final chat = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.tap(chat, buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Chat options'), findsOneWidget);
    await tester.tap(find.text('Open chat'));
    await tester.pumpAndSettle();
    final picture = find.byWidgetPredicate(
      (widget) =>
          widget is Image &&
          widget.image is MemoryImage &&
          identical(
            (widget.image as MemoryImage).bytes,
            gateway.images[1].bytes,
          ),
    );
    await tester.runAsync(
      () => precacheImage(
        MemoryImage(gateway.images[1].bytes),
        tester.element(find.byType(Scaffold).first),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(picture, buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Open gallery'), findsOneWidget);
    expect(find.text('Copy image'), findsOneWidget);
    expect(find.text('Save image'), findsOneWidget);
  });
  testWidgets('renders message image bytes in chat bubbles', (tester) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: ImageMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final images = tester.widgetList<Image>(find.byType(Image)).toList();
    expect(images, hasLength(1));
    expect((images.single.image as MemoryImage).bytes, same(testPng));
  });

  testWidgets('renders initials for group chats without a chat photo', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: GroupAvatarGateway(),
      ),
    );
    await tester.pumpAndSettle();

    final images = tester.widgetList<Image>(find.byType(Image)).toList();
    expect(images, hasLength(0));
    expect(find.text('PG'), findsOneWidget);
  });

  testWidgets('retries a group photo after a transient miss on selection', (
    tester,
  ) async {
    final gateway = FlakyGroupPhotoGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.chatPhotoRequests, 1);

    await tester.tap(find.text('Project group'));
    await tester.pumpAndSettle();

    expect(gateway.chatPhotoRequests, 3);
    final sidebarAvatar = tester.widget<CircleAvatar>(
      find.descendant(
        of: find.byKey(const ValueKey('conversation-tile-chat-group-1')),
        matching: find.byType(CircleAvatar),
      ),
    );
    expect(sidebarAvatar.backgroundImage, isA<MemoryImage>());
    expect(
      identical(
        (sidebarAvatar.backgroundImage! as MemoryImage).bytes,
        gateway.chatPhoto,
      ),
      isTrue,
    );
  });

  testWidgets('renders meeting chats with a calendar avatar', (tester) async {
    final gateway = MeetingAvatarGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('Meeting'), findsOneWidget);
    expect(gateway.chatPhotoRequests, 0);
    expect(find.text('AZ'), findsNothing);
  });

  testWidgets('does not restart group avatar requests on sidebar rebuilds', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = CountingGroupAvatarGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    final chatRequests = gateway.chatPhotoRequests;
    final profileRequests = gateway.profilePhotoRequests;

    await tester.binding.setSurfaceSize(const Size(1180, 800));
    await tester.pumpAndSettle();

    expect(gateway.chatPhotoRequests, chatRequests + 1);
    expect(gateway.profilePhotoRequests, profileRequests);

    await tester.binding.setSurfaceSize(const Size(1160, 800));
    await tester.pumpAndSettle();

    expect(gateway.chatPhotoRequests, chatRequests + 1);
    expect(gateway.profilePhotoRequests, profileRequests);
  });

  testWidgets('renders the chat photo for a named group chat', (tester) async {
    final gateway = NamedGroupAvatarGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    final avatarImages = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .map((avatar) => avatar.backgroundImage)
        .whereType<MemoryImage>();
    expect(
      avatarImages,
      contains(
        predicate<MemoryImage>(
          (image) => identical(image.bytes, gateway.chatPhoto),
        ),
      ),
    );
    expect(gateway.requestedChatId, 'bingo-bus');
  });

  testWidgets('renders a group chat photo instead of a profile photo', (
    tester,
  ) async {
    final gateway = GroupWithProfileAvatarGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    final avatarImages = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .map((avatar) => avatar.backgroundImage)
        .whereType<MemoryImage>()
        .toList();
    expect(
      avatarImages,
      contains(
        predicate<MemoryImage>(
          (image) => identical(image.bytes, gateway.chatPhoto),
        ),
      ),
    );
    expect(gateway.requestedChatId, 'consumer-stc-chat');
    expect(gateway.requestedProfileUserIds, isNot(contains('consumer-stc')));
  });

  testWidgets('renders team and channel-message sender photos', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = AvatarChannelsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();
    var avatarImages = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .map((avatar) => avatar.backgroundImage)
        .whereType<MemoryImage>()
        .toList();
    expect(
      avatarImages.any((image) => identical(image.bytes, gateway.teamPhoto)),
      isTrue,
    );

    await tester.tap(find.text('General'));
    await tester.pumpAndSettle();
    avatarImages = tester
        .widgetList<CircleAvatar>(find.byType(CircleAvatar))
        .map((avatar) => avatar.backgroundImage)
        .whereType<MemoryImage>()
        .toList();
    expect(
      avatarImages.any((image) => identical(image.bytes, gateway.senderPhoto)),
      isTrue,
    );
  });

  testWidgets('shows team channels without cluttering the chat list', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: ChannelsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.text('Channels'), findsOneWidget);
    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();

    expect(find.text('Engineering'), findsOneWidget);
    expect(find.text('General'), findsOneWidget);
    await tester.tap(find.text('General'));
    await tester.pumpAndSettle();
    expect(selectableMessage('Channel message'), findsOneWidget);
  });

  testWidgets('expanded channels fit in a short desktop sidebar', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(800, 480));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: ChannelsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();

    expect(find.text('General'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
