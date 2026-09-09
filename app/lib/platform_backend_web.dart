import 'auth_gateway.dart';
import 'call_gateway.dart';
import 'desktop_notifications.dart';
import 'teams_gateway.dart';
import 'web_bridge_gateway.dart';

Future<void> initializePlatformBackend() async {}

AuthGateway createPlatformAuthGateway() => WebBridgeAuthGateway();

TeamsGateway createPlatformTeamsGateway() => WebBridgeTeamsGateway();

CallGateway createPlatformCallGateway() => WebBridgeCallGateway();

DesktopNotifications createPlatformNotifications() => WebBridgeNotifications();
