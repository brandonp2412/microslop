part of '../widget_test.dart';

void registerNotificationReactionTests() {
  testWidgets('favourites and mute are available from conversation actions', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: WatchingMessagesGateway(),
      ),
    );
    await tester.pumpAndSettle();

    await tester.longPress(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(find.text('Add to favourites'), findsOneWidget);
    expect(find.text('Mute notifications'), findsOneWidget);
    await tester.tap(find.text('Add to favourites'));
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.star_rounded), findsOneWidget);
    await tester.longPress(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(find.text('Remove from favourites'), findsOneWidget);
  });

  testWidgets('favourited channels move ahead of other teams', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FavoriteChannelsGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();

    expect(
      tester.getTopLeft(find.text('General')).dy,
      lessThan(tester.getTopLeft(find.text('Deployments')).dy),
    );
    await tester.longPress(find.text('Deployments'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Add to favourites'));
    await tester.pumpAndSettle();

    expect(
      tester.getTopLeft(find.text('Deployments')).dy,
      lessThan(tester.getTopLeft(find.text('General')).dy),
    );
  });

  testWidgets('inactive saved accounts contribute notifications', (
    tester,
  ) async {
    final gateway = MultiAccountWatchingGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(
      const MessageEvent(
        conversationId: 'other-chat',
        accountId: 'account-secondary',
        accountActive: false,
        resourceType: 'NewMessage',
      ),
    );
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Other Account · Chat with Grace']);
    expect(notifications.bodies, ['Message from the other account']);
    expect(notifications.conversationIds, [isNull]);
    expect(gateway.requestedLimits, [1]);
  });

  testWidgets('retries a saved-account notification after delivery fails', (
    tester,
  ) async {
    final gateway = MultiAccountWatchingGateway();
    final notifications = FailingOnceDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    const event = MessageEvent(
      conversationId: 'other-chat',
      accountId: 'account-secondary',
      accountActive: false,
      resourceType: 'NewMessage',
    );
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.showAttempts, 1);
    expect(notifications.titles, isEmpty);

    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.showAttempts, 2);
    expect(notifications.titles, ['Other Account · Chat with Grace']);
    expect(notifications.bodies, ['Message from the other account']);
  });

  testWidgets('retries a message notification after delivery fails', (
    tester,
  ) async {
    final gateway = NotificationRetryGateway();
    final notifications = FailingOnceConversationNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    gateway.latest = true;
    const event = MessageEvent(conversationId: 'chat-grace');
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.showAttempts, 1);
    expect(notifications.titles, isEmpty);

    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.showAttempts, 2);
    expect(notifications.titles, ['Chat with Grace']);
    expect(notifications.bodies, ['Retry this notification']);
  });

  testWidgets('opening a chat cancels a failed pending notification', (
    tester,
  ) async {
    final gateway = NotificationRetryGateway();
    final notifications = FailingOnceConversationNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    gateway.latest = true;
    const event = MessageEvent(conversationId: 'chat-grace');
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.showAttempts, 1);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.showAttempts, 1);
    expect(notifications.titles, isEmpty);
  });

  testWidgets('retries a saved-account reaction after delivery fails', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.reactions': true});
    final gateway = HistoricalSavedAccountReactionGateway();
    final notifications = FailingOnceDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    const event = MessageEvent(
      conversationId: 'other-chat',
      accountId: 'account-secondary',
      accountActive: false,
      resourceType: 'MessageUpdate',
    );
    gateway.events.add(event);
    await tester.pumpAndSettle();
    gateway.reacted = true;
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.showAttempts, 1);
    expect(notifications.titles, isEmpty);

    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.showAttempts, 2);
    expect(notifications.titles, ['❤️ Other Account · Chat with Grace']);
  });

  testWidgets('saved-account historical reactions use recent message history', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.reactions': true});
    final gateway = HistoricalSavedAccountReactionGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    const event = MessageEvent(
      conversationId: 'other-chat',
      accountId: 'account-secondary',
      accountActive: false,
      resourceType: 'MessageUpdate',
    );
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.titles, isEmpty);

    gateway.reacted = true;
    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.titles, ['❤️ Other Account · Chat with Grace']);
    expect(notifications.bodies, ['']);
    expect(gateway.requestedLimits, [25, 25]);
  });

  testWidgets('saved-account reaction updates require a count increase', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.reactions': true});
    final gateway = SavedAccountReactionWatchingGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(
      const MessageEvent(
        conversationId: 'other-chat',
        accountId: 'account-secondary',
        accountActive: false,
        resourceType: 'MessageUpdate',
      ),
    );
    await tester.pumpAndSettle();
    expect(notifications.titles, isEmpty);

    gateway.reactionCount = 2;
    gateway.events.add(
      const MessageEvent(
        conversationId: 'other-chat',
        accountId: 'account-secondary',
        accountActive: false,
        resourceType: 'MessageUpdate',
      ),
    );
    await tester.pumpAndSettle();

    expect(notifications.titles, ['❤️ Other Account · Chat with Grace']);
    expect(notifications.bodies, ['']);
  });

  testWidgets('stale saved-account activity does not notify after newer data', (
    tester,
  ) async {
    final gateway = OutOfOrderSavedAccountGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    const event = MessageEvent(
      conversationId: 'other-chat',
      accountId: 'account-secondary',
      accountActive: false,
      resourceType: 'NewMessage',
    );
    gateway.events.add(event);
    await tester.pump();
    await gateway.firstReadStarted.future;

    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.bodies, ['Newer saved activity']);

    gateway.releaseFirstRead.complete();
    await tester.pumpAndSettle();

    expect(notifications.bodies, ['Newer saved activity']);
  });

  testWidgets(
    'saved-account fetch cannot notify after notifications are disabled',
    (tester) async {
      final gateway = DelayedSavedAccountGateway();
      final notifications = RecordingDesktopNotifications();
      await tester.pumpWidget(
        OstApp(
          gateway: MultiAccountRestoringAuthGateway(),
          teamsGateway: gateway,
          notifications: notifications,
        ),
      );
      await tester.pumpAndSettle();

      gateway.events.add(
        const MessageEvent(
          conversationId: 'other-chat',
          accountId: 'account-secondary',
          accountActive: false,
          resourceType: 'NewMessage',
        ),
      );
      await tester.pump();
      await gateway.fetchStarted.future;

      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Enable notifications'));
      await tester.pumpAndSettle();

      gateway.releaseFetch.complete();
      await tester.pumpAndSettle();

      expect(notifications.titles, isEmpty);
    },
  );

  testWidgets('saved-account fetch cannot notify after workspace disposal', (
    tester,
  ) async {
    final gateway = DelayedSavedAccountGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: MultiAccountRestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(
      const MessageEvent(
        conversationId: 'other-chat',
        accountId: 'account-secondary',
        accountActive: false,
        resourceType: 'NewMessage',
      ),
    );
    await tester.pump();
    await gateway.fetchStarted.future;

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    gateway.releaseFetch.complete();
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });

  testWidgets(
    'does not notify for own messages when user id formatting differs',
    (tester) async {
      final gateway = OwnIdVariationWatchingGateway();
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
    },
  );

  testWidgets('activity finishing after open does not notify the open chat', (
    tester,
  ) async {
    final gateway = ActivityOpenRaceGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
    await tester.pump();
    await tester.pump();
    expect(gateway.activityStarted.isCompleted, isTrue);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pump();
    await tester.pump();
    expect(gateway.selectedRefreshStarted.isCompleted, isTrue);

    gateway.releaseActivity.complete(const [
      MessageSummary(
        id: 'grace-new',
        sender: 'Grace Hopper',
        timestamp: '2026-09-04T13:01:00Z',
        content: 'Grace new while opening',
      ),
    ]);
    await tester.pumpAndSettle();

    expect(notifications.bodies, isEmpty);
  });

  testWidgets(
    'stale background activity cannot replace newer selected history',
    (tester) async {
      final gateway = ActivitySelectionRaceGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Chat with Grace'));
      await tester.pumpAndSettle();
      expect(selectableMessage('Grace baseline'), findsOneWidget);

      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();
      gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
      await tester.pump();
      await tester.pump();
      expect(gateway.activityStarted.isCompleted, isTrue);

      await tester.tap(find.text('Chat with Grace'));
      await tester.pumpAndSettle();
      expect(selectableMessage('Grace fresh selected'), findsOneWidget);

      gateway.releaseActivity.complete(const [
        MessageSummary(
          id: 'grace-stale',
          sender: 'Grace Hopper',
          timestamp: '2026-09-04T12:01:00Z',
          content: 'Grace stale activity',
        ),
      ]);
      await tester.pumpAndSettle();

      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Grace'));
      await tester.pump();
      await tester.pump();
      expect(gateway.reopenRefreshStarted.isCompleted, isTrue);

      expect(selectableMessage('Grace fresh selected'), findsOneWidget);
      expect(selectableMessage('Grace stale activity'), findsNothing);
    },
  );

  testWidgets('stale activity responses do not override newer activity', (
    tester,
  ) async {
    final gateway = OutOfOrderActivityGateway();
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
    await tester.pump();
    await gateway.firstReadStarted.future;

    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pump();
    await tester.pump();
    expect(notifications.bodies, ['Newer activity']);

    gateway.releaseFirstRead.complete();
    await tester.pumpAndSettle();

    expect(notifications.bodies, ['Newer activity']);
  });

  testWidgets('retries a reaction notification after delivery fails', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.reactions': true});
    final gateway = HistoricalReactionNotificationGateway();
    final notifications = FailingOnceConversationNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    const event = MessageEvent(
      conversationId: 'chat-ada',
      resourceType: 'MessageUpdate',
    );
    gateway.events.add(event);
    await tester.pumpAndSettle();
    gateway.reacted = true;
    gateway.events.add(event);
    await tester.pumpAndSettle();
    expect(notifications.showAttempts, 1);
    expect(notifications.titles, isEmpty);

    gateway.events.add(event);
    await tester.pumpAndSettle();

    expect(notifications.showAttempts, 2);
    expect(notifications.titles, ['❤️ Chat with Ada']);
  });

  testWidgets('notifies for reactions on an earlier own message', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.reactions': true});
    final gateway = HistoricalReactionNotificationGateway();
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

    gateway.reacted = true;
    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-ada',
        resourceType: 'MessageUpdate',
      ),
    );
    await tester.pumpAndSettle();

    expect(notifications.titles, ['❤️ Chat with Ada']);
    expect(notifications.bodies, ['❤️ Reacted to your message']);
  });

  testWidgets('queued chat refresh survives an earlier refresh failure', (
    tester,
  ) async {
    final gateway = FailingQueuedRefreshGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pump();
    await gateway.refreshStarted.future;

    gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
    await tester.pump();
    gateway.releaseFailure.complete();
    await tester.pumpAndSettle();

    expect(gateway.listCalls, greaterThanOrEqualTo(3));
  });

  testWidgets('stale refresh preserves a queued preview reconciliation', (
    tester,
  ) async {
    final gateway = StaleQueuedReconcileGateway();
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
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pump();
    await gateway.refreshStarted.future;

    gateway.events.add(const MessageEvent(conversationId: ''));
    await tester.pump();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await gateway.newerLoadApplied.future;
    await tester.pump();

    gateway.releaseStaleRefresh.complete();
    await tester.pumpAndSettle();

    expect(gateway.listCalls, greaterThanOrEqualTo(4));
    expect(notifications.titles, ['Chat with Grace']);
    expect(notifications.bodies, ['Queued reconcile message']);
  });

  testWidgets('notifies when a message arrives during initial chat loading', (
    tester,
  ) async {
    final gateway = StartupConversationWatchingGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    for (
      var index = 0;
      index < 10 && !gateway.chatsRequested.isCompleted;
      index++
    ) {
      await tester.pump();
    }
    expect(gateway.chatsRequested.isCompleted, isTrue);

    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-startup',
        resourceType: 'NewMessage',
      ),
    );
    gateway.releaseChats.complete();
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Startup Grace']);
    expect(notifications.bodies, ['Arrived during startup']);
    expect(notifications.conversationIds, ['chat-startup']);
  });

  testWidgets('notifies when a message arrives in a newly discovered chat', (
    tester,
  ) async {
    final gateway = NewConversationWatchingGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.revealNewConversation = true;
    gateway.events.add(
      const MessageEvent(
        conversationId: 'chat-new',
        resourceType: 'NewMessage',
      ),
    );
    await tester.pumpAndSettle();

    expect(notifications.titles, ['Chat with Grace']);
    expect(notifications.bodies, ['Hello from a new chat']);
    expect(notifications.conversationIds, ['chat-new']);
  });

  testWidgets('does not notify for channel traffic by default', (tester) async {
    final gateway = WatchingChannelsGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'channel-general'));
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });

  testWidgets('channel notifications show the latest message when enabled', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.channels': true});
    final gateway = WatchingChannelsGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    gateway.events.add(const MessageEvent(conversationId: 'channel-general'));
    await tester.pumpAndSettle();

    expect(notifications.titles, ['General']);
    expect(notifications.bodies, ['Ada Lovelace: Channel message']);
    expect(notifications.groupConversations, [isTrue]);
  });

  testWidgets('coalesces bursty unknown channel team loads', (tester) async {
    final gateway = BurstyUnknownChannelGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    for (var index = 0; index < 4; index++) {
      gateway.events.add(
        MessageEvent(conversationId: 'unknown-channel-$index'),
      );
    }
    for (
      var attempt = 0;
      attempt < 10 && gateway.maxConcurrentTeamLoads < 2;
      attempt++
    ) {
      await tester.pump();
    }

    expect(gateway.maxConcurrentTeamLoads, 1);
    gateway.releaseTeamLoads.complete();
    await tester.pumpAndSettle();
    expect(gateway.teamLoads, 3);
  });

  testWidgets('discovers a new channel when its first message arrives', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({'notifications.channels': true});
    final gateway = NewlyDiscoveredChannelGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    expect(gateway.teamLoads, 1);
    gateway.events.add(const MessageEvent(conversationId: 'channel-general'));
    await tester.pumpAndSettle();

    expect(gateway.teamLoads, greaterThanOrEqualTo(2));
    expect(notifications.titles, ['General']);
    expect(notifications.bodies, ['Ada Lovelace: Channel message']);
  });

  testWidgets(
    'keeps channel activity that arrives during initial team loading',
    (tester) async {
      SharedPreferences.setMockInitialValues({'notifications.channels': true});
      final gateway = StartupChannelWatchingGateway();
      final notifications = RecordingDesktopNotifications();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: gateway,
          notifications: notifications,
        ),
      );

      for (
        var attempt = 0;
        attempt < 10 && !gateway.teamsRequested.isCompleted;
        attempt++
      ) {
        await tester.pump();
      }
      expect(gateway.teamsRequested.isCompleted, isTrue);
      gateway.events.add(const MessageEvent(conversationId: 'channel-general'));
      await tester.pump();
      gateway.releaseTeams.complete();
      await tester.pumpAndSettle();

      expect(notifications.titles, ['General']);
      expect(notifications.bodies, ['Ada Lovelace: Channel message']);
    },
  );

  testWidgets('message notifications carry the sender avatar', (tester) async {
    final gateway = NotificationAvatarGateway();
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

    expect(notifications.senderNames, ['Ada Lovelace']);
    expect(notifications.senderAvatars.single, testPng);
    expect(notifications.groupConversations, [isFalse]);
  });

  testWidgets('avatar lookup cannot notify a chat opened while it waits', (
    tester,
  ) async {
    final gateway = DelayedNotificationAvatarGateway();
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
    for (
      var attempt = 0;
      attempt < 10 && !gateway.avatarStarted.isCompleted;
      attempt++
    ) {
      await tester.pump();
    }
    expect(gateway.avatarStarted.isCompleted, isTrue);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    gateway.releaseAvatar.complete(testPng);
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });

  testWidgets('avatar lookup cannot notify a chat muted while it waits', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = DelayedNotificationAvatarGateway();
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: gateway,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    await tester.longPress(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(find.text('Mute notifications'), findsOneWidget);

    gateway.changedPreview = true;
    gateway.events.add(const MessageEvent(conversationId: ''));
    for (
      var attempt = 0;
      attempt < 10 && !gateway.avatarStarted.isCompleted;
      attempt++
    ) {
      await tester.pump();
    }
    expect(gateway.avatarStarted.isCompleted, isTrue);

    await tester.tap(find.text('Mute notifications'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Mute notifications'), findsNothing);
    expect(find.byIcon(Icons.notifications_off_outlined), findsOneWidget);

    gateway.releaseAvatar.complete(testPng);
    await tester.pumpAndSettle();

    expect(notifications.titles, isEmpty);
  });

  testWidgets('tapping outside the composer removes its focus', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: MessagesGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.pump();
    final editable = tester.widget<EditableText>(
      find.descendant(of: composer, matching: find.byType(EditableText)),
    );
    expect(editable.focusNode.hasFocus, isTrue);

    await tester.tapAt(const Offset(1000, 300));
    await tester.pump();
    expect(editable.focusNode.hasFocus, isFalse);
  });

  testWidgets('opening mobile navigation releases composer focus', (
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
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-1')),
    );
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.pump();
    final editable = tester.widget<EditableText>(
      find.descendant(of: composer, matching: find.byType(EditableText)),
    );
    expect(editable.focusNode.hasFocus, isTrue);

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    expect(editable.focusNode.hasFocus, isFalse);

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-1')),
    );
    await tester.pumpAndSettle();
    final reopenedComposer = find.byType(TextField).last;
    final reopenedEditable = tester.widget<EditableText>(
      find.descendant(
        of: reopenedComposer,
        matching: find.byType(EditableText),
      ),
    );
    expect(reopenedEditable.focusNode.hasFocus, isFalse);
  });

  testWidgets('returning from settings does not restore composer focus', (
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

    final composer = find.byType(TextField).last;
    await tester.tap(composer);
    await tester.pump();
    final editable = tester.widget<EditableText>(
      find.descendant(of: composer, matching: find.byType(EditableText)),
    );
    expect(editable.focusNode.hasFocus, isTrue);

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();

    expect(editable.focusNode.hasFocus, isFalse);
  });

  testWidgets('keeps message drafts scoped to their conversation', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = WatchingMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'Ada draft');

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      isEmpty,
    );

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      'Ada draft',
    );
  });

  testWidgets('stops watching messages when the workspace is disposed', (
    tester,
  ) async {
    final gateway = WatchingMessagesGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(gateway.readCount, 1);

    await tester.pumpWidget(const SizedBox());
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pump();

    expect(gateway.readCount, 1);
    expect(gateway.stopCount, 1);
  });

  testWidgets('retries image hydration after a newer refresh arrives', (
    tester,
  ) async {
    final gateway = PendingImageHydrationGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    await gateway.hydrationStarted.future;

    gateway.newerMessage = true;
    gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
    await tester.pump();
    await tester.pump();
    expect(selectableMessage('New image message'), findsOneWidget);

    gateway.releaseHydration.complete();
    await tester.pumpAndSettle();

    expect(gateway.hydrationCalls, 2);
  });

  testWidgets('refreshes a newly selected chat during another chat refresh', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = DelayedRefreshSwitchGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();

    gateway.delayNextAdaRefresh();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pump();
    expect(gateway.hasPendingAdaRefresh, isTrue);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pump();
    expect(gateway.readCounts['chat-grace'], 2);

    gateway.completeAdaRefresh();
    await tester.pumpAndSettle();

    expect(gateway.readCounts['chat-ada'], 2);
    expect(gateway.readCounts['chat-grace'], 2);
  });

  testWidgets('equivalent refresh leaves a cached history fling alone', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = EquivalentDelayedResponsiveHistoryGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    Future<void> select(String id) async {
      await tester.tap(find.byTooltip('Open navigation'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(ValueKey('conversation-tile-chat-$id')));
      await tester.pumpAndSettle();
      await tester.pump(const Duration(milliseconds: 400));
    }

    await select('chat-ada');
    await select('chat-grace');
    gateway.delayNextAdaRefresh();
    await select('chat-ada');

    final messageList = find.byWidgetPredicate(
      (widget) => widget is ListView && widget.reverse,
    );
    expect(messageList, findsOneWidget);
    final hydrationCalls = gateway.hydrationCalls;

    await tester.fling(messageList, const Offset(0, 600), 4000);
    await tester.pump(const Duration(milliseconds: 16));
    final listBeforeRefresh = tester.widget<ListView>(messageList);
    gateway.completeAdaRefresh();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 32));

    expect(tester.widget<ListView>(messageList), same(listBeforeRefresh));
    await tester.pump(const Duration(milliseconds: 400));
    expect(gateway.hydrationCalls, hydrationCalls);
  });

  testWidgets('reuses cached history while refreshing a chat in background', (
    tester,
  ) async {
    final gateway = DelayedResponsiveHistoryGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(selectableMessage('Old Ada history'), findsOneWidget);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    gateway.delayNextAdaRefresh();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pump();

    expect(selectableMessage('Old Ada history'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);

    gateway.completeAdaRefresh();
    await tester.pumpAndSettle();
    expect(selectableMessage('Current Ada history'), findsOneWidget);
  });

  testWidgets(
    'stops watching the previous conversation after switching chats',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = WatchingMessagesGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Grace'));
      await tester.pumpAndSettle();
      gateway.events.add(const MessageEvent(conversationId: 'chat-ada'));
      await tester.pumpAndSettle();

      expect(gateway.readCounts['chat-ada'], 2);

      gateway.events.add(const MessageEvent(conversationId: 'chat-grace'));
      await tester.pumpAndSettle();

      expect(gateway.readCounts['chat-ada'], 2);
      expect(gateway.readCounts['chat-grace'], 2);
    },
  );

  testWidgets(
    'searches chats and retains a description when a preview is absent',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: SplitChatsGateway(),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Groups'), findsOneWidget);
      expect(find.text('Direct messages'), findsOneWidget);
      expect(find.text('No recent messages'), findsNWidgets(2));

      await tester.enterText(find.byType(TextField).first, 'project');
      await tester.pumpAndSettle();

      expect(find.text('Project group'), findsOneWidget);
      expect(find.text('Chat with Ada'), findsNothing);

      await tester.tap(find.text('Groups'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('conversation-tile-chat-group-1')),
        findsNothing,
      );
      await tester.tap(find.text('Groups'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('conversation-tile-chat-group-1')),
        findsOneWidget,
      );

      await tester.enterText(find.byType(TextField).first, '');
      await tester.pumpAndSettle();
      await tester.tap(find.text('Groups'));
      await tester.tap(find.text('Direct messages'));
      await tester.pumpAndSettle();

      expect(find.text('Project group'), findsNothing);
      expect(find.text('Chat with Ada'), findsNothing);

      await tester.enterText(find.byType(TextField).first, 'project');
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('conversation-tile-chat-group-1')),
        findsOneWidget,
      );

      await tester.enterText(find.byType(TextField).first, 'chat with ada');
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
        findsOneWidget,
      );

      await tester.enterText(
        find.byType(TextField).first,
        'no-such-conversation-9a71',
      );
      await tester.pumpAndSettle();
      expect(find.text('No chats found.'), findsOneWidget);
    },
  );

  testWidgets('renders tenant custom reaction assets', (tester) async {
    final gateway = CustomReactionIconGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final reaction = find.byKey(
      const ValueKey('reaction-party-parrot;0-sau-d4-asset-unselected'),
    );
    expect(reaction, findsOneWidget);
    expect(
      find.descendant(of: reaction, matching: find.byType(Image)),
      findsOneWidget,
    );
    expect(gateway.requestedReactionTypes, ['party-parrot;0-sau-d4-asset']);
  });

  testWidgets('sends a selected reaction for a message', (tester) async {
    final gateway = ReactionsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(
      tester.getCenter(selectableMessage('React to this message')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('👍'));
    await tester.pumpAndSettle();

    expect(gateway.reactionType, 'like');
  });

  testWidgets('pending reaction in one chat does not block another chat', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = CrossChatDelayedReactionGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(
      tester.getCenter(selectableMessage('React while switching chats')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('👍'));
    await tester.pump();
    await gateway.reactionStarted.future;

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
    );
    await tester.pumpAndSettle();
    await mouse.moveTo(tester.getCenter(selectableMessage('Grace stale')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('quick-reaction-like')), findsOneWidget);
    await tester.tap(find.text('👍'));
    await tester.pump();

    expect(gateway.reactionConversationIds, ['chat-ada', 'chat-grace']);
    gateway.releaseReaction.complete();
    await tester.pumpAndSettle();
    await tester.pump(const Duration(milliseconds: 350));
  });

  testWidgets(
    'reaction ownership does not leak across chats with matching ids',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = SameMessageIdCrossChatReactionGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();

      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(
          const ValueKey<Object>((
            'chat',
            null,
            'chat-ada',
            'shared-message-id',
          )),
        ),
        findsOneWidget,
      );
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(tester.getCenter(selectableMessage('Ada shared id')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('👍'));
      await tester.pump();
      await gateway.reactionStarted.future;

      await tester.tap(
        find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
      );
      await tester.pumpAndSettle();

      expect(selectableMessage('Grace shared id'), findsOneWidget);
      expect(
        find.byKey(
          const ValueKey<Object>((
            'chat',
            null,
            'chat-grace',
            'shared-message-id',
          )),
        ),
        findsOneWidget,
      );
      expect(
        find.byKey(
          const ValueKey<Object>((
            'chat',
            null,
            'chat-ada',
            'shared-message-id',
          )),
        ),
        findsNothing,
      );
      expect(
        find.byKey(const ValueKey('reaction-like-selected')),
        findsNothing,
      );

      gateway.releaseReaction.complete();
      await tester.pumpAndSettle();
    },
  );

  testWidgets('pending reaction cannot suppress another chat refresh', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = CrossChatDelayedReactionGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
    );
    await tester.pumpAndSettle();
    expect(selectableMessage('Grace stale'), findsOneWidget);

    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-ada')),
    );
    await tester.pumpAndSettle();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(
      tester.getCenter(selectableMessage('React while switching chats')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('👍'));
    await tester.pump();
    await gateway.reactionStarted.future;

    gateway.graceFresh = true;
    await tester.tap(
      find.byKey(const ValueKey('conversation-tile-chat-chat-grace')),
    );
    await tester.pumpAndSettle();
    gateway.releaseReaction.complete();
    await tester.pumpAndSettle();

    expect(selectableMessage('Grace fresh'), findsOneWidget);
  });

  testWidgets(
    'applies reactions immediately and closes the menu before the server replies',
    (tester) async {
      final gateway = DelayedReactionGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(
        tester.getCenter(selectableMessage('React to this message')),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('quick-reaction-like')));
      await tester.pumpAndSettle();

      expect(gateway.reactionType, 'like');
      expect(gateway.reactionCompleter.isCompleted, isFalse);
      expect(find.byKey(const ValueKey('quick-reaction-like')), findsNothing);
      final reactionTray = find.byKey(
        const ValueKey('message-reactions-message-1'),
      );
      expect(reactionTray, findsOneWidget);
      expect(
        find.descendant(of: reactionTray, matching: find.text('👍')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: reactionTray, matching: find.text('1')),
        findsOneWidget,
      );
      final selectedReaction = find.byKey(
        const ValueKey('reaction-like-selected'),
      );
      expect(selectedReaction, findsOneWidget);
      expect(
        tester.getSize(selectedReaction).height,
        tester.getSize(reactionTray).height,
      );

      gateway.serveStaleReactionAfterAcceptance = true;
      gateway.reactionCompleter.complete();
      await tester.pumpAndSettle();
      expect(reactionTray, findsOneWidget);
      expect(
        find.byKey(const ValueKey('reaction-like-selected')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: reactionTray, matching: find.text('1')),
        findsOneWidget,
      );
    },
  );

  testWidgets('existing reaction toggles mine off and back on', (tester) async {
    final gateway = HistoricalReactionGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(
      find.byKey(const ValueKey('reaction-like-selected')),
      findsOneWidget,
    );

    var tray = find.byKey(const ValueKey('message-reactions-message-history'));
    await tester.tap(find.descendant(of: tray, matching: find.text('👍')));
    await tester.pumpAndSettle();
    expect(gateway.removingActions, [true]);
    expect(
      find.byKey(const ValueKey('reaction-like-unselected')),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('message-reactions-message-history')),
        matching: find.text('1'),
      ),
      findsOneWidget,
    );

    tray = find.byKey(const ValueKey('message-reactions-message-history'));
    await tester.tap(find.descendant(of: tray, matching: find.text('👍')));
    await tester.pumpAndSettle();
    expect(gateway.removingActions, [true, false]);
    expect(
      find.byKey(const ValueKey('reaction-like-selected')),
      findsOneWidget,
    );
  });

  testWidgets(
    'reaction tray hugs the bubble edge without covering text or the next message',
    (tester) async {
      final gateway = DelayedReactionGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(
        tester.getCenter(selectableMessage('React to this message')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('quick-reaction-like')));
      await tester.pumpAndSettle();

      final bubbleRect = tester.getRect(
        find.byKey(const ValueKey('message-bubble-message-1')),
      );
      final reactionRect = tester.getRect(
        find.byKey(const ValueKey('message-reactions-message-1')),
      );
      final textRect = tester.getRect(
        selectableMessage('React to this message'),
      );
      final followingRect = tester.getRect(
        selectableMessage('Following message'),
      );

      expect(reactionRect.top, greaterThanOrEqualTo(textRect.bottom));
      expect(reactionRect.top, lessThan(bubbleRect.bottom));
      expect(reactionRect.bottom, greaterThan(bubbleRect.bottom));
      expect(reactionRect.bottom, lessThan(followingRect.top));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('message-reactions-message-1')),
          matching: find.text('👍'),
        ),
        findsOneWidget,
      );

      gateway.reactionCompleter.complete();
      await tester.pumpAndSettle();
    },
  );

  testWidgets('pending send in one chat does not block another chat', (
    tester,
  ) async {
    final gateway = CrossChatSendGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    var composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Ada pending');
    await tester.tap(find.byTooltip('Send message'));
    await gateway.adaSendStarted.future;
    await tester.pump();

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Grace should send now');
    await tester.tap(find.byTooltip('Send message'));
    await tester.pump();

    gateway.releaseAdaSend.complete();
    await tester.pumpAndSettle();

    expect(gateway.sentConversationIds, ['chat-ada', 'chat-grace']);
  });

  testWidgets('sending promotes the chat immediately', (tester) async {
    final gateway = DelayedFailingSendGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Promote this chat');
    await tester.tap(find.byTooltip('Send message'));
    await gateway.sendStarted.future;
    await tester.pump();

    final ada = find.byKey(const ValueKey('conversation-tile-chat-chat-ada'));
    final grace = find.byKey(
      const ValueKey('conversation-tile-chat-chat-grace'),
    );
    expect(tester.getTopLeft(grace).dy, lessThan(tester.getTopLeft(ada).dy));

    gateway.releaseSend.complete();
    await tester.pumpAndSettle();
  });

  testWidgets('failed send after switching chats restores the original draft', (
    tester,
  ) async {
    final gateway = DelayedFailingSendGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Do not lose this message');
    await tester.tap(find.byTooltip('Send message'));
    await gateway.sendStarted.future;
    await tester.pump();
    expect(selectableMessage('Do not lose this message'), findsOneWidget);

    await tester.tap(find.text('Chat with Grace'));
    await tester.pumpAndSettle();
    gateway.releaseSend.complete();
    await tester.pumpAndSettle();

    expect(find.textContaining('send failed'), findsNothing);

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(
      tester.widget<TextField>(find.byType(TextField).last).controller!.text,
      'Do not lose this message',
    );
  });

  testWidgets('failed send preserves newer composer text too', (tester) async {
    final gateway = DelayedFailingSendGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'First failed message');
    await tester.tap(find.byTooltip('Send message'));
    await gateway.sendStarted.future;
    await tester.pump();
    await tester.enterText(composer, 'Newer draft');

    gateway.releaseSend.complete();
    await tester.pumpAndSettle();

    expect(
      tester.widget<TextField>(composer).controller!.text,
      'First failed message\nNewer draft',
    );
  });

  testWidgets(
    'shows sent text and clears the composer before the server replies',
    (tester) async {
      final gateway = DelayedSendGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Chat with Ada'));
      await tester.pumpAndSettle();

      final composer = find.byType(TextField).last;
      await tester.enterText(composer, 'Instant outgoing message');
      await tester.tap(find.byTooltip('Send message'));
      await tester.pump();

      expect(gateway.sentContent, 'Instant outgoing message');
      expect(gateway.sendCompleter.isCompleted, isFalse);
      expect(selectableMessage('Instant outgoing message'), findsOneWidget);
      expect(tester.widget<TextField>(composer).controller!.text, isEmpty);

      gateway.sendCompleter.complete();
      await tester.pumpAndSettle();
      expect(selectableMessage('Instant outgoing message'), findsOneWidget);
    },
  );

  testWidgets(
    'does not duplicate a group message when own id formatting differs',
    (tester) async {
      final gateway = FormattedOwnIdGroupOutgoingGateway();
      await tester.pumpWidget(
        OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Project group'));
      await tester.pumpAndSettle();

      final composer = find.byType(TextField).last;
      await tester.enterText(composer, 'Group outgoing message');
      await tester.tap(find.byTooltip('Send message'));
      await tester.pumpAndSettle();

      expect(selectableMessage('Group outgoing message'), findsOneWidget);
    },
  );

  testWidgets('does not duplicate an acknowledged direct message', (
    tester,
  ) async {
    final gateway = AmbiguousOutgoingGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Message to James');
    await tester.tap(find.byTooltip('Send message'));
    await tester.pumpAndSettle();

    expect(selectableMessage('Message to James'), findsOneWidget);
  });

  testWidgets('opens the reaction bar after a mobile long press', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: ReactionsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await tester.longPress(find.text('React to this message'));
    await tester.pumpAndSettle();

    expect(find.byTooltip('👍'), findsOneWidget);
  });

  testWidgets('keeps the reaction menu open while moving to a reaction', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: ReactionsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(
      tester.getCenter(selectableMessage('React to this message')),
    );
    await tester.pumpAndSettle();
    await mouse.moveTo(
      tester.getCenter(find.byKey(const ValueKey('quick-reaction-like'))),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('quick-reaction-like')), findsOneWidget);
  });

  testWidgets('floats the reaction control above the hovered message', (
    tester,
  ) async {
    final gateway = ReactionsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    final beforeHover = tester.getRect(
      selectableMessage('React to this message'),
    );
    final bubble = tester.getRect(
      find.byKey(const ValueKey('message-bubble-message-1')),
    );
    final header = tester.getRect(find.text('Ada Lovelace').last);
    expect(bubble.top - header.bottom, 4);
    await mouse.moveTo(
      tester.getCenter(selectableMessage('React to this message')),
    );
    await tester.pumpAndSettle();

    final messageRect = tester.getRect(
      selectableMessage('React to this message'),
    );
    final reactionRect = tester.getRect(
      find.byKey(const ValueKey('quick-reaction-like')),
    );

    expect(reactionRect.width, lessThan(32));
    expect(reactionRect.height, lessThan(30));
    expect(reactionRect.top, lessThan(messageRect.bottom));
    expect(reactionRect.bottom, lessThanOrEqualTo(bubble.top));
    expect(messageRect.top, beforeHover.top);
    expect(messageRect.bottom, beforeHover.bottom);
    await mouse.moveTo(reactionRect.center);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('quick-reaction-like')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('quick-reaction-like')));
    await tester.pumpAndSettle();
    expect(gateway.reactionType, 'like');
  });
}
