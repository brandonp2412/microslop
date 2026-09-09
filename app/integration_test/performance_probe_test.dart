import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/foundation.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/src/rust/frb_generated.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:microslop/workspace_database.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('probe responsive conversation latency', (tester) async {
    final auth = RustAuthGateway(restoreTimeout: const Duration(minutes: 3));
    final session = await auth.restoreWorkSession();
    expect(session.signedIn, isTrue);

    final gateway = RustTeamsGateway(chatTimeout: const Duration(seconds: 90));
    final chats = await gateway.listChats(limit: 50);
    final teams = await gateway.listTeams();
    final samples = <Conversation>[
      if (chats.where((chat) => !chat.isGroup).firstOrNull case final chat?)
        chat,
      if (chats.where((chat) => chat.isGroup).firstOrNull case final group?)
        group,
      if (teams.expand((team) => team.channels).firstOrNull case final channel?)
        channel,
    ];
    expect(samples, isNotEmpty);

    final database = WorkspaceDatabase();
    final databaseOpen = Stopwatch()..start();
    await database.initialize();
    databaseOpen.stop();
    debugPrint('PERF_DB_OPEN|${databaseOpen.elapsedMilliseconds}ms');

    for (final conversation in samples) {
      final textWatch = Stopwatch()..start();
      final textMessages = await gateway.refreshMessages(conversation);
      textWatch.stop();
      debugPrint(
        'PERF_TEXT|${conversation.kind.name}|${conversation.name}|'
        '${textWatch.elapsedMilliseconds}ms|${textMessages.length}',
      );

      final saveWatch = Stopwatch()..start();
      await database.saveMessages(
        conversation,
        textMessages,
        replaceImages: false,
      );
      saveWatch.stop();
      final loadWatch = Stopwatch()..start();
      final stored = await database.loadMessages(conversation);
      loadWatch.stop();
      debugPrint(
        'PERF_DB|${conversation.kind.name}|${conversation.name}|'
        'save=${saveWatch.elapsedMilliseconds}ms|'
        'load=${loadWatch.elapsedMilliseconds}ms|${stored.length}',
      );

      final imageWatch = Stopwatch()..start();
      final hydrated = await gateway.hydrateRecentImages(conversation);
      imageWatch.stop();
      final imageCount = hydrated.fold<int>(
        0,
        (count, message) => count + message.images.length,
      );
      debugPrint(
        'PERF_IMAGES|${conversation.kind.name}|${conversation.name}|'
        '${imageWatch.elapsedMilliseconds}ms|messages=${hydrated.length}|'
        'images=$imageCount',
      );
    }
    await database.close();
  });
}
