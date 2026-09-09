part of '../widget_test.dart';

class FakeCallGateway implements CallGateway {
  final callEvents = StreamController<CallUpdate>.broadcast();
  final started = <String>[];
  final startedVideo = <bool>[];
  final startedCalleeUserIds = <String?>[];
  final accepted = <String>[];
  final declined = <String>[];
  final microphoneStates = <bool>[];
  final speakerStates = <bool>[];
  int hangUps = 0;
  int stopCount = 0;

  @override
  Future<void> acceptCall(String callId) async => accepted.add(callId);

  @override
  Future<void> declineCall(String callId) async => declined.add(callId);

  @override
  Stream<CallUpdate> events() => callEvents.stream;

  @override
  Future<void> hangUp() async => hangUps++;

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async =>
      microphoneStates.add(enabled);

  @override
  Future<void> setSpeakerEnabled(bool enabled) async =>
      speakerStates.add(enabled);

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) async {
    started.add(conversationId);
    startedVideo.add(video);
    startedCalleeUserIds.add(calleeUserId);
  }

  @override
  Future<void> stopEvents() async => stopCount++;
}

class DelayedStartCallGateway extends FakeCallGateway {
  final startEntered = Completer<void>();
  final releaseStart = Completer<void>();
  bool remoteCallActive = false;

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) async {
    started.add(conversationId);
    startedVideo.add(video);
    startedCalleeUserIds.add(calleeUserId);
    if (!startEntered.isCompleted) startEntered.complete();
    await releaseStart.future;
    remoteCallActive = true;
  }

  @override
  Future<void> hangUp() async {
    hangUps++;
    remoteCallActive = false;
  }
}

class DelayedCallStopGateway extends FakeCallGateway {
  final stopStarted = Completer<void>();
  final releaseStop = Completer<void>();
  int eventStarts = 0;

  @override
  Stream<CallUpdate> events() {
    eventStarts++;
    return callEvents.stream;
  }

  @override
  Future<void> stopEvents() async {
    stopCount++;
    if (!stopStarted.isCompleted) stopStarted.complete();
    await releaseStop.future;
  }
}

class FakeMediaCallGateway extends FakeCallGateway
    implements MediaDeviceCallGateway {
  int microphonePreviewCalls = 0;
  int cameraPreviewCalls = 0;
  final selectedDevices = <MediaDeviceKind, String>{};

  @override
  Future<List<MediaDevice>> mediaDevices(MediaDeviceKind kind) async =>
      switch (kind) {
        MediaDeviceKind.microphone => const [
          MediaDevice('', 'System default'),
          MediaDevice('mic-1', 'Desk microphone'),
        ],
        MediaDeviceKind.speaker => const [
          MediaDevice('', 'System default'),
          MediaDevice('speaker-1', 'USB headset'),
        ],
        MediaDeviceKind.camera => const [
          MediaDevice('', 'System default'),
          MediaDevice('camera-1', 'Virtual camera'),
        ],
      };

  @override
  Future<void> selectMediaDevice(MediaDeviceKind kind, String id) async {
    selectedDevices[kind] = id;
  }

  @override
  Future<int> previewMicrophone() async {
    microphonePreviewCalls++;
    return 1000;
  }

  @override
  Future<bool> previewSpeaker() async => true;

  @override
  Future<CallVideoFrame?> previewCamera() async {
    cameraPreviewCalls++;
    return null;
  }
}

class FailingMediaEnumerationGateway extends FakeMediaCallGateway {
  final requestedKinds = <MediaDeviceKind>[];

  @override
  Future<List<MediaDevice>> mediaDevices(MediaDeviceKind kind) async {
    requestedKinds.add(kind);
    if (kind == MediaDeviceKind.microphone) {
      throw StateError('microphone unavailable');
    }
    return super.mediaDevices(kind);
  }
}

class DuplicateSpeakerGateway extends FakeMediaCallGateway {
  @override
  Future<List<MediaDevice>> mediaDevices(MediaDeviceKind kind) async =>
      kind == MediaDeviceKind.speaker
      ? const [
          MediaDevice('speaker-1', 'USB headset'),
          MediaDevice('speaker-1', 'USB headset'),
        ]
      : super.mediaDevices(kind);
}

class DelayedMediaSelectionGateway extends FakeMediaCallGateway {
  final selectionStarted = Completer<void>();
  final finishSelection = Completer<void>();

  @override
  Future<void> selectMediaDevice(MediaDeviceKind kind, String id) async {
    if (!selectionStarted.isCompleted) selectionStarted.complete();
    await finishSelection.future;
    await super.selectMediaDevice(kind, id);
  }
}

class FailingAcceptCallGateway extends FakeCallGateway {
  @override
  Future<void> acceptCall(String callId) async {
    accepted.add(callId);
    throw StateError('accept failed before remote acceptance');
  }
}

class TerminalAcceptFailureCallGateway extends FakeCallGateway {
  @override
  Future<void> acceptCall(String callId) async {
    accepted.add(callId);
    callEvents.add(
      CallUpdate(
        kind: CallUpdateKind.error,
        callId: callId,
        detail: 'Incoming media failed after remote acceptance',
      ),
    );
  }
}

class FailingHangUpCallGateway extends FakeCallGateway {
  @override
  Future<void> hangUp() async {
    hangUps++;
    throw StateError('hang up failed');
  }
}

class DelayedDeclineCallGateway extends FakeCallGateway {
  final declineStarted = Completer<void>();
  final releaseDecline = Completer<void>();

  @override
  Future<void> declineCall(String callId) async {
    declined.add(callId);
    declineStarted.complete();
    await releaseDecline.future;
  }
}

class AccountAwareCallGateway extends FakeCallGateway {
  AccountAwareCallGateway(this.activeAccountId);

  final String Function() activeAccountId;
  final declineAccountIds = <String>[];

  @override
  Future<void> declineCall(String callId) async {
    declineAccountIds.add(activeAccountId());
    await super.declineCall(callId);
  }
}

class FailingAccountAwareCallGateway extends AccountAwareCallGateway {
  FailingAccountAwareCallGateway(super.activeAccountId);

  @override
  Future<void> declineCall(String callId) async {
    await super.declineCall(callId);
    throw StateError('decline failed');
  }
}

class FailingOnceConversationNotifications
    extends RecordingDesktopNotifications {
  int showAttempts = 0;

  @override
  Future<void> showConversation({
    required String title,
    required String body,
    required String conversationId,
    required String senderName,
    Uint8List? senderAvatar,
    required bool groupConversation,
  }) async {
    showAttempts++;
    if (showAttempts == 1) throw StateError('notification failed');
    await super.showConversation(
      title: title,
      body: body,
      conversationId: conversationId,
      senderName: senderName,
      senderAvatar: senderAvatar,
      groupConversation: groupConversation,
    );
  }
}

class FailingOnceDesktopNotifications extends RecordingDesktopNotifications {
  int showAttempts = 0;

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {
    showAttempts++;
    if (showAttempts == 1) throw StateError('notification failed');
    await super.show(title: title, body: body, conversationId: conversationId);
  }
}

class RecordingDesktopNotifications
    implements DesktopNotifications, ConversationDesktopNotifications {
  final titles = <String>[];
  final bodies = <String>[];
  final conversationIds = <String?>[];
  final senderNames = <String>[];
  final senderAvatars = <Uint8List?>[];
  final groupConversations = <bool>[];
  final dismissedConversationIds = <String>[];
  final _selections = StreamController<String>.broadcast();

  @override
  Stream<String> get conversationSelections => _selections.stream;

  void selectConversation(String conversationId) {
    _selections.add(conversationId);
  }

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {
    titles.add(title);
    bodies.add(body);
    conversationIds.add(conversationId);
  }

  @override
  Future<void> showConversation({
    required String title,
    required String body,
    required String conversationId,
    required String senderName,
    Uint8List? senderAvatar,
    required bool groupConversation,
  }) async {
    titles.add(title);
    bodies.add(body);
    conversationIds.add(conversationId);
    senderNames.add(senderName);
    senderAvatars.add(senderAvatar);
    groupConversations.add(groupConversation);
  }

  @override
  Future<void> dismissConversation(String conversationId) async {
    dismissedConversationIds.add(conversationId);
  }
}
