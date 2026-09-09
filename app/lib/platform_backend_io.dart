import 'auth_gateway.dart';
import 'call_gateway.dart';
import 'desktop_notifications.dart';
import 'src/rust/frb_generated.dart';
import 'teams_gateway.dart';

Future<void> initializePlatformBackend() => RustLib.init();

AuthGateway createPlatformAuthGateway() => RustAuthGateway();

TeamsGateway createPlatformTeamsGateway() => RustTeamsGateway();

CallGateway createPlatformCallGateway() => RustCallGateway();

DesktopNotifications createPlatformNotifications() =>
    FlutterDesktopNotifications();
