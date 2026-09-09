import 'dart:async';

import 'src/rust/api/auth.dart' as rust;

typedef SessionRestorer = Future<rust.AuthStatus> Function();

class AuthSnapshot {
  const AuthSnapshot({required this.signedIn, required this.loginInProgress});

  final bool signedIn;
  final bool loginInProgress;
}

class DeviceCodeDetails {
  const DeviceCodeDetails({
    required this.verificationUri,
    required this.userCode,
    required this.expiresInSeconds,
  });

  final String verificationUri;
  final String userCode;
  final BigInt expiresInSeconds;
}

class SavedAccountSummary {
  const SavedAccountSummary({
    required this.id,
    required this.displayName,
    required this.username,
  });

  final String id;
  final String displayName;
  final String username;
}

abstract interface class AuthGateway {
  Future<AuthSnapshot> status();
  Future<AuthSnapshot> restoreWorkSession();
  Future<DeviceCodeDetails> beginWorkLogin();
  Future<AuthSnapshot> completeWorkLogin();
  Future<void> cancelWorkLogin();
  Future<AuthSnapshot> logout();
}

abstract interface class MultiAccountAuthGateway {
  Future<List<SavedAccountSummary>> savedAccounts();
  Future<String?> activeAccountId();
  Future<AuthSnapshot> switchAccount(String accountId);
}

class RustAuthGateway implements AuthGateway, MultiAccountAuthGateway {
  RustAuthGateway({SessionRestorer? sessionRestorer, Duration? restoreTimeout})
    : _sessionRestorer = sessionRestorer ?? rust.restoreWorkSession,
      _restoreTimeout = restoreTimeout ?? const Duration(seconds: 15);

  final SessionRestorer _sessionRestorer;
  final Duration _restoreTimeout;

  @override
  Future<AuthSnapshot> status() async {
    final status = await rust.getAuthStatus();
    return AuthSnapshot(
      signedIn: status.signedIn,
      loginInProgress: status.loginInProgress,
    );
  }

  @override
  Future<AuthSnapshot> restoreWorkSession() async {
    final status = await _sessionRestorer().timeout(_restoreTimeout);
    return AuthSnapshot(
      signedIn: status.signedIn,
      loginInProgress: status.loginInProgress,
    );
  }

  @override
  Future<List<SavedAccountSummary>> savedAccounts() async => [
    for (final account in await rust.listSavedAccounts())
      SavedAccountSummary(
        id: account.id,
        displayName: account.displayName,
        username: account.username,
      ),
  ];

  @override
  Future<String?> activeAccountId() => rust.activeAccountId();

  @override
  Future<AuthSnapshot> switchAccount(String accountId) async {
    final status = await rust.switchSavedAccount(accountId: accountId);
    return AuthSnapshot(
      signedIn: status.signedIn,
      loginInProgress: status.loginInProgress,
    );
  }

  @override
  Future<DeviceCodeDetails> beginWorkLogin() async {
    final code = await rust.beginWorkLogin();
    return DeviceCodeDetails(
      verificationUri: code.verificationUri,
      userCode: code.userCode,
      expiresInSeconds: code.expiresInSeconds,
    );
  }

  @override
  Future<AuthSnapshot> completeWorkLogin() async {
    final status = await rust.completeWorkLogin();
    return AuthSnapshot(
      signedIn: status.signedIn,
      loginInProgress: status.loginInProgress,
    );
  }

  @override
  Future<void> cancelWorkLogin() => rust.cancelWorkLogin();

  @override
  Future<AuthSnapshot> logout() async {
    final status = await rust.logout();
    return AuthSnapshot(
      signedIn: status.signedIn,
      loginInProgress: status.loginInProgress,
    );
  }
}
