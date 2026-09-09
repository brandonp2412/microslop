import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' show ImageByteFormat, PointerDeviceKind;

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/app_log.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/desktop_notifications.dart';
import 'package:microslop/main.dart';
import 'package:microslop/message_clipboard.dart';
import 'package:microslop/src/rust/api/auth.dart' as auth_rust;
import 'package:microslop/src/rust/api/teams.dart' as rust;
import 'package:microslop/src/rust/api/calls.dart' as calls_rust;
import 'package:microslop/src/rust/frb_generated.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:microslop/workspace_cache.dart';
import 'package:shared_preferences/shared_preferences.dart';

part 'widget_test/workspace_tests.dart';
part 'widget_test/conversation_tests.dart';
part 'widget_test/notification_reaction_tests.dart';
part 'widget_test/call_tests.dart';
part 'widget_test/avatar_channel_tests.dart';
part 'widget_test/gateway_fakes.dart';
part 'widget_test/message_gateway_fakes.dart';
part 'widget_test/call_notification_fakes.dart';

Finder selectableMessage(String text) => find.byWidgetPredicate(
  (widget) => widget is SelectableText && widget.data == text,
  description: 'selectable message "$text"',
);

Future<void> captureUiScreenshot(
  WidgetTester tester,
  GlobalKey boundaryKey,
  String name,
) async {
  await tester.pumpAndSettle();
  final boundary =
      boundaryKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ImageByteFormat.png);
    image.dispose();
    final directory = Directory('build/ui-screenshots');
    await directory.create(recursive: true);
    await File(
      '${directory.path}/$name.png',
    ).writeAsBytes(bytes!.buffer.asUint8List());
  });
}

void main() {
  RustLib.initMock(api: WidgetRustApi());
  PackageInfo.setMockInitialValues(
    appName: 'Microslop',
    packageName: 'com.microslop.app',
    version: '1.2.3',
    buildNumber: '42',
    buildSignature: '',
  );
  test('rejects custom reaction names longer than 64 characters', () {
    expect(
      () => validateReactionType(List.filled(65, 'x').join()),
      throwsA(isA<ArgumentError>()),
    );
    expect(validateReactionType(' like '), 'like');
  });

  test('maps server reaction types to visual reactions', () {
    expect(reactionEmoji('like'), '👍');
    expect(reactionEmoji('1f9e0_brain'), '🧠');
    expect(reactionEmoji('party-parrot;0-sau-d4-asset'), '🎉');
    expect(reactionEmoji('dumpsterfire;0-sau-d1-asset'), '🔥');
    expect(reactionEmoji('cat-roomba-ultra-fast;0-eau-d3-asset'), '🐈');
    expect(reactionEmoji('approved-with-comments;0-eau-d1-asset'), '✅');
    expect(reactionEmoji('unmapped-custom-reaction;0-eau-d1-asset'), '✨');
  });

  test('parses test call video diagnostics', () {
    final result = parseMicrosoftTestCallResult(
      'test-call;video_sent=42;video_received=17',
    );

    expect(result.videoPacketsSent, 42);
    expect(result.videoPacketsReceived, 17);
  });

  const backgroundNotifications = MethodChannel(
    'microslop/background_notifications',
  );
  setUp(() {
    SharedPreferences.setMockInitialValues({});
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          backgroundNotifications,
          (call) async => call.method == 'isRunning' ? false : null,
        );
  });
  tearDown(() {
    MessageClipboard.debugRead = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(backgroundNotifications, null);
  });

  registerWorkspaceTests();
  registerConversationTests();
  registerNotificationReactionTests();
  registerCallTests();
  registerAvatarChannelTests();
}

class WidgetRustApi extends RustLibApi {
  @override
  Future<void> crateApiCallsStartCall({required String conversationId}) async {
    expect(conversationId, startsWith('__microslop_call_ringback_'));
  }

  @override
  Future<calls_rust.RemoteVideoFrame?> crateApiCallsLocalVideoFrame() async =>
      null;

  @override
  Future<calls_rust.RemoteVideoFrame?> crateApiCallsRemoteVideoFrame() async =>
      null;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}
