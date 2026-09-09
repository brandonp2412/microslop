import 'dart:async';
import 'dart:io';
import 'dart:ui' show ImageByteFormat;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/main.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:microslop/src/rust/frb_generated.dart';

import 'widget_test.dart' show WidgetRustApi;
import 'package:shared_preferences/shared_preferences.dart';

class _StaticAuthGateway implements AuthGateway {
  const _StaticAuthGateway(this.signedIn);

  final bool signedIn;

  AuthSnapshot get _snapshot =>
      AuthSnapshot(signedIn: signedIn, loginInProgress: false);

  @override
  Future<DeviceCodeDetails> beginWorkLogin() async => DeviceCodeDetails(
    verificationUri: 'https://microsoft.com/devicelogin',
    userCode: 'ABCD-EFGH',
    expiresInSeconds: BigInt.from(900),
  );

  @override
  Future<void> cancelWorkLogin() async {}

  @override
  Future<AuthSnapshot> completeWorkLogin() async => _snapshot;

  @override
  Future<AuthSnapshot> logout() async =>
      const AuthSnapshot(signedIn: false, loginInProgress: false);

  @override
  Future<AuthSnapshot> restoreWorkSession() async => _snapshot;

  @override
  Future<AuthSnapshot> status() async => _snapshot;
}

class _StaticTeamsGateway implements TeamsGateway {
  static const ada = Conversation.chat(
    id: 'ada',
    name: 'Ada Lovelace',
    isGroup: false,
    profilePhotoUserId: 'ada-user',
    preview: 'The deployment is ready for review.',
  );
  static const grace = Conversation.chat(
    id: 'grace',
    name: 'Grace Hopper',
    isGroup: false,
    profilePhotoUserId: 'grace-user',
    preview: 'I found the regression.',
  );
  static const group = Conversation.chat(
    id: 'project-group',
    name: 'Project Phoenix',
    isGroup: true,
    avatarUserIds: ['ada-user', 'grace-user'],
    preview: 'Ada: Shipping after lunch.',
  );
  static const channel = Conversation.channel(
    id: 'engineering-general',
    name: 'General',
    teamId: 'engineering',
  );

  static const _messages = <String, List<MessageSummary>>{
    'ada': [
      MessageSummary(
        id: 'ada-1',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: '2026-09-02T09:41:00+12:00',
        content: 'The deployment is ready for review.',
        reactions: [MessageReaction(type: 'like', count: 2)],
      ),
      MessageSummary(
        id: 'ada-2',
        sender: 'You',
        senderId: 'current-user',
        isFromCurrentUser: true,
        timestamp: '2026-09-02T09:43:00+12:00',
        content: 'Checking it now. The new navigation feels much faster.',
      ),
      MessageSummary(
        id: 'ada-3',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: '2026-09-02T09:45:00+12:00',
        content: 'Good. I also tightened up the notification behaviour.',
        reactions: [MessageReaction(type: 'party-parrot', count: 1)],
      ),
    ],
    'grace': [
      MessageSummary(
        id: 'grace-1',
        sender: 'Grace Hopper',
        senderId: 'grace-user',
        timestamp: '2026-09-02T08:12:00+12:00',
        content:
            'I found the regression. It only happens after reopening a chat.',
      ),
    ],
    'project-group': [
      MessageSummary(
        id: 'group-1',
        sender: 'Ada Lovelace',
        senderId: 'ada-user',
        timestamp: '2026-09-02T10:02:00+12:00',
        content: 'Shipping after lunch.',
      ),
      MessageSummary(
        id: 'group-2',
        sender: 'Grace Hopper',
        senderId: 'grace-user',
        timestamp: '2026-09-02T10:04:00+12:00',
        content: 'I will keep an eye on the release metrics.',
      ),
    ],
    'engineering-general': [
      MessageSummary(
        id: 'channel-1',
        sender: 'Engineering Bot',
        senderId: 'bot',
        timestamp: '2026-09-02T08:00:00+12:00',
        content: 'Build 428 passed all required checks.',
      ),
    ],
  };

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async => null;

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => null;

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) async => null;

  @override
  Future<UserSummary> getUser() async => const UserSummary(
    id: 'current-user',
    displayName: 'brandonp2412',
    email: 'user@example.com',
  );

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async =>
      [group, ada, grace].take(limit).toList();

  @override
  Future<List<TeamSummary>> listTeams() async => const [
    TeamSummary(id: 'engineering', name: 'Engineering', channels: [channel]),
  ];

  @override
  Stream<MessageEvent> messageEvents() => const Stream.empty();

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      _messages[conversation.id] ?? const [];

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {}

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {}

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {}

  @override
  Future<void> stopMessageEvents() async {}
}

class _ScreenshotCallGateway implements CallGateway {
  final controller = StreamController<CallUpdate>.broadcast();

  @override
  Future<void> acceptCall(String callId) async {}

  @override
  Future<void> declineCall(String callId) async {}

  @override
  Stream<CallUpdate> events() => controller.stream;

  @override
  Future<void> hangUp() async {}

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {}

  @override
  Future<void> setSpeakerEnabled(bool enabled) async {}

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) async {}

  @override
  Future<void> stopEvents() async {}
}

Future<GlobalKey> _pumpApp(
  WidgetTester tester, {
  required Size size,
  required bool signedIn,
  Brightness brightness = Brightness.light,
  CallGateway callGateway = const NoopCallGateway(),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  tester.platformDispatcher.platformBrightnessTestValue = brightness;
  final boundaryKey = GlobalKey();
  await tester.pumpWidget(
    RepaintBoundary(
      key: boundaryKey,
      child: OstApp(
        gateway: _StaticAuthGateway(signedIn),
        teamsGateway: _StaticTeamsGateway(),
        callGateway: callGateway,
      ),
    ),
  );
  await tester.pumpAndSettle();
  return boundaryKey;
}

String _flutterRoot() {
  final configured = Platform.environment['FLUTTER_ROOT']?.trim();
  if (configured?.isNotEmpty == true) {
    return configured!;
  }

  final result = Process.runSync(Platform.isWindows ? 'where' : 'which', [
    'flutter',
  ]);
  if (result.exitCode != 0) {
    throw StateError('Unable to locate Flutter SDK for screenshot fonts.');
  }

  final executable = File(
    (result.stdout as String).split(RegExp(r'[\r\n]+')).first.trim(),
  ).resolveSymbolicLinksSync();
  return File(executable).parent.parent.path;
}

Future<void> _loadScreenshotFont() async {
  final configured = Platform.environment['MICROSLOP_SCREENSHOT_FONT'];
  final candidates = [
    if (configured?.trim().isNotEmpty == true) configured!.trim(),
    '/usr/share/fonts/liberation/LiberationSans-Regular.ttf',
    '/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf',
    '/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf',
    r'C:\Windows\Fonts\arial.ttf',
    r'C:\Windows\Fonts\segoeui.ttf',
  ];
  final path = candidates
      .where((candidate) => File(candidate).existsSync())
      .first;
  final bytes = await File(path).readAsBytes();
  await (FontLoader(
    'Arial',
  )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
}

Future<void> _loadEmojiFont() async {
  final candidates = <(String, String)>[
    ('/usr/share/fonts/noto/NotoColorEmoji.ttf', 'Noto Color Emoji'),
    ('/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf', 'Noto Color Emoji'),
    (r'C:\Windows\Fonts\seguiemj.ttf', 'Segoe UI Emoji'),
  ];
  for (final (path, family) in candidates) {
    final file = File(path);
    if (!file.existsSync()) continue;
    final bytes = await file.readAsBytes();
    await (FontLoader(
      family,
    )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
    return;
  }
}

Future<void> _loadMaterialIconsFont() async {
  final separator = Platform.pathSeparator;
  final path = [
    _flutterRoot(),
    'bin',
    'cache',
    'artifacts',
    'material_fonts',
    'MaterialIcons-Regular.otf',
  ].join(separator);
  final file = File(path);
  if (!file.existsSync()) {
    throw StateError('Material Icons font not found at $path.');
  }

  final bytes = await file.readAsBytes();
  await (FontLoader(
    'MaterialIcons',
  )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
}

Future<void> _capture(
  WidgetTester tester,
  GlobalKey boundaryKey,
  String name,
) async {
  await tester.pumpAndSettle();
  final boundary =
      boundaryKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1);
    final data = await image.toByteData(format: ImageByteFormat.png);
    image.dispose();
    final directory = Directory('build/ui-screenshots');
    await directory.create(recursive: true);
    await File('${directory.path}/$name.png').writeAsBytes(
      data!.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
    );
  });
}

void main() {
  RustLib.initMock(api: WidgetRustApi());
  const backgroundNotifications = MethodChannel(
    'microslop/background_notifications',
  );

  setUpAll(() async {
    await _loadScreenshotFont();
    await _loadEmojiFont();
    await _loadMaterialIconsFont();
    final directory = Directory('build/ui-screenshots');
    if (await directory.exists()) await directory.delete(recursive: true);
  });

  setUp(() {
    SharedPreferences.setMockInitialValues({
      'notifications.backgroundWatcher': false,
    });
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(backgroundNotifications, (call) async {
          if (call.method == 'isRunning') return false;
          return null;
        });
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(backgroundNotifications, null);
    TestWidgetsFlutterBinding.instance.platformDispatcher
        .clearPlatformBrightnessTestValue();
  });

  testWidgets('captures signed-out desktop UI', (tester) async {
    final boundary = await _pumpApp(
      tester,
      size: const Size(1280, 800),
      signedIn: false,
    );
    await _capture(tester, boundary, 'desktop-signed-out-light');
  });

  testWidgets('captures desktop workspace, chat, settings, and dark mode', (
    tester,
  ) async {
    final boundary = await _pumpApp(
      tester,
      size: const Size(1280, 800),
      signedIn: true,
    );
    await _capture(tester, boundary, 'desktop-workspace-light');

    await tester.tap(find.byKey(const ValueKey('conversation-tile-chat-ada')));
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'desktop-direct-message-light');

    await tester.tap(find.text('Settings').first);
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'desktop-settings-light');

    await tester.pageBack();
    await tester.pumpAndSettle();
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'desktop-direct-message-dark');
  });

  testWidgets('captures mobile navigation and conversation UI', (tester) async {
    final boundary = await _pumpApp(
      tester,
      size: const Size(390, 844),
      signedIn: true,
    );

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'mobile-navigation-light');

    await tester.tap(find.byKey(const ValueKey('conversation-tile-chat-ada')));
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'mobile-direct-message-light');
  });

  testWidgets('captures incoming video call UI', (tester) async {
    final calls = _ScreenshotCallGateway();
    addTearDown(calls.controller.close);
    final boundary = await _pumpApp(
      tester,
      size: const Size(390, 844),
      signedIn: true,
      brightness: Brightness.dark,
      callGateway: calls,
    );
    calls.controller.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'screenshot-call',
        displayName: 'Ada Lovelace',
        detail: 'video=true',
      ),
    );
    await tester.pumpAndSettle();
    await _capture(tester, boundary, 'incoming-video-dark');
  });
}
