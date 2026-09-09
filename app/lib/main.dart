import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import 'app_log.dart';
import 'auth_gateway.dart';
import 'call_gateway.dart';
import 'desktop_notifications.dart';
import 'platform_backend.dart';
import 'teams_gateway.dart';
import 'workspace.dart';

typedef ExternalUrlLauncher = Future<bool> Function(Uri uri);

Future<bool> _launchExternalUrl(Uri uri) =>
    launchUrl(uri, mode: LaunchMode.externalApplication);

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final flutterErrorHandler = FlutterError.onError;
  FlutterError.onError = (details) {
    if (details.library == 'services library' &&
        details.exceptionAsString().contains(
          'Attempted to send a key down event when no keys are in keysPressed',
        )) {
      return;
    }
    flutterErrorHandler?.call(details);
  };
  await initializePlatformBackend();
  runApp(const OstApp());
}

class OstApp extends StatelessWidget {
  const OstApp({
    super.key,
    this.gateway,
    this.teamsGateway,
    this.callGateway,
    this.notifications,
    this.externalUrlLauncher,
  });

  final AuthGateway? gateway;
  final TeamsGateway? teamsGateway;
  final CallGateway? callGateway;
  final DesktopNotifications? notifications;
  final ExternalUrlLauncher? externalUrlLauncher;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Microslop',
      debugShowCheckedModeBanner: false,
      theme: _appTheme(Brightness.light),
      darkTheme: _appTheme(Brightness.dark),
      themeMode: ThemeMode.system,
      home: AuthScreen(
        gateway: gateway ?? createPlatformAuthGateway(),
        teamsGateway: teamsGateway ?? createPlatformTeamsGateway(),
        callGateway:
            callGateway ??
            ((gateway != null || teamsGateway != null)
                ? const NoopCallGateway()
                : createPlatformCallGateway()),
        notifications:
            notifications ??
            ((gateway != null || teamsGateway != null)
                ? const NoopDesktopNotifications()
                : createPlatformNotifications()),
        externalUrlLauncher: externalUrlLauncher ?? _launchExternalUrl,
      ),
    );
  }
}

ThemeData _appTheme(Brightness brightness) {
  final scheme = ColorScheme.fromSeed(
    seedColor: const Color(0xFF6950A8),
    brightness: brightness,
  );
  return ThemeData(
    colorScheme: scheme,
    useMaterial3: true,
    fontFamily: 'Arial',
    scaffoldBackgroundColor: scheme.surface,
    appBarTheme: AppBarTheme(
      backgroundColor: scheme.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 1,
    ),
    cardTheme: const CardThemeData(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(24)),
      ),
      margin: EdgeInsets.zero,
      surfaceTintColor: Colors.transparent,
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: scheme.surfaceContainerHighest.withValues(alpha: .5),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(18),
        borderSide: BorderSide.none,
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(18),
        borderSide: BorderSide.none,
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(18),
        borderSide: BorderSide(color: scheme.primary),
      ),
      contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
    ),
  );
}

class AuthScreen extends StatefulWidget {
  const AuthScreen({
    super.key,
    required this.gateway,
    required this.teamsGateway,
    required this.callGateway,
    required this.notifications,
    required this.externalUrlLauncher,
  });

  final AuthGateway gateway;
  final TeamsGateway teamsGateway;
  final CallGateway callGateway;
  final DesktopNotifications notifications;
  final ExternalUrlLauncher externalUrlLauncher;

  @override
  State<AuthScreen> createState() => _AuthScreenState();
}

class _AuthScreenState extends State<AuthScreen> {
  Future<AuthSnapshot>? _status;
  DeviceCodeDetails? _deviceCode;
  String? _error;
  String? _activeAccountId;
  bool _authActionBusy = false;
  bool _completing = false;
  bool _workspaceActive = false;

  @override
  void initState() {
    super.initState();
    _status = _restoreWorkSession();
  }

  Future<AuthSnapshot> _restoreWorkSession() async {
    try {
      final status = await widget.gateway.restoreWorkSession();
      _workspaceActive = status.signedIn;
      if (status.signedIn) {
        await _refreshActiveAccountIdSafely(
          'Read active account after session restore',
        );
      }
      return status;
    } catch (error) {
      AppLog.record('Restore work session', error);
      rethrow;
    }
  }

  void _retryRestore() {
    setState(() => _status = _restoreWorkSession());
  }

  Future<void> _refreshActiveAccountId() async {
    final gateway = widget.gateway;
    _activeAccountId = gateway is MultiAccountAuthGateway
        ? await (gateway as MultiAccountAuthGateway).activeAccountId()
        : null;
  }

  Future<void> _refreshActiveAccountIdSafely(String operation) async {
    try {
      await _refreshActiveAccountId();
    } catch (error, stackTrace) {
      _activeAccountId = null;
      AppLog.record(operation, error, stackTrace);
    }
  }

  Future<List<SavedAccountSummary>> _savedAccounts() {
    final gateway = widget.gateway;
    return gateway is MultiAccountAuthGateway
        ? (gateway as MultiAccountAuthGateway).savedAccounts()
        : Future.value(const []);
  }

  Future<void> _stopRealtimeEvents() async {
    await Future.wait<void>([
      widget.teamsGateway.stopMessageEvents(),
      widget.callGateway.stopEvents(),
    ]);
  }

  Future<void> _stopRealtimeEventsSafely(String operation) async {
    try {
      await _stopRealtimeEvents();
    } catch (error, stackTrace) {
      AppLog.record(operation, error, stackTrace);
    }
  }

  Future<void> _switchAccount(String accountId) async {
    final gateway = widget.gateway;
    if (gateway is! MultiAccountAuthGateway || _authActionBusy) return;
    setState(() => _authActionBusy = true);
    try {
      final status = await (gateway as MultiAccountAuthGateway).switchAccount(
        accountId,
      );
      await _stopRealtimeEventsSafely(
        'Stop realtime events after account switch',
      );
      _workspaceActive = status.signedIn;
      _activeAccountId = status.signedIn ? accountId : null;
      if (!mounted) return;
      setState(() {
        _deviceCode = null;
        _error = null;
        _status = Future.value(status);
      });
    } catch (error) {
      AppLog.record('Switch Microsoft account', error);
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _authActionBusy = false);
    }
  }

  Future<void> _beginLogin({bool stopRealtimeEvents = false}) async {
    if (_authActionBusy) return;
    var shouldComplete = false;
    setState(() => _authActionBusy = true);
    try {
      final deviceCode = await widget.gateway.beginWorkLogin();
      final verificationUri = deviceCode.verificationUri.trim();
      final userCode = deviceCode.userCode.trim();
      if (verificationUri.isEmpty ||
          userCode.isEmpty ||
          deviceCode.expiresInSeconds <= BigInt.zero) {
        throw StateError('Microsoft returned an invalid device sign-in code.');
      }
      if (!mounted) return;
      if (stopRealtimeEvents) {
        await _stopRealtimeEventsSafely(
          'Stop realtime events before adding account',
        );
        _workspaceActive = false;
        if (!mounted) return;
      }
      setState(() {
        _deviceCode = DeviceCodeDetails(
          verificationUri: verificationUri,
          userCode: userCode,
          expiresInSeconds: deviceCode.expiresInSeconds,
        );
        _error = null;
        _status = Future.value(
          const AuthSnapshot(signedIn: false, loginInProgress: true),
        );
      });
      shouldComplete = true;
      await _copyDeviceCode(showConfirmation: false);
      await _openLoginPage();
    } catch (error) {
      AppLog.record('Start Microsoft sign-in', error);
      if (!mounted) return;
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _authActionBusy = false);
    }
    if (shouldComplete && mounted) {
      unawaited(_completeLogin());
    }
  }

  Future<void> _completeLogin() async {
    if (!mounted || _completing) return;
    setState(() => _completing = true);
    try {
      final status = await widget.gateway.completeWorkLogin();
      _workspaceActive = status.signedIn;
      if (status.signedIn) {
        await _refreshActiveAccountIdSafely(
          'Read active account after Microsoft sign-in',
        );
      }
      if (!mounted) return;
      setState(() {
        if (status.signedIn || !status.loginInProgress) {
          _deviceCode = null;
        }
        _status = Future.value(status);
        _error = status.signedIn || status.loginInProgress
            ? null
            : 'Microsoft sign-in was not completed.';
      });
    } catch (error) {
      AppLog.record('Complete Microsoft sign-in', error);
      if (!mounted || _deviceCode == null) return;
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _completing = false);
    }
  }

  Future<void> _cancelLogin() async {
    if (_authActionBusy) return;
    setState(() => _authActionBusy = true);
    try {
      await widget.gateway.cancelWorkLogin();
      final status = await widget.gateway.status();
      _workspaceActive = status.signedIn;
      if (status.signedIn) {
        await _refreshActiveAccountIdSafely(
          'Read active account after cancelling sign-in',
        );
      }
      if (!mounted) return;
      setState(() {
        _deviceCode = null;
        _error = null;
        _status = Future.value(status);
      });
    } catch (error) {
      AppLog.record('Cancel Microsoft sign-in', error);
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _authActionBusy = false);
    }
  }

  Future<void> _signOut() async {
    if (_authActionBusy) return;
    setState(() => _authActionBusy = true);
    try {
      final status = await widget.gateway.logout();
      await _stopRealtimeEventsSafely('Stop realtime events after sign out');
      _workspaceActive = false;
      if (!mounted) return;
      setState(() {
        _deviceCode = null;
        _error = null;
        _activeAccountId = null;
        _status = Future.value(status);
      });
    } catch (error) {
      AppLog.record('Sign out', error);
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _authActionBusy = false);
    }
  }

  @override
  void dispose() {
    if (_workspaceActive) {
      unawaited(_stopRealtimeEventsSafely('Stop realtime events on dispose'));
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<AuthSnapshot>(
      future: _status,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Scaffold(
            body: SafeArea(
              child: Center(
                child: FilledButton.icon(
                  onPressed: _retryRestore,
                  icon: const Icon(Icons.refresh),
                  label: const Text('Retry session restore'),
                ),
              ),
            ),
          );
        }
        if (!snapshot.hasData) {
          return const Scaffold(
            body: SafeArea(child: Center(child: CircularProgressIndicator())),
          );
        }
        if (snapshot.data!.signedIn) {
          return TeamsWorkspace(
            key: ValueKey(_activeAccountId ?? 'active-account'),
            gateway: widget.teamsGateway,
            callGateway: widget.callGateway,
            notifications: widget.notifications,
            activeAccountId: _activeAccountId,
            onListAccounts: _savedAccounts,
            onSwitchAccount: _switchAccount,
            onAddAccount: () => _beginLogin(stopRealtimeEvents: true),
            onSignOut: _signOut,
          );
        }
        return Scaffold(
          body: SafeArea(
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 520),
                child: Padding(
                  padding: const EdgeInsets.all(32),
                  child: Card(
                    child: Padding(
                      padding: const EdgeInsets.all(32),
                      child: _SignedOutPanel(
                        deviceCode: _deviceCode,
                        error: _error,
                        busy: _authActionBusy,
                        completing: _completing,
                        onBegin: _beginLogin,
                        onOpen: _openLoginPage,
                        onCopyCode: _copyDeviceCode,
                        onComplete: _completeLogin,
                        onCancel: _cancelLogin,
                        onCopyError: _copyError,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  Future<void> _openLoginPage() async {
    final verificationUri = _deviceCode?.verificationUri;
    final uri = verificationUri == null ? null : Uri.tryParse(verificationUri);
    if (uri == null || uri.scheme != 'https') {
      if (mounted) {
        setState(
          () => _error = 'Microsoft returned an invalid sign-in address.',
        );
      }
      return;
    }
    try {
      final opened = await widget.externalUrlLauncher(uri);
      if (!opened) {
        throw StateError('The system browser did not accept the sign-in URL.');
      }
    } catch (error, stackTrace) {
      AppLog.record('Open Microsoft sign-in', error, stackTrace);
      if (mounted) {
        setState(
          () => _error =
              'Could not open Microsoft sign-in automatically. The device code is ready to paste.',
        );
      }
    }
  }

  Future<void> _copyDeviceCode({bool showConfirmation = true}) async {
    final userCode = _deviceCode?.userCode;
    if (userCode == null) return;
    try {
      await Clipboard.setData(ClipboardData(text: userCode));
      if (!mounted || !showConfirmation) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Sign-in code copied to clipboard.')),
      );
    } catch (error, stackTrace) {
      AppLog.record('Copy Microsoft sign-in code', error, stackTrace);
    }
  }

  Future<void> _copyError() async {
    final error = _error;
    if (error == null) return;

    try {
      await Clipboard.setData(ClipboardData(text: error));
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Error copied to clipboard.')),
      );
    } catch (copyError) {
      AppLog.record('Copy sign-in error', copyError);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Could not copy error text: $copyError')),
      );
    }
  }
}

class _SignedOutPanel extends StatelessWidget {
  const _SignedOutPanel({
    required this.deviceCode,
    required this.error,
    required this.busy,
    required this.completing,
    required this.onBegin,
    required this.onOpen,
    required this.onCopyCode,
    required this.onComplete,
    required this.onCancel,
    required this.onCopyError,
  });

  final DeviceCodeDetails? deviceCode;
  final String? error;
  final bool busy;
  final bool completing;
  final Future<void> Function() onBegin;
  final Future<void> Function() onOpen;
  final Future<void> Function({bool showConfirmation}) onCopyCode;
  final Future<void> Function() onComplete;
  final Future<void> Function() onCancel;
  final Future<void> Function() onCopyError;

  @override
  Widget build(BuildContext context) {
    final deviceCode = this.deviceCode;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('Microslop', style: Theme.of(context).textTheme.headlineMedium),
        const SizedBox(height: 12),
        const Text('Sign in with your work or school account.'),
        const SizedBox(height: 24),
        if (deviceCode == null)
          FilledButton.icon(
            onPressed: busy ? null : onBegin,
            icon: const Icon(Icons.login),
            label: const Text('Start Microsoft sign-in'),
          )
        else ...[
          const Text(
            'Microsoft sign-in opened in your browser. The code is copied and ready to paste:',
          ),
          const SizedBox(height: 16),
          SelectableText(
            deviceCode.userCode,
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.displaySmall,
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            onPressed: busy ? null : onOpen,
            icon: const Icon(Icons.open_in_browser),
            label: const Text('Open Microsoft sign-in'),
          ),
          TextButton.icon(
            onPressed: busy ? null : onCopyCode,
            icon: const Icon(Icons.copy),
            label: const Text('Copy code'),
          ),
          const SizedBox(height: 8),
          FilledButton(
            onPressed: busy && !completing
                ? null
                : (completing ? null : onComplete),
            child: Text(
              completing ? 'Waiting for Microsoft…' : 'I have signed in',
            ),
          ),
          TextButton(
            onPressed: busy ? null : onCancel,
            child: const Text('Cancel'),
          ),
        ],
        if (error != null) ...[
          const SizedBox(height: 16),
          SelectableText(
            error!,
            style: TextStyle(color: Theme.of(context).colorScheme.error),
          ),
          Align(
            alignment: Alignment.centerRight,
            child: TextButton.icon(
              onPressed: onCopyError,
              icon: const Icon(Icons.copy),
              label: const Text('Copy error text'),
            ),
          ),
        ],
      ],
    );
  }
}
