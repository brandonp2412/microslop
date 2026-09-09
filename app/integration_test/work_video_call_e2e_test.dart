import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

const _targetName = String.fromEnvironment('MICROSLOP_E2E_CALL_TARGET');

Map<String, String> _fields(String? detail) {
  final values = <String, String>{};
  for (final field in (detail ?? '').split(';').skip(1)) {
    final separator = field.indexOf('=');
    if (separator > 0) {
      values[field.substring(0, separator)] = field.substring(separator + 1);
    }
  }
  return values;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(initializePlatformBackend);

  testWidgets('work-account video transport receives RTCP reports', (
    tester,
  ) async {
    expect(
      _targetName,
      isNotEmpty,
      reason: 'Set MICROSLOP_E2E_CALL_TARGET with --dart-define.',
    );

    final auth = createPlatformAuthGateway();
    final teams = createPlatformTeamsGateway();
    final calls = createPlatformCallGateway();
    final session = await auth.restoreWorkSession();
    expect(
      session.signedIn,
      isTrue,
      reason: 'A cached work session is required.',
    );

    final chats = await teams.listChats(limit: 200);
    final matches = chats.where(
      (chat) => !chat.isGroup && chat.name.trim() == _targetName.trim(),
    );
    expect(matches, hasLength(1));
    final target = matches.single;
    final calleeUserId =
        target.profilePhotoUserId ??
        (target.avatarUserIds.isNotEmpty ? target.avatarUserIds.first : null);
    expect(
      calleeUserId,
      isNotNull,
      reason: 'Could not resolve target user ID.',
    );

    final connected = Completer<CallUpdate>();
    final finished = Completer<CallUpdate>();
    final subscription = calls.events().listen((event) {
      if (event.conversationId != target.id) return;
      // ignore: avoid_print
      print('WORK_VIDEO_EVENT kind=${event.kind.name} detail=${event.detail}');
      if (event.kind == CallUpdateKind.connected && !connected.isCompleted) {
        connected.complete(event);
      }
      if ((event.kind == CallUpdateKind.ended ||
              event.kind == CallUpdateKind.error) &&
          !finished.isCompleted) {
        finished.complete(event);
      }
    });

    try {
      await calls.startCall(target.id, video: true, calleeUserId: calleeUserId);
      final first = await Future.any([
        connected.future,
        finished.future,
      ]).timeout(const Duration(seconds: 75));
      expect(first.kind, CallUpdateKind.connected, reason: first.detail);

      await Future<void>.delayed(const Duration(seconds: 25));
      await calls.hangUp();
      final terminal = await finished.future.timeout(
        const Duration(seconds: 30),
      );
      expect(terminal.kind, CallUpdateKind.ended, reason: terminal.detail);

      final values = _fields(terminal.detail);
      final sent = int.tryParse(values['video_sent'] ?? '') ?? 0;
      final srtcp = int.tryParse(values['video_srtcp'] ?? '') ?? 0;
      final receiverReports =
          int.tryParse(values['receiver_reports'] ?? '') ?? 0;
      final cameraFrames = int.tryParse(values['camera_frames'] ?? '') ?? 0;
      expect(cameraFrames, greaterThan(0));
      expect(sent, greaterThan(0));
      expect(
        srtcp,
        greaterThan(0),
        reason: 'No receiver-originated video RTCP reached Microslop.',
      );
      expect(
        receiverReports,
        greaterThan(0),
        reason: 'Remote Teams never reported reception of our video SSRC.',
      );
    } finally {
      await calls.hangUp();
      await calls.stopEvents();
      await subscription.cancel();
    }
  });
}
