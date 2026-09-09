part of '../widget_test.dart';

Future<void> _chooseDirectCall(
  WidgetTester tester, {
  required bool video,
}) async {
  final directAction = find.byTooltip(
    video ? 'Start video call' : 'Start audio call',
  );
  if (directAction.evaluate().isNotEmpty) {
    await tester.tap(directAction);
    await tester.pump();
    return;
  }
  await tester.tap(find.byTooltip('Start call'));
  await tester.pumpAndSettle();
  await tester.tap(find.text(video ? 'Video call' : 'Audio call'));
  await tester.pump();
}

void registerCallTests() {
  testWidgets(
    'speaker selection tolerates duplicate IDs and a missing default device',
    (tester) async {
      SharedPreferences.setMockInitialValues({
        'calls.device.speaker': 'speaker-1',
      });
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: FakeTeamsGateway(),
          callGateway: DuplicateSpeakerGateway(),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      await tester.scrollUntilVisible(
        find.text('Make a test call'),
        300,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.tap(find.text('Make a test call'));
      await tester.pumpAndSettle();
      final selector = tester.widget<DropdownButton<String>>(
        find.byWidgetPredicate(
          (widget) =>
              widget is DropdownButton<String> && widget.value == 'speaker-1',
        ),
      );
      expect(selector.items!.map((item) => item.value), ['', 'speaker-1']);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('runs the Microsoft test call as an interactive call', (
    tester,
  ) async {
    final calls = FakeCallGateway();
    final platformCalls = <MethodCall>[];
    const callAudioChannel = MethodChannel('microslop/call_audio');
    const callVideoChannel = MethodChannel('microslop/call_video');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(callAudioChannel, (call) async {
          platformCalls.add(call);
          return null;
        });
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(callVideoChannel, (call) async {
          platformCalls.add(call);
          return call.method == 'startCamera';
        });
    addTearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(callAudioChannel, null);
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(callVideoChannel, null);
    });
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Make a test call'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Make a test call'));
    await tester.pumpAndSettle();

    expect(calls.started, [microsoftTestCallConversationId]);
    expect(calls.startedVideo, [false]);
    expect(find.text('Microsoft Test Call Bot'), findsOneWidget);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'test-call',
        conversationId: microsoftTestCallConversationId,
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(find.textContaining('Connected'), findsOneWidget);
    expect(find.textContaining('speak when prompted'), findsOneWidget);
    expect(find.text('Mic on'), findsOneWidget);
    expect(find.text('Speaker'), findsOneWidget);
    expect(
      platformCalls.any((call) => call.method == 'startRingback'),
      PlatformCallAudio.supportsSpeakerRouting,
    );
    expect(
      platformCalls.any((call) => call.method == 'stopRingback'),
      PlatformCallAudio.supportsSpeakerRouting,
    );

    await tester.ensureVisible(find.text('Mic on'));
    await tester.tap(find.text('Mic on'));
    await tester.pump();
    expect(calls.microphoneStates, [false]);
    expect(find.text('Muted'), findsOneWidget);

    await tester.tap(find.text('Speaker'));
    await tester.pump();
    expect(
      calls.speakerStates,
      PlatformCallAudio.supportsSpeakerRouting ? isEmpty : [false],
    );
    expect(
      platformCalls.any(
        (call) =>
            call.method == 'setSpeakerphone' &&
            (call.arguments as Map<Object?, Object?>?)?['enabled'] == false,
      ),
      PlatformCallAudio.supportsSpeakerRouting,
    );
    expect(
      find.text(
        PlatformCallAudio.supportsSpeakerRouting ? 'Earpiece' : 'Speaker off',
      ),
      findsOneWidget,
    );

    await tester.ensureVisible(find.text('Hang up'));
    await tester.pump();
    await tester.tap(find.text('Hang up'));
    await tester.pump();
    expect(calls.hangUps, 1);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.ended,
        callId: 'test-call',
        conversationId: microsoftTestCallConversationId,
        detail:
            'test-call;accepted=true;sent=600;received=400;mic_frames=250;mic_non_silent=140;mic_peak=12000;speaker_frames=400;speaker_samples=64000;speaker_errors=0;video_sent=900;video_received=0;camera_frames=120;echo=false;delay_ms=0;correlation=0;reason=',
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(find.text('Audio diagnostics passed'), findsOneWidget);
    expect(
      find.text('600 sent · 400 received · 140 live mic frames · peak 12000'),
      findsOneWidget,
    );
    expect(
      find.text('400 speaker frames · 64000 samples rendered'),
      findsOneWidget,
    );
    expect(find.text('900 video packets sent · 0 received'), findsOneWidget);
    expect(
      find.text('Camera not verified · no live preview was available'),
      findsOneWidget,
    );
    await tester.scrollUntilVisible(
      find.text('Done'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    expect(find.text('Done'), findsOneWidget);
  });

  testWidgets('loads persisted media devices into the test call', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({
      'calls.device.microphone': 'mic-1',
      'calls.device.speaker': 'speaker-1',
      'calls.device.camera': 'camera-1',
    });
    final calls = FakeMediaCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Make a test call'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Make a test call'));
    await tester.pumpAndSettle();

    expect(find.text('Device settings'), findsOneWidget);
    expect(find.text('Desk microphone'), findsOneWidget);
    expect(find.text('USB headset'), findsOneWidget);
    expect(find.text('Virtual camera'), findsOneWidget);
    expect(calls.selectedDevices, {
      MediaDeviceKind.microphone: 'mic-1',
      MediaDeviceKind.speaker: 'speaker-1',
      MediaDeviceKind.camera: 'camera-1',
    });
  });

  testWidgets('late test-call camera start is stopped after disposal', (
    tester,
  ) async {
    PlatformCallVideo.debugSupportedOverride = true;
    addTearDown(() => PlatformCallVideo.debugSupportedOverride = null);
    const callVideoChannel = MethodChannel('microslop/call_video');
    final secondStartEntered = Completer<void>();
    final releaseSecondStart = Completer<bool>();
    var startCalls = 0;
    var stopCalls = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      callVideoChannel,
      (call) async {
        if (call.method == 'startCamera') {
          startCalls++;
          if (startCalls == 1) return false;
          if (!secondStartEntered.isCompleted) secondStartEntered.complete();
          return releaseSecondStart.future;
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
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Make a test call'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Make a test call'));
    await tester.pumpAndSettle();

    expect(startCalls, 1);
    await tester.tap(find.text('Camera'));
    await tester.pump();
    expect(find.text('Camera off'), findsOneWidget);
    await tester.tap(find.text('Camera off'));
    await secondStartEntered.future;

    await tester.pumpWidget(const SizedBox.shrink());
    releaseSecondStart.complete(true);
    await tester.pump();

    expect(stopCalls, 1);
    PlatformCallVideo.debugSupportedOverride = null;
  });

  testWidgets('test call survives media enumeration failure', (tester) async {
    final calls = FailingMediaEnumerationGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Make a test call'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Make a test call'));
    await tester.pumpAndSettle();

    expect(calls.started, [microsoftTestCallConversationId]);
    expect(
      calls.requestedKinds.sublist(calls.requestedKinds.length - 3),
      MediaDeviceKind.values,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('media enumeration failure does not abort settings', (
    tester,
  ) async {
    final calls = FailingMediaEnumerationGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Microphone'),
      300,
      scrollable: find.byType(Scrollable).first,
    );

    final microphoneRow = find.widgetWithText(ListTile, 'Microphone');
    expect(
      find.descendant(of: microphoneRow, matching: find.text('System default')),
      findsOneWidget,
    );
    expect(calls.requestedKinds, MediaDeviceKind.values);
    expect(tester.takeException(), isNull);
  });

  testWidgets('leaving settings during media selection is safe', (
    tester,
  ) async {
    final calls = DelayedMediaSelectionGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Microphone'),
      300,
      scrollable: find.byType(Scrollable).first,
    );

    final microphoneRow = find.widgetWithText(ListTile, 'Microphone');
    await tester.tap(
      find.descendant(
        of: microphoneRow,
        matching: find.byType(DropdownButton<String>),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Desk microphone').last);
    await calls.selectionStarted.future;
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();

    calls.finishSelection.complete();
    await tester.pump();

    expect(tester.takeException(), isNull);
  });

  testWidgets('copies a test-call failure to the clipboard', (tester) async {
    final calls = FakeCallGateway();
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Make a test call'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Make a test call'));
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.error,
        callId: 'test-call',
        conversationId: microsoftTestCallConversationId,
        detail: '1:1 call POST to epconv failed: connection aborted',
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy error'));
    await tester.pump();

    expect(copiedText, '1:1 call POST to epconv failed: connection aborted');
  });

  testWidgets('late outgoing camera start cannot place a call after disposal', (
    tester,
  ) async {
    PlatformCallVideo.debugSupportedOverride = true;
    addTearDown(() => PlatformCallVideo.debugSupportedOverride = null);
    const callVideoChannel = MethodChannel('microslop/call_video');
    final cameraStartEntered = Completer<void>();
    final releaseCameraStart = Completer<bool>();
    var stopCalls = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      callVideoChannel,
      (call) async {
        if (call.method == 'startCamera') {
          if (!cameraStartEntered.isCompleted) cameraStartEntered.complete();
          return releaseCameraStart.future;
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
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: true);
    await cameraStartEntered.future;
    await tester.pumpWidget(const SizedBox.shrink());
    releaseCameraStart.complete(true);
    await tester.pump();

    expect(calls.started, isEmpty);
    expect(stopCalls, 1);
    PlatformCallVideo.debugSupportedOverride = null;
  });

  testWidgets('late remote call start is hung up after disposal', (
    tester,
  ) async {
    final calls = DelayedStartCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: false);
    await calls.startEntered.future;
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(calls.hangUps, 1);

    calls.releaseStart.complete();
    await tester.pump();

    expect(calls.remoteCallActive, isFalse);
    expect(calls.hangUps, 2);
  });

  testWidgets('prefers the actual peer message identity for direct calls', (
    tester,
  ) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: CallIdentityTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: true);
    await tester.pumpAndSettle();

    expect(calls.started, ['chat-call-identity']);
    expect(calls.startedCalleeUserIds, [
      '33333333-3333-3333-3333-333333333333',
    ]);
  });

  testWidgets('uses the other chat member when messages have no peer identity', (
    tester,
  ) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: MemberIdentityTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Test Peer'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: true);
    await tester.pumpAndSettle();

    expect(calls.started, ['chat-member-identity']);
    expect(calls.startedCalleeUserIds, [
      '44444444-4444-4444-4444-444444444444',
    ]);
  });

  testWidgets('starts and hangs up a direct Teams call', (tester) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: false);
    await tester.pumpAndSettle();
    expect(calls.started, ['chat-1']);
    expect(calls.startedVideo, [false]);
    expect(calls.startedCalleeUserIds, [
      '22222222-2222-2222-2222-222222222222',
    ]);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'call-out',
        conversationId: 'chat-1',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.textContaining('Connected ·'), findsOneWidget);
    expect(find.text('Chats'), findsNothing);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'unrelated-incoming',
        displayName: 'Grace Hopper',
      ),
    );
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.ended,
        callId: 'old-call',
        conversationId: 'chat-old',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('Connected ·'), findsOneWidget);
    expect(find.text('Grace Hopper'), findsNothing);

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-microphone')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls.microphoneStates, [false]);

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-speaker')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls.speakerStates, [false]);
    expect(find.text('Muted'), findsOneWidget);

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-hang-up')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls.hangUps, 1);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'call-out',
        conversationId: 'chat-1',
      ),
    );
    await tester.pump();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();

    expect(find.textContaining('Connected ·'), findsNothing);
  });

  testWidgets(
    'declines a pending incoming call when the workspace is disposed',
    (tester) async {
      final calls = DelayedDeclineCallGateway();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: FakeTeamsGateway(),
          callGateway: calls,
        ),
      );
      await tester.pumpAndSettle();

      calls.callEvents.add(
        const CallUpdate(
          kind: CallUpdateKind.incoming,
          callId: 'call-in-dispose',
          displayName: 'Ada Lovelace',
        ),
      );
      await tester.pumpAndSettle();

      await tester.pumpWidget(const SizedBox.shrink());
      await calls.declineStarted.future;
      await tester.pump();

      expect(calls.declined, ['call-in-dispose']);
      expect(calls.stopCount, 1);
      expect(calls.hangUps, 0);

      calls.releaseDecline.complete();
      await tester.pumpAndSettle();
      expect(calls.stopCount, 1);
    },
  );

  testWidgets('keeps an active call visible when hang up fails', (
    tester,
  ) async {
    final calls = FailingHangUpCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await _chooseDirectCall(tester, video: false);
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'call-hangup-fail',
        conversationId: 'chat-1',
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-hang-up')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls.hangUps, 1);
    expect(find.textContaining('Connected ·'), findsOneWidget);
  });

  testWidgets('hangs up an active call when the workspace is disposed', (
    tester,
  ) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await _chooseDirectCall(tester, video: false);
    await tester.pumpAndSettle();

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpAndSettle();

    expect(calls.hangUps, 1);
  });

  testWidgets('records backend call failures in the application log', (
    tester,
  ) async {
    AppLog.entries.value = const [];
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await _chooseDirectCall(tester, video: true);
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.error,
        callId: 'call-failed',
        conversationId: 'chat-1',
        detail:
            'User invite failed (404): failure querying object from remote store',
      ),
    );
    await tester.pumpAndSettle();

    expect(
      AppLog.entries.value.last,
      contains(
        'User invite failed (404): failure querying object from remote store',
      ),
    );
  });

  testWidgets('records a call that ends before connection', (tester) async {
    AppLog.entries.value = const [];
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await _chooseDirectCall(tester, video: true);
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.ringing,
        callId: 'call-ended-before-connect',
        conversationId: 'chat-1',
      ),
    );
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.ended,
        callId: 'call-ended-before-connect',
        conversationId: 'chat-1',
      ),
    );
    await tester.pumpAndSettle();

    expect(AppLog.entries.value.last, contains('ended before connection'));
    expect(AppLog.entries.value.last, contains('while ringing'));
    AppLog.entries.value = const [];
  });

  testWidgets('records call event stream closure while a call is active', (
    tester,
  ) async {
    AppLog.entries.value = const [];
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await _chooseDirectCall(tester, video: false);
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.ringing,
        callId: 'call-stream-closed',
        conversationId: 'chat-1',
      ),
    );
    await tester.pumpAndSettle();

    await calls.callEvents.close();
    await tester.pumpAndSettle();

    expect(AppLog.entries.value.last, contains('Call event stream closed'));
    expect(AppLog.entries.value.last, contains('while ringing'));
    AppLog.entries.value = const [];
  });

  testWidgets('starts a direct Teams video call in video mode', (tester) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    await _chooseDirectCall(tester, video: true);
    await tester.pumpAndSettle();

    expect(calls.started, ['chat-1']);
    expect(calls.startedVideo, [true]);
    expect(calls.startedCalleeUserIds, [
      '22222222-2222-2222-2222-222222222222',
    ]);
  });

  testWidgets(
    'shows incoming video calls as video before and after acceptance',
    (tester) async {
      final calls = FakeCallGateway();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: FakeTeamsGateway(),
          callGateway: calls,
        ),
      );
      await tester.pumpAndSettle();

      calls.callEvents.add(
        const CallUpdate(
          kind: CallUpdateKind.incoming,
          callId: 'call-video-in',
          displayName: 'Ada Lovelace',
          detail: 'video=true',
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Incoming video call'), findsOneWidget);
      expect(find.text('AL'), findsOneWidget);
      expect(find.text('Video call'), findsOneWidget);
      expect(
        find.text('Your camera stays off when you answer'),
        findsOneWidget,
      );
      calls.callEvents.add(
        const CallUpdate(
          kind: CallUpdateKind.connected,
          callId: 'call-video-in',
          detail: 'video=true',
        ),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('Connected ·'), findsOneWidget);
      expect(find.text('Ada Lovelace'), findsOneWidget);

      calls.callEvents.add(
        const CallUpdate(kind: CallUpdateKind.ended, callId: 'call-video-in'),
      );
      await tester.pumpAndSettle();
      calls.callEvents.add(
        const CallUpdate(
          kind: CallUpdateKind.incoming,
          callId: 'call-audio-in',
          displayName: 'Grace Hopper',
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Incoming call'), findsOneWidget);
      expect(find.text('Incoming video call'), findsNothing);
    },
  );

  testWidgets('call controls fit a compact phone viewport', (tester) async {
    tester.view.physicalSize = const Size(320, 568);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'compact-video',
        displayName: 'Ada Lovelace',
        detail: 'video=true',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('call-accept')), findsOneWidget);
    expect(find.byKey(const ValueKey('call-decline')), findsOneWidget);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'compact-video',
        detail: 'video=true',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('call-microphone')), findsOneWidget);
    expect(find.byKey(const ValueKey('call-hang-up')), findsOneWidget);
    expect(find.byKey(const ValueKey('call-speaker')), findsOneWidget);
  });

  testWidgets('compact dark audio call keeps long caller identity', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 568);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    const caller = 'Ada Augusta Lovelace With A Very Long Display Name';
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'compact-audio',
        displayName: caller,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text(caller), findsOneWidget);
    expect(find.text('AN'), findsOneWidget);

    calls.callEvents.add(
      const CallUpdate(kind: CallUpdateKind.connected, callId: 'compact-audio'),
    );
    await tester.pumpAndSettle();
    expect(find.text(caller), findsOneWidget);
    expect(find.textContaining('Connected ·'), findsOneWidget);
    expect(find.byKey(const ValueKey('call-hang-up')), findsOneWidget);
  });

  testWidgets('automatically captures call UI screenshots', (tester) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(
      tester.binding.platformDispatcher.clearPlatformBrightnessTestValue,
    );
    SharedPreferences.setMockInitialValues({
      'notifications.backgroundWatcher': false,
    });

    Future<void> captureIncoming({
      required String name,
      required String caller,
      required bool video,
      required Brightness brightness,
      bool connected = false,
    }) async {
      tester.binding.platformDispatcher.platformBrightnessTestValue =
          brightness;
      final boundaryKey = GlobalKey();
      final calls = FakeCallGateway();
      await tester.pumpWidget(
        RepaintBoundary(
          key: boundaryKey,
          child: OstApp(
            gateway: RestoringAuthGateway(),
            teamsGateway: FakeTeamsGateway(),
            callGateway: calls,
          ),
        ),
      );
      await tester.pumpAndSettle();
      calls.callEvents.add(
        CallUpdate(
          kind: CallUpdateKind.incoming,
          callId: name,
          displayName: caller,
          detail: video ? 'video=true' : null,
        ),
      );
      await tester.pumpAndSettle();
      if (connected) {
        calls.callEvents.add(
          CallUpdate(
            kind: CallUpdateKind.connected,
            callId: name,
            detail: video ? 'video=true' : null,
          ),
        );
        await tester.pumpAndSettle();
      }
      await captureUiScreenshot(tester, boundaryKey, name);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
    }

    await captureIncoming(
      name: 'incoming-audio-light',
      caller: 'Ada Lovelace',
      video: false,
      brightness: Brightness.light,
    );
    await captureIncoming(
      name: 'incoming-video',
      caller: 'Ada Lovelace',
      video: true,
      brightness: Brightness.dark,
    );
    await captureIncoming(
      name: 'connected-audio-light',
      caller: 'Ada Lovelace',
      video: false,
      brightness: Brightness.light,
      connected: true,
    );
    await captureIncoming(
      name: 'connected-audio-dark',
      caller: 'Ada Lovelace',
      video: false,
      brightness: Brightness.dark,
      connected: true,
    );
  });

  testWidgets('accepts and declines incoming Teams calls', (tester) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'call-in-1',
        displayName: 'Ada Lovelace',
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Ada Lovelace'), findsOneWidget);
    expect(find.text('Incoming call'), findsOneWidget);
    expect(find.text('Chats'), findsNothing);
    final acceptButton = find.descendant(
      of: find.byKey(const ValueKey('call-accept')),
      matching: find.byType(IconButton),
    );
    expect(tester.widget<IconButton>(acceptButton).onPressed, isNotNull);
    tester.widget<IconButton>(acceptButton).onPressed!.call();
    await tester.pump(const Duration(milliseconds: 10));
    await tester.pumpAndSettle();
    expect(calls.accepted, ['call-in-1']);
    calls.callEvents.add(
      const CallUpdate(kind: CallUpdateKind.connected, callId: 'call-in-1'),
    );
    calls.callEvents.add(
      const CallUpdate(kind: CallUpdateKind.ended, callId: 'call-in-1'),
    );
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'call-in-2',
        displayName: 'Grace Hopper',
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-decline')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls.declined, ['call-in-2']);
  });

  testWidgets('keeps a retryable incoming call visible when acceptance fails', (
    tester,
  ) async {
    final calls = FailingAcceptCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'retryable-call',
        displayName: 'Ada Lovelace',
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-accept')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls.accepted, ['retryable-call']);
    expect(find.text('Ada Lovelace'), findsOneWidget);
    expect(find.byKey(const ValueKey('call-accept')), findsOneWidget);
  });

  testWidgets('clears an incoming call after terminal acceptance failure', (
    tester,
  ) async {
    final calls = TerminalAcceptFailureCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'terminal-call',
        displayName: 'Grace Hopper',
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey('call-accept')),
        matching: find.byType(IconButton),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls.accepted, ['terminal-call']);
    expect(find.text('Grace Hopper'), findsNothing);
    expect(find.byKey(const ValueKey('call-accept')), findsNothing);
  });

  testWidgets('does not replace a pending incoming call with another call', (
    tester,
  ) async {
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'call-in-1',
        displayName: 'Ada Lovelace',
      ),
    );
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'call-in-2',
        displayName: 'Grace Hopper',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Ada Lovelace'), findsOneWidget);
    expect(find.text('Grace Hopper'), findsNothing);
  });
}
