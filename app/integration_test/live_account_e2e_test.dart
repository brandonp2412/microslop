import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';
import 'package:microslop/teams_gateway.dart';

const _selfChatName = String.fromEnvironment('MICROSLOP_E2E_SELF_CHAT');
const _platformLabel = String.fromEnvironment(
  'MICROSLOP_E2E_PLATFORM',
  defaultValue: 'unknown',
);

Future<MessageSummary> _waitForMessage(
  TeamsGateway gateway,
  Conversation conversation,
  bool Function(MessageSummary message) predicate, {
  Duration timeout = const Duration(seconds: 30),
}) async {
  final deadline = DateTime.now().add(timeout);
  Object? lastError;
  while (DateTime.now().isBefore(deadline)) {
    try {
      final messages = await gateway.readMessages(conversation);
      for (final message in messages.reversed) {
        if (predicate(message)) return message;
      }
    } catch (error) {
      lastError = error;
    }
    await Future<void>.delayed(const Duration(seconds: 1));
  }
  throw TestFailure(
    'Timed out waiting for matching message in ${conversation.name}. '
    'Last read error: ${lastError ?? 'none'}',
  );
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(initializePlatformBackend);

  testWidgets('live account end-to-end flow', (tester) async {
    expect(
      _selfChatName,
      isNotEmpty,
      reason: 'Set MICROSLOP_E2E_SELF_CHAT with --dart-define.',
    );

    final auth = createPlatformAuthGateway();
    final teams = createPlatformTeamsGateway();
    final calls = createPlatformCallGateway();

    final session = await auth.restoreWorkSession();
    expect(
      session.signedIn,
      isTrue,
      reason: 'A real cached work session is required.',
    );

    final user = await teams.getUser();
    expect(user.displayName, isNotEmpty);
    expect(user.id, isNotNull);
    final profilePhoto = await teams.getProfilePhoto(user.id!);
    expect(profilePhoto, isNotNull);
    expect(profilePhoto, isNotEmpty);

    final chats = await teams.listChats(limit: 100);
    expect(chats, isNotEmpty);
    final selfChat = chats.where(
      (chat) => !chat.isGroup && chat.name.trim() == _selfChatName.trim(),
    );
    expect(
      selfChat,
      hasLength(1),
      reason: 'CUD is only permitted in the requested self-chat.',
    );
    final self = selfChat.single;

    final initialMessages = await teams.readMessages(self);
    expect(initialMessages, isNotEmpty);

    final teamsList = await teams.listTeams();
    expect(teamsList, isNotEmpty);
    final channel = teamsList
        .expand((team) => team.channels)
        .cast<Conversation?>()
        .firstWhere((candidate) => candidate != null, orElse: () => null);
    expect(channel, isNotNull);
    await teams.readMessages(channel!);

    final teamPhoto = await teams.getTeamPhoto(teamsList.first.id);
    if (teamPhoto != null) expect(teamPhoto, isNotEmpty);

    final group = chats.where((chat) => chat.isGroup).firstOrNull;
    if (group != null) {
      final groupPhoto = await teams.getChatPhoto(group.id);
      if (groupPhoto != null) expect(groupPhoto, isNotEmpty);
      for (final memberId in group.avatarUserIds.take(2)) {
        final memberPhoto = await teams.getProfilePhoto(memberId);
        if (memberPhoto != null) expect(memberPhoto, isNotEmpty);
      }
    }

    final marker =
        'Microslop E2E $_platformLabel ${DateTime.now().toUtc().toIso8601String()}';

    final listenerReady = Completer<void>();
    final matchingEventCompleter = Completer<MessageEvent>();
    final messageSubscription = teams.messageEvents().listen((event) {
      if (event.conversationId == null || event.conversationId!.isEmpty) {
        if (!listenerReady.isCompleted) listenerReady.complete();
        return;
      }
      if (event.conversationId == self.id &&
          !matchingEventCompleter.isCompleted) {
        matchingEventCompleter.complete(event);
      }
    });
    await listenerReady.future.timeout(
      const Duration(seconds: 45),
      onTimeout: () => throw TestFailure(
        'Trouter did not authenticate and register within 45 seconds.',
      ),
    );

    await teams.sendMessage(self, marker);
    final sent = await _waitForMessage(
      teams,
      self,
      (message) => message.content.contains(marker),
    );
    expect(sent.id, isNotEmpty);
    expect(sent.isFromCurrentUser, isTrue);

    await teams.setReaction(self, sent, 'like');
    final reacted = await _waitForMessage(
      teams,
      self,
      (message) =>
          message.id == sent.id &&
          message.reactions.any(
            (reaction) => reaction.type == 'like' && reaction.count > 0,
          ),
    );
    expect(
      reacted.reactions.any(
        (reaction) => reaction.type == 'like' && reaction.count > 0,
      ),
      isTrue,
    );

    final png = Uint8List.fromList(
      base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=',
      ),
    );
    final imageCaption = '$marker image';
    await teams.sendImageMessage(self, png, 'image/png', caption: imageCaption);
    final imageMessage = await _waitForMessage(
      teams,
      self,
      (message) =>
          message.content.contains(imageCaption) && message.images.isNotEmpty,
    );
    expect(imageMessage.images.first.contentType, contains('image'));
    expect(imageMessage.images.first.bytes, isNotEmpty);

    // Trouter does not consistently echo a mutation back to the endpoint that
    // originated it. If Microsoft does echo it, validate the conversation id;
    // the authenticated registration above is the deterministic live-stream
    // assertion for this same-client test.
    final echoedEvent = await matchingEventCompleter.future.timeout(
      const Duration(seconds: 3),
      onTimeout: () => const MessageEvent(),
    );
    if (echoedEvent.conversationId != null) {
      expect(echoedEvent.conversationId, self.id);
    }
    await teams.stopMessageEvents();
    await messageSubscription.cancel();

    final connected = Completer<CallUpdate>();
    final finished = Completer<CallUpdate>();
    CallUpdate? lastCallUpdate;
    final callSubscription = calls.events().listen((event) {
      // ignore: avoid_print
      print(
        'LIVE_CALL_EVENT kind=${event.kind.name} conversation=${event.conversationId} detail=${event.detail}',
      );
      if (event.conversationId != microsoftTestCallConversationId) return;
      lastCallUpdate = event;
      if (event.kind == CallUpdateKind.connected && !connected.isCompleted) {
        connected.complete(event);
      }
      if ((event.kind == CallUpdateKind.ended ||
              event.kind == CallUpdateKind.error) &&
          !finished.isCompleted) {
        finished.complete(event);
      }
    });

    await calls.startCall(microsoftTestCallConversationId);
    try {
      final firstOutcome = await Future.any([connected.future, finished.future])
          .timeout(
            const Duration(seconds: 60),
            onTimeout: () => throw TestFailure(
              'Microsoft test call did not connect or terminate within 60 seconds. '
              'Last event: ${lastCallUpdate?.kind.name ?? 'none'}; '
              'detail=${lastCallUpdate?.detail ?? 'none'}',
            ),
          );
      if (firstOutcome.kind != CallUpdateKind.connected) {
        fail(
          'Microsoft test call terminated before connecting: '
          '${firstOutcome.kind.name}; detail=${firstOutcome.detail ?? 'none'}',
        );
      }
      await calls.setMicrophoneEnabled(false);
      await calls.setMicrophoneEnabled(true);
      await calls.setSpeakerEnabled(false);
      await calls.setSpeakerEnabled(true);
      // Exercise the real hang-up path after enough connected media has flowed
      // to prove microphone capture, transmit, receive, and playback plumbing.
      await Future<void>.delayed(const Duration(seconds: 12));
      await calls.hangUp();
      final terminal = await finished.future.timeout(
        const Duration(seconds: 30),
      );
      expect(
        terminal.kind,
        CallUpdateKind.ended,
        reason: terminal.detail ?? 'Microsoft test call failed.',
      );
      final result = parseMicrosoftTestCallResult(terminal.detail);
      expect(
        result.videoPacketsSent,
        greaterThan(0),
        reason: 'The Test Call must negotiate and transmit a video RTP stream.',
      );
      // ignore: avoid_print
      print(
        'LIVE_VIDEO_RESULT packetsSent=${result.videoPacketsSent} '
        'packetsReceived=${result.videoPacketsReceived} '
        'cameraFrames=${result.cameraFramesSent}',
      );
      expect(
        result.passed,
        isTrue,
        reason:
            'Test call did not prove two-way media. Detail: ${terminal.detail}',
      );
    } finally {
      await calls.hangUp();
      await calls.stopEvents();
      await callSubscription.cancel();
    }
  });
}

extension<T> on Iterable<T> {
  T? get firstOrNull => this.isEmpty ? null : first;
}
