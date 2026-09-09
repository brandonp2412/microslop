import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'app_log.dart';

class BackgroundMessageWatcher {
  BackgroundMessageWatcher._();

  static const _channel = MethodChannel('microslop/background_notifications');

  static bool get supported =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.android;

  static Future<void> setEnabled(bool enabled) async {
    if (!supported) return;
    await _channel.invokeMethod<void>('setEnabled', {'enabled': enabled});
  }

  static Future<bool> isRunning() async {
    if (!supported) return false;
    return await _channel.invokeMethod<bool>('isRunning') ?? false;
  }
}

abstract interface class DesktopNotifications {
  Stream<String> get conversationSelections;
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  });
}

abstract interface class ConversationDesktopNotifications {
  Future<void> showConversation({
    required String title,
    required String body,
    required String conversationId,
    required String senderName,
    Uint8List? senderAvatar,
    required bool groupConversation,
  });
  Future<void> dismissConversation(String conversationId);
}

class NoopDesktopNotifications implements DesktopNotifications {
  const NoopDesktopNotifications();

  @override
  Stream<String> get conversationSelections => const Stream.empty();

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {}
}

class NotificationHistory {
  static final entries = ValueNotifier<List<String>>([]);

  static void add(String title, String body) {
    final next = List<String>.from(entries.value)
      ..add('${DateTime.now().toIso8601String()} | $title | $body');
    entries.value = next.length > 100 ? next.sublist(next.length - 100) : next;
  }
}

const _notificationAvatarSize = 96;

@visibleForTesting
Future<Uint8List?> createCircularNotificationAvatar(
  Uint8List? avatar, {
  Uint8List? badge,
}) async {
  if (avatar == null || avatar.isEmpty) return null;
  ui.Codec? codec;
  ui.Image? source;
  ui.Codec? badgeCodec;
  ui.Image? badgeImage;
  ui.PictureRecorder? recorder;
  ui.Picture? picture;
  ui.Image? rendered;
  try {
    codec = await ui.instantiateImageCodec(avatar);
    source = (await codec.getNextFrame()).image;
    final sourceSide = source.width < source.height
        ? source.width
        : source.height;
    final sourceRect = ui.Rect.fromLTWH(
      (source.width - sourceSide) / 2,
      (source.height - sourceSide) / 2,
      sourceSide.toDouble(),
      sourceSide.toDouble(),
    );
    final destinationRect = ui.Rect.fromLTWH(
      0,
      0,
      _notificationAvatarSize.toDouble(),
      _notificationAvatarSize.toDouble(),
    );
    recorder = ui.PictureRecorder();
    final canvas = ui.Canvas(recorder);
    canvas.save();
    canvas.clipPath(ui.Path()..addOval(destinationRect), doAntiAlias: true);
    canvas.drawImageRect(
      source,
      sourceRect,
      destinationRect,
      ui.Paint()
        ..isAntiAlias = true
        ..filterQuality = ui.FilterQuality.high,
    );
    canvas.restore();
    if (badge != null) {
      badgeCodec = await ui.instantiateImageCodec(badge);
      badgeImage = (await badgeCodec.getNextFrame()).image;
      canvas.drawImageRect(
        badgeImage,
        ui.Rect.fromLTWH(
          0,
          0,
          badgeImage.width.toDouble(),
          badgeImage.height.toDouble(),
        ),
        const ui.Rect.fromLTWH(64, 64, 32, 32),
        ui.Paint()..filterQuality = ui.FilterQuality.high,
      );
    }
    picture = recorder.endRecording();
    rendered = await picture.toImage(
      _notificationAvatarSize,
      _notificationAvatarSize,
    );
    final bytes = await rendered.toByteData(format: ui.ImageByteFormat.png);
    return bytes?.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes);
  } catch (error, stackTrace) {
    AppLog.record(
      'Notifications: could not prepare circular sender avatar',
      error,
      stackTrace,
    );
    return null;
  } finally {
    rendered?.dispose();
    picture?.dispose();
    if (recorder?.isRecording == true) recorder!.endRecording().dispose();
    badgeImage?.dispose();
    badgeCodec?.dispose();
    source?.dispose();
    codec?.dispose();
  }
}

class FlutterDesktopNotifications
    implements DesktopNotifications, ConversationDesktopNotifications {
  static const _androidBridge = MethodChannel(
    'microslop/background_notifications',
  );
  static const _androidChannel = AndroidNotificationChannel(
    'microslop_messages',
    'Microslop messages',
    description: 'Notifications for Microslop message activity',
    importance: Importance.high,
  );
  final _notifications = FlutterLocalNotificationsPlugin();
  late final _conversationSelections = StreamController<String>.broadcast(
    onListen: _flushPendingSelection,
  );
  late final Future<void> _initialised = _initialize();
  final _conversationMessages = <String, List<Message>>{};
  final _legacyGroupsCleaned = <String>{};
  String? _pendingSelection;
  var _nextId = DateTime.now().millisecondsSinceEpoch & 0x3fffffff;

  @override
  Stream<String> get conversationSelections {
    unawaited(_initialised);
    return _conversationSelections.stream;
  }

  void _flushPendingSelection() {
    final pendingSelection = _pendingSelection;
    if (pendingSelection == null || !_conversationSelections.hasListener) {
      return;
    }
    _pendingSelection = null;
    scheduleMicrotask(() => _conversationSelections.add(pendingSelection));
  }

  void _selectConversation(String? payload) {
    final conversationId = payload?.trim();
    if (conversationId == null || conversationId.isEmpty) return;
    if (_conversationSelections.hasListener) {
      _conversationSelections.add(conversationId);
    } else {
      _pendingSelection = conversationId;
    }
  }

  Future<void> _initialize() async {
    AppLog.info(
      'Notifications',
      'Initialising platform=${Platform.operatingSystem}',
    );
    final initialized = await _notifications.initialize(
      settings: InitializationSettings(
        android: const AndroidInitializationSettings('@mipmap/ic_launcher'),
        linux: LinuxInitializationSettings(defaultActionName: 'Open Microslop'),
        windows: WindowsInitializationSettings(
          appName: 'Microslop',
          appUserModelId: 'com.microslop.app',
          guid: 'f76699ce-1954-4e83-b342-2a7ad034bd64',
        ),
      ),
      onDidReceiveNotificationResponse: (response) {
        _selectConversation(response.payload);
      },
    );
    if (initialized != true) {
      throw StateError('Notification initialisation was declined.');
    }
    if (Platform.isAndroid) {
      final android = _notifications
          .resolvePlatformSpecificImplementation<
            AndroidFlutterLocalNotificationsPlugin
          >();
      if (android == null) {
        throw StateError('Android notification support is unavailable.');
      }
      AppLog.info('Notifications', 'Creating Android notification channel');
      await android.createNotificationChannel(_androidChannel);
      final enabledBeforeRequest = await android.areNotificationsEnabled();
      AppLog.info(
        'Notifications',
        'Android permission enabledBeforeRequest=$enabledBeforeRequest',
      );
      final enabled = enabledBeforeRequest == true
          ? true
          : await android.requestNotificationsPermission() == true;
      AppLog.info(
        'Notifications',
        'Android permission enabledAfterRequest=$enabled',
      );
      if (!enabled) {
        throw StateError(
          'Notification permission is disabled. Enable it in Android settings.',
        );
      }
    }
    final launchDetails = await _notifications
        .getNotificationAppLaunchDetails();
    if (launchDetails?.didNotificationLaunchApp == true) {
      _selectConversation(launchDetails?.notificationResponse?.payload);
    }
    _flushPendingSelection();
    AppLog.info('Notifications', 'Notification support is ready');
  }

  String _legacyGroupKey(String conversationId) =>
      'microslop.conversation.${conversationId.trim()}';

  String _conversationTag(String conversationId) =>
      'microslop.conversation.${conversationId.trim()}.notification';

  int _conversationNotificationId(String conversationId) {
    var hash = 0;
    for (final value in conversationId.trim().codeUnits) {
      hash = (hash * 31 + value) & 0x1fffffff;
    }
    return 0x40000000 | hash;
  }

  String _conversationShortcutId(String conversationId) =>
      'microslop-${_conversationNotificationId(conversationId)}';

  Future<String?> _publishConversationShortcut({
    required String conversationId,
    required String title,
    required String senderName,
    required Uint8List? avatar,
  }) async {
    if (avatar == null) return null;
    final shortcutId = _conversationShortcutId(conversationId);
    try {
      final published = await _androidBridge
          .invokeMethod<bool>('upsertConversationShortcut', {
            'shortcutId': shortcutId,
            'conversationId': conversationId,
            'label': title,
            'senderName': senderName,
            'avatar': avatar,
          });
      return published == true ? shortcutId : null;
    } catch (error, stackTrace) {
      AppLog.record('Publish conversation shortcut', error, stackTrace);
      return null;
    }
  }

  Future<void> _cancelLegacyConversationGroup(
    String conversationId, {
    bool once = false,
  }) async {
    final normalized = conversationId.trim();
    if (once && !_legacyGroupsCleaned.add(normalized)) return;
    final groupKey = _legacyGroupKey(normalized);
    final active = await _notifications.getActiveNotifications();
    for (final notification in active.where(
      (notification) => notification.groupKey == groupKey,
    )) {
      final id = notification.id;
      if (id != null) {
        await _notifications.cancel(id: id, tag: notification.tag);
      }
    }
  }

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {
    await _initialised;
    Uri? logoUri;
    if (Platform.isWindows) {
      final directory = await getApplicationSupportDirectory();
      final file = File('${directory.path}/notification_logo.png');
      final logo = await rootBundle.load('assets/icon/app_icon.png');
      await file.writeAsBytes(logo.buffer.asUint8List(), flush: true);
      logoUri = file.uri;
    }
    final id = _nextId++;
    AppLog.info(
      'Notifications',
      'Submitting notification id=$id platform=${Platform.operatingSystem}',
    );
    await _notifications.show(
      id: id,
      title: title,
      body: body,
      notificationDetails: NotificationDetails(
        android: const AndroidNotificationDetails(
          'microslop_messages',
          'Microslop messages',
          channelDescription: 'Notifications for Microslop message activity',
          importance: Importance.high,
          priority: Priority.high,
        ),
        linux: const LinuxNotificationDetails(),
        windows: WindowsNotificationDetails(
          images: [
            if (logoUri != null)
              WindowsImage(
                logoUri,
                altText: 'Microslop',
                placement: WindowsImagePlacement.appLogoOverride,
              ),
          ],
        ),
      ),
      payload: conversationId,
    );
    NotificationHistory.add(title, body);
    AppLog.info(
      'Notifications',
      'Platform accepted notification id=$id for display',
    );
  }

  @override
  Future<void> showConversation({
    required String title,
    required String body,
    required String conversationId,
    required String senderName,
    Uint8List? senderAvatar,
    required bool groupConversation,
  }) async {
    await _initialised;
    if (Platform.isWindows) {
      final logo = (await rootBundle.load(
        'assets/icon/app_icon.png',
      )).buffer.asUint8List();
      final avatar =
          await createCircularNotificationAvatar(senderAvatar, badge: logo) ??
          logo;
      final directory = await getApplicationSupportDirectory();
      final avatarDirectory = Directory(
        '${directory.path}/notification_avatars',
      );
      await avatarDirectory.create(recursive: true);
      final file = File(
        '${avatarDirectory.path}/${_conversationNotificationId(conversationId)}.png',
      );
      await file.writeAsBytes(avatar, flush: true);
      await _notifications.show(
        id: _conversationNotificationId(conversationId),
        title: title,
        body: body,
        notificationDetails: NotificationDetails(
          windows: WindowsNotificationDetails(
            images: [
              WindowsImage(
                file.uri,
                altText: senderName,
                placement: WindowsImagePlacement.appLogoOverride,
              ),
            ],
          ),
        ),
        payload: conversationId,
      );
      NotificationHistory.add(title, body);
      return;
    }
    if (!Platform.isAndroid) {
      return show(title: title, body: body, conversationId: conversationId);
    }
    final normalizedConversationId = conversationId.trim();
    await _cancelLegacyConversationGroup(normalizedConversationId, once: true);
    final notificationAvatar = await createCircularNotificationAvatar(
      senderAvatar,
    );
    final sender = Person(
      key: senderName,
      name: senderName,
      icon: notificationAvatar == null
          ? null
          : ByteArrayAndroidIcon(notificationAvatar),
    );
    final shortcutId = await _publishConversationShortcut(
      conversationId: normalizedConversationId,
      title: title,
      senderName: senderName,
      avatar: notificationAvatar,
    );
    final messages = _conversationMessages.putIfAbsent(
      normalizedConversationId,
      () => <Message>[],
    )..add(Message(body, DateTime.now(), sender));
    if (messages.length > 8) {
      messages.removeRange(0, messages.length - 8);
    }
    final details = NotificationDetails(
      android: AndroidNotificationDetails(
        'microslop_messages',
        'Microslop messages',
        channelDescription: 'Notifications for Microslop message activity',
        importance: Importance.high,
        priority: Priority.high,
        category: AndroidNotificationCategory.message,
        tag: _conversationTag(normalizedConversationId),
        shortcutId: shortcutId,
        largeIcon: notificationAvatar == null
            ? null
            : ByteArrayAndroidBitmap(notificationAvatar),
        styleInformation: MessagingStyleInformation(
          const Person(name: 'You'),
          conversationTitle: title,
          groupConversation: groupConversation,
          messages: List<Message>.unmodifiable(messages),
        ),
      ),
    );
    await _notifications.show(
      id: _conversationNotificationId(normalizedConversationId),
      title: title,
      body: body,
      notificationDetails: details,
      payload: normalizedConversationId,
    );
    NotificationHistory.add(title, body);
  }

  @override
  Future<void> dismissConversation(String conversationId) async {
    await _initialised;
    if (!Platform.isAndroid) return;
    final normalizedConversationId = conversationId.trim();
    _conversationMessages.remove(normalizedConversationId);
    await _notifications.cancel(
      id: _conversationNotificationId(normalizedConversationId),
      tag: _conversationTag(normalizedConversationId),
    );
    await _cancelLegacyConversationGroup(normalizedConversationId);
  }
}
