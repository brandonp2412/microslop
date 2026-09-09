import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';
import 'package:microslop/teams_gateway.dart';

const _selfChatName = String.fromEnvironment('MICROSLOP_E2E_SELF_CHAT');

@JS('document')
external JSObject get _document;

void _setBrowserResult(bool passed) {
  final result = passed ? 'pass' : 'fail';
  _document.setProperty(
    'title'.toJS,
    'MICROSLOP_E2E_${result.toUpperCase()}'.toJS,
  );
  _document
      .getProperty<JSObject?>('body'.toJS)
      ?.callMethod<JSAny?>(
        'setAttribute'.toJS,
        'data-microslop-e2e'.toJS,
        result.toJS,
      );
}

void _require(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Future<MessageSummary> _waitForMessage(
  TeamsGateway gateway,
  Conversation conversation,
  bool Function(MessageSummary message) predicate, {
  Duration timeout = const Duration(seconds: 45),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    final messages = await gateway.readMessages(conversation);
    for (final message in messages.reversed) {
      if (predicate(message)) return message;
    }
    await Future<void>.delayed(const Duration(seconds: 1));
  }
  throw StateError('Timed out waiting for a message in ${conversation.name}.');
}

Future<void> _run() async {
  _require(
    _selfChatName.trim().isNotEmpty,
    'MICROSLOP_E2E_SELF_CHAT is required.',
  );

  final auth = createPlatformAuthGateway();
  final teams = createPlatformTeamsGateway();
  final calls = createPlatformCallGateway();

  final session = await auth.restoreWorkSession();
  _require(session.signedIn, 'A cached work session is required.');

  final user = await teams.getUser();
  _require(
    user.displayName.trim().isNotEmpty,
    'Current user is missing a display name.',
  );
  if (user.id?.trim().isNotEmpty == true) {
    final photo = await teams.getProfilePhoto(user.id!);
    _require(
      photo?.isNotEmpty == true,
      'Current user profile photo is missing.',
    );
  }

  final chats = await teams.listChats(limit: 100);
  _require(chats.isNotEmpty, 'No chats were returned.');
  final selfMatches = chats.where(
    (chat) => !chat.isGroup && chat.name.trim() == _selfChatName.trim(),
  );
  _require(selfMatches.length == 1, 'Self-chat was not uniquely identified.');
  final self = selfMatches.single;

  final initialMessages = await teams.readMessages(self);
  _require(initialMessages.isNotEmpty, 'Self-chat has no readable messages.');

  final teamList = await teams.listTeams();
  _require(teamList.isNotEmpty, 'No teams were returned.');
  final channels = teamList.expand((team) => team.channels);
  _require(channels.isNotEmpty, 'No channels were returned.');
  await teams.readMessages(channels.first);

  final ready = Completer<void>();
  final matchingEvent = Completer<MessageEvent>();
  final messageSubscription = teams.messageEvents().listen((event) {
    if (event.conversationId == null || event.conversationId!.isEmpty) {
      if (!ready.isCompleted) ready.complete();
      return;
    }
    if (event.conversationId == self.id && !matchingEvent.isCompleted) {
      matchingEvent.complete(event);
    }
  });
  await ready.future.timeout(const Duration(seconds: 45));

  final marker =
      'Microslop E2E chrome ${DateTime.now().toUtc().toIso8601String()}';
  await teams.sendMessage(self, marker);
  final sent = await _waitForMessage(
    teams,
    self,
    (message) => message.content.contains(marker),
  );
  _require(sent.id.isNotEmpty, 'Sent message has no id.');
  _require(
    sent.isFromCurrentUser,
    'Sent message was not classified as current user.',
  );

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
  _require(
    reacted.reactions.any(
      (reaction) => reaction.type == 'like' && reaction.count > 0,
    ),
    'Reaction did not round-trip.',
  );

  final png = Uint8List.fromList(
    base64Decode(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=',
    ),
  );
  final caption = '$marker image';
  await teams.sendImageMessage(self, png, 'image/png', caption: caption);
  final image = await _waitForMessage(
    teams,
    self,
    (message) => message.content.contains(caption) && message.images.isNotEmpty,
  );
  _require(image.images.first.bytes.isNotEmpty, 'Image payload is empty.');

  final event = await matchingEvent.future.timeout(
    const Duration(seconds: 3),
    onTimeout: () => const MessageEvent(),
  );
  if (event.conversationId != null) {
    _require(
      event.conversationId == self.id,
      'Message event targeted the wrong conversation.',
    );
  }
  await teams.stopMessageEvents();
  await messageSubscription.cancel();

  final connected = Completer<CallUpdate>();
  final finished = Completer<CallUpdate>();
  final callSubscription = calls.events().listen((event) {
    if (event.conversationId != microsoftTestCallConversationId) return;
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
    final firstOutcome = await Future.any([
      connected.future,
      finished.future,
    ]).timeout(const Duration(seconds: 60));
    _require(
      firstOutcome.kind == CallUpdateKind.connected,
      'Test call ended before connecting.',
    );
    await calls.setMicrophoneEnabled(false);
    await calls.setMicrophoneEnabled(true);
    await calls.setSpeakerEnabled(false);
    await calls.setSpeakerEnabled(true);
    await Future<void>.delayed(const Duration(seconds: 12));
    await calls.hangUp();
    final terminal = await finished.future.timeout(const Duration(seconds: 30));
    _require(
      terminal.kind == CallUpdateKind.ended,
      terminal.detail ?? 'Test call failed.',
    );
    _require(
      parseMicrosoftTestCallResult(terminal.detail).passed,
      'Two-way test-call media failed.',
    );
  } finally {
    await calls.hangUp();
    await calls.stopEvents();
    await callSubscription.cancel();
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await initializePlatformBackend();
  try {
    await _run();
    _setBrowserResult(true);
    runApp(const MaterialApp(home: Center(child: Text('MICROSLOP_E2E_PASS'))));
  } catch (error, stackTrace) {
    _setBrowserResult(false);
    debugPrint('$error\n$stackTrace');
    runApp(
      MaterialApp(home: Center(child: Text('MICROSLOP_E2E_FAIL: $error'))),
    );
  }
}
