import 'package:flutter/widgets.dart';
import 'package:microslop/platform_backend.dart';
import 'package:microslop/teams_gateway.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await initializePlatformBackend();

  final auth = createPlatformAuthGateway();
  final session = await auth.restoreWorkSession();
  if (!session.signedIn) throw StateError('No cached work session');

  final teams = createPlatformTeamsGateway();
  if (teams is! PresenceTeamsGateway) {
    throw StateError('Platform gateway does not support presence');
  }
  final currentUser = await teams.getUser();
  final chats = await teams.listChats(limit: 100);
  final directChats = chats.where(
    (chat) =>
        !chat.isGroup && chat.teamId == null && chat.profilePhotoUserId != null,
  );
  final userIds = directChats
      .map((chat) => chat.profilePhotoUserId!)
      .where((userId) => userId != currentUser.id)
      .toSet()
      .take(20)
      .toList(growable: false);
  if (userIds.isEmpty) throw StateError('No direct-message users found');

  final presences = await (teams as PresenceTeamsGateway).getPresences(userIds);
  if (presences.isEmpty) throw StateError('No teammate presence returned');
  for (final presence in presences) {
    debugPrint(
      'PRESENCE_PROBE|${presence.userId}|${presence.availability}|${presence.activity}',
    );
  }
}
