import 'package:flutter/widgets.dart';
import 'package:microslop/platform_backend.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await initializePlatformBackend();

  final auth = createPlatformAuthGateway();
  final session = await auth.restoreWorkSession();
  if (!session.signedIn) throw StateError('No cached work session');

  final teams = createPlatformTeamsGateway();
  final chats = await teams.listChats(limit: 100);
  final chat = chats.singleWhere(
    (candidate) => candidate.name.trim().toLowerCase() == 'bingo bus',
  );
  final photo = await teams.getChatPhoto(chat.id);
  debugPrint(
    'AVATAR_PROBE|${chat.name}|${chat.id}|bytes=${photo?.length ?? 0}',
  );
  if (photo == null || photo.isEmpty) {
    throw StateError('Known custom group avatar did not load');
  }
}
